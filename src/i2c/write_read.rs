//! I2C write_read implementation

use super::{commands, Error, State};

use core::{
    future::Future,
    marker::PhantomPinned,
    pin,
    task::{self, Poll},
};

/// An I2C write-read future
///
/// Use [`write_read`](crate::I2C::write_read) to create this future.
pub struct WriteRead<'a, SCL, SDA> {
    i2c: &'a mut super::I2C<SCL, SDA>,
    address: u8,
    output: &'a [u8],
    input: &'a mut [u8],
    _pin: PhantomPinned,
}

impl<'a, SCL, SDA> WriteRead<'a, SCL, SDA> {
    pub(super) fn new(
        i2c: &'a mut super::I2C<SCL, SDA>,
        address: u8,
        output: &'a [u8],
        input: &'a mut [u8],
    ) -> Self {
        WriteRead {
            i2c,
            address,
            output,
            input,
            _pin: PhantomPinned,
        }
    }
}

impl<SCL: Unpin, SDA: Unpin> Future for WriteRead<'_, SCL, SDA> {
    type Output = Result<(), Error>;

    fn poll(self: pin::Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        // Safety: keeping all memory pinned; calling poll operation with same arguments
        // until completion.
        unsafe {
            let this = pin::Pin::into_inner_unchecked(self);
            this.i2c
                .poll_write_read(cx, this.address, this.output, this.input)
        }
    }
}

impl<SCL, SDA> Drop for WriteRead<'_, SCL, SDA> {
    fn drop(&mut self) {
        self.i2c.poll_cancel();
    }
}

impl<SCL, SDA> super::I2C<SCL, SDA> {
    /// Manually drive an I2C write-read transaction
    ///
    /// Sends `output`, generates a repeated start, then waits for the I2C device to send enough
    /// data for `input`.
    ///
    /// See [`I2C::write_read`](crate::I2C::write_read) for a safer, simpler interface.
    ///
    /// # Safety
    ///
    /// This function allows you to manually drive the I2C write-read state machine. You must always
    /// call the method with the same arguments. The `output` and `input` buffers must not outlive
    /// the I2C instance.
    ///
    /// Once you call `poll_write_read`, you must continue to call the method
    /// until you receive `Poll::Ready(_)`, or until you call [`poll_cancel`](crate::I2C::poll_cancel). You cannot use any
    /// other 'poll' operations while this result is pending.
    pub unsafe fn poll_write_read(
        &mut self,
        cx: &mut task::Context<'_>,
        address: u8,
        output: &[u8],
        input: &mut [u8],
    ) -> Poll<Result<(), super::Error>> {
        loop {
            match self.state {
                None => {
                    if output.is_empty() {
                        return Poll::Ready(Ok(()));
                    } else if input.len() > 256 {
                        return Poll::Ready(Err(super::Error::RequestTooMuchData));
                    }
                    super::check_busy(&self.i2c)?;
                    super::clear_fifo(&self.i2c);
                    super::clear_status(&self.i2c);
                    self.state = Some(State::StartWrite);
                }
                Some(State::StartWrite) => {
                    futures::ready!(commands::poll_start_write(&mut self.i2c, cx, address)?);
                    self.state = Some(State::Send(0));
                }
                Some(State::Send(idx)) => {
                    futures::ready!(commands::poll_send(&mut self.i2c, cx, output[idx])?);
                    let next_idx = idx + 1;
                    self.state = if next_idx < output.len() {
                        Some(State::Send(next_idx))
                    } else {
                        Some(State::StartRead)
                    };
                }
                Some(State::StartRead) => {
                    futures::ready!(commands::poll_start_read(&mut self.i2c, cx, address)?);
                    self.state = Some(State::EndOfPacket);
                }
                Some(State::EndOfPacket) => {
                    futures::ready!(commands::poll_end_of_packet(&mut self.i2c, cx)?);
                    self.state = if !input.is_empty() {
                        Some(State::ReceiveLength)
                    } else {
                        Some(State::StopSetup)
                    };
                }
                Some(State::ReceiveLength) => {
                    futures::ready!(commands::poll_receive_length(
                        &mut self.i2c,
                        cx,
                        input.len()
                    )?);
                    self.state = Some(State::Receive(0));
                }
                Some(State::Receive(idx)) => {
                    let byte = futures::ready!(commands::poll_receive(&mut self.i2c, cx)?);
                    input[idx] = byte;
                    let next_idx = idx + 1;
                    self.state = if next_idx < input.len() {
                        Some(State::Receive(next_idx))
                    } else {
                        Some(State::StopSetup)
                    };
                }
                Some(State::StopSetup) => {
                    futures::ready!(commands::poll_stop_setup(&mut self.i2c, cx)?);
                    self.state = Some(State::Stop);
                }
                Some(State::Stop) => {
                    futures::ready!(commands::poll_stop(&mut self.i2c, cx)?);
                    self.state = None;
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}
