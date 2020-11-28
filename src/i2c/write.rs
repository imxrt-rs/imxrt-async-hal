//! I2C write implementation

use super::{commands, Error, Instance, State};

use core::{
    future::Future,
    marker::PhantomPinned,
    pin,
    task::{self, Poll},
};

/// An I2C write future
///
/// Use [`write`](crate::I2C::write) to create this future.
pub struct Write<'a> {
    i2c: &'a Instance,
    address: u8,
    buffer: &'a [u8],
    state: Option<State>,
    _pin: PhantomPinned,
}

impl<'a> Write<'a> {
    pub(super) fn new(i2c: &'a Instance, address: u8, buffer: &'a [u8]) -> Self {
        Write {
            i2c,
            address,
            buffer,
            state: None,
            _pin: PhantomPinned,
        }
    }
}

impl Future for Write<'_> {
    type Output = Result<(), Error>;

    fn poll(self: pin::Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        // Safety: future is safely Unpin; only exposed as !Unpin, just in case.
        let this = unsafe { pin::Pin::into_inner_unchecked(self) };
        loop {
            match this.state {
                None => {
                    if this.buffer.is_empty() {
                        return Poll::Ready(Ok(()));
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
                    futures::ready!(commands::poll_send(&this.i2c, cx, this.buffer[idx])?);
                    let next_idx = idx + 1;
                    this.state = if next_idx < this.buffer.len() {
                        Some(State::Send(next_idx))
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

impl Drop for Write<'_> {
    fn drop(&mut self) {
        super::disable_interrupts(&self.i2c);
    }
}
