//! I2C write_read implementation

use super::{commands, Error, Instance, State};

use core::{
    future::Future,
    marker::PhantomPinned,
    pin,
    task::{self, Poll},
};

/// An I2C write-read future
///
/// Use [`write_read`](crate::I2C::write_read) to create this future.
pub struct WriteRead<'a> {
    i2c: &'a Instance,
    address: u8,
    output: &'a [u8],
    input: &'a mut [u8],
    state: Option<State>,
    _pin: PhantomPinned,
}

impl<'a> WriteRead<'a> {
    pub(super) fn new(
        i2c: &'a Instance,
        address: u8,
        output: &'a [u8],
        input: &'a mut [u8],
    ) -> Self {
        WriteRead {
            i2c,
            address,
            output,
            input,
            state: None,
            _pin: PhantomPinned,
        }
    }
}

impl Future for WriteRead<'_> {
    type Output = Result<(), Error>;

    fn poll(self: pin::Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        // Safety: future is safely Unpin; only exposed as !Unpin, just in case.
        let this = unsafe { pin::Pin::into_inner_unchecked(self) };
        loop {
            match this.state {
                None => {
                    if this.output.is_empty() {
                        return Poll::Ready(Ok(()));
                    } else if this.input.len() > 256 {
                        return Poll::Ready(Err(super::Error::RequestTooMuchData));
                    }
                    super::check_busy(&this.i2c)?;
                    super::clear_fifo(&this.i2c);
                    super::clear_status(&this.i2c);
                    this.state = Some(State::StartWrite);
                }
                Some(State::StartWrite) => {
                    futures::ready!(commands::poll_start_write(&this.i2c, cx, this.address)?);
                    this.state = Some(State::Send(0));
                }
                Some(State::Send(idx)) => {
                    futures::ready!(commands::poll_send(&this.i2c, cx, this.output[idx])?);
                    let next_idx = idx + 1;
                    this.state = if next_idx < this.output.len() {
                        Some(State::Send(next_idx))
                    } else {
                        Some(State::StartRead)
                    };
                }
                Some(State::StartRead) => {
                    futures::ready!(commands::poll_start_read(&this.i2c, cx, this.address)?);
                    this.state = Some(State::EndOfPacket);
                }
                Some(State::EndOfPacket) => {
                    futures::ready!(commands::poll_end_of_packet(&this.i2c, cx)?);
                    this.state = if !this.input.is_empty() {
                        Some(State::ReceiveLength)
                    } else {
                        Some(State::StopSetup)
                    };
                }
                Some(State::ReceiveLength) => {
                    futures::ready!(commands::poll_receive_length(
                        &this.i2c,
                        cx,
                        this.input.len()
                    )?);
                    this.state = Some(State::Receive(0));
                }
                Some(State::Receive(idx)) => {
                    let byte = futures::ready!(commands::poll_receive(&this.i2c, cx)?);
                    this.input[idx] = byte;
                    let next_idx = idx + 1;
                    this.state = if next_idx < this.input.len() {
                        Some(State::Receive(next_idx))
                    } else {
                        Some(State::StopSetup)
                    };
                }
                Some(State::StopSetup) => {
                    futures::ready!(commands::poll_stop_setup(&this.i2c, cx)?);
                    this.state = Some(State::Stop);
                }
                Some(State::Stop) => {
                    futures::ready!(commands::poll_stop(&this.i2c, cx)?);
                    this.state = None;
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl Drop for WriteRead<'_> {
    fn drop(&mut self) {
        super::disable_interrupts(&self.i2c);
    }
}
