//! I2C read implementation

use super::{commands, Error, RegisterBlock, State};

use core::{
    future::Future,
    marker::PhantomPinned,
    pin,
    task::{self, Poll},
};

/// An I2C read future
///
/// Use [`read`](crate::I2C::read) to create this future.
pub struct Read<'a> {
    i2c: &'a RegisterBlock,
    address: u8,
    buffer: &'a mut [u8],
    state: Option<State>,
    _pin: PhantomPinned,
}

impl<'a> Read<'a> {
    pub(super) fn new(i2c: &'a RegisterBlock, address: u8, buffer: &'a mut [u8]) -> Self {
        Read {
            i2c,
            address,
            buffer,
            state: None,
            _pin: PhantomPinned,
        }
    }
}

impl Future for Read<'_> {
    type Output = Result<(), Error>;

    fn poll(self: pin::Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        // Safety: future is safely Unpin; only exposed as !Unpin, just in case.
        let this = unsafe { pin::Pin::into_inner_unchecked(self) };
        loop {
            match this.state {
                None => {
                    if this.buffer.len() > 256 {
                        return Poll::Ready(Err(super::Error::RequestTooMuchData));
                    } else if this.buffer.is_empty() {
                        return Poll::Ready(Ok(()));
                    }
                    super::check_busy(&this.i2c)?;
                    super::clear_fifo(&this.i2c);
                    super::clear_status(&this.i2c);
                    this.state = Some(State::StartRead);
                }
                Some(State::StartRead) => {
                    futures::ready!(commands::poll_start_read(&this.i2c, cx, this.address)?);
                    this.state = Some(State::ReceiveLength);
                }
                Some(State::ReceiveLength) => {
                    futures::ready!(commands::poll_receive_length(
                        &this.i2c,
                        cx,
                        this.buffer.len()
                    )?);
                    this.state = Some(State::Receive(0));
                }
                Some(State::Receive(idx)) => {
                    let byte = futures::ready!(commands::poll_receive(&this.i2c, cx)?);
                    this.buffer[idx] = byte;
                    let next_idx = idx + 1;
                    this.state = if next_idx < this.buffer.len() {
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
                _ => unreachable!(),
            }
        }
    }
}

impl Drop for Read<'_> {
    fn drop(&mut self) {
        super::disable_interrupts(&self.i2c);
    }
}
