//! I2C read implementation

use super::{commands, Error, State};

use core::{
    future::Future,
    marker::PhantomPinned,
    pin,
    task::{self, Poll},
};

/// An I2C read future
///
/// Use [`read`](crate::I2C::read) to create this future.
pub struct Read<'a, SCL, SDA> {
    i2c: &'a mut super::I2C<SCL, SDA>,
    address: u8,
    buffer: &'a mut [u8],
    _pin: PhantomPinned,
}

impl<'a, SCL, SDA> Read<'a, SCL, SDA> {
    pub(super) fn new(
        i2c: &'a mut super::I2C<SCL, SDA>,
        address: u8,
        buffer: &'a mut [u8],
    ) -> Self {
        Read {
            i2c,
            address,
            buffer,
            _pin: PhantomPinned,
        }
    }
}

impl<SCL: Unpin, SDA: Unpin> Future for Read<'_, SCL, SDA> {
    type Output = Result<(), Error>;

    fn poll(self: pin::Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        // Safety: keeping all memory pinned; calling poll operation with same arguments
        // until completion.
        unsafe {
            let this = pin::Pin::into_inner_unchecked(self);
            this.i2c.poll_read(cx, this.address, this.buffer)
        }
    }
}

impl<SCL, SDA> Drop for Read<'_, SCL, SDA> {
    fn drop(&mut self) {
        self.i2c.poll_cancel();
    }
}

impl<SCL, SDA> super::I2C<SCL, SDA> {
    /// Manually drive an I2C read
    ///
    /// Request a `buffer` of data from the I2C device identified by `address`.
    ///
    /// See [`read`](crate::I2C::read) for a safer, simpler interface.
    ///
    /// # Safety
    ///
    /// This function allows you to manually drive the I2C read state machine. You must always
    /// call the method with the same arguments. `buffer` must not outlive the I2C instance.
    ///
    /// Once you call `poll_read`, you must continue to call the method
    /// until you receive `Poll::Ready(_)`, or until you call [`poll_cancel`](crate::I2C::poll_cancel). You cannot use any
    /// other 'poll' operations while this result is pending.
    pub unsafe fn poll_read(
        &mut self,
        cx: &mut task::Context<'_>,
        address: u8,
        buffer: &mut [u8],
    ) -> Poll<Result<(), super::Error>> {
        loop {
            match self.state {
                None => {
                    if buffer.len() > 256 {
                        return Poll::Ready(Err(super::Error::RequestTooMuchData));
                    } else if buffer.is_empty() {
                        return Poll::Ready(Ok(()));
                    }
                    super::check_busy(&self.i2c)?;
                    super::clear_fifo(&self.i2c);
                    super::clear_status(&self.i2c);
                    self.state = Some(State::StartRead);
                }
                Some(State::StartRead) => {
                    futures::ready!(commands::poll_start_read(&mut self.i2c, cx, address)?);
                    self.state = Some(State::ReceiveLength);
                }
                Some(State::ReceiveLength) => {
                    futures::ready!(commands::poll_receive_length(
                        &mut self.i2c,
                        cx,
                        buffer.len()
                    )?);
                    self.state = Some(State::Receive(0));
                }
                Some(State::Receive(idx)) => {
                    let byte = futures::ready!(commands::poll_receive(&mut self.i2c, cx)?);
                    buffer[idx] = byte;
                    let next_idx = idx + 1;
                    self.state = if next_idx < buffer.len() {
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
                _ => unreachable!(),
            }
        }
    }
}
