//! I2C write_read implementation

use super::{commands, Error};

use core::{
    future::Future,
    pin,
    task::{self, Poll},
};

#[pin_project::pin_project]
#[doc(hidden)]
pub struct WriteRead<'a, I2C> {
    i2c: &'a mut I2C,
    address: u8,
    output: &'a [u8],
    input: &'a mut [u8],
    state: Option<State>,
}

impl<'a, I2C> WriteRead<'a, I2C> {
    pub(super) fn new(
        i2c: &'a mut I2C,
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
        }
    }
}

pub enum State {
    StartWrite,
    Send,
    StartRead,
    EndOfPacket,
    Receive,
    Stop,
}

impl<'a, SCL: Unpin, SDA: Unpin> Future for WriteRead<'a, super::I2C<SCL, SDA>> {
    type Output = Result<(), Error>;

    fn poll(self: pin::Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        let this = self.project();
        let i2c: &mut super::I2C<SCL, SDA> = this.i2c;
        pin::Pin::new(i2c).poll_write_read(cx, *this.address, this.output, this.input)
    }
}

impl<SCL, SDA> super::I2C<SCL, SDA> {
    pub fn poll_write_read(
        self: pin::Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        address: u8,
        output: &[u8],
        input: &mut [u8],
    ) -> Poll<Result<(), super::Error>> {
        let mut this = self.project();
        loop {
            match this.write_read_state {
                None => {
                    if input.len() > 256 {
                        return Poll::Ready(Err(super::Error::RequestTooMuchData));
                    }
                    super::check_busy(&mut this.i2c)?;
                    super::clear_fifo(&mut this.i2c);
                    super::clear_status(&mut this.i2c);
                    *this.write_read_state = Some(State::StartWrite);
                }
                Some(State::StartWrite) => {
                    let mut start_write = commands::start_write(&mut this.i2c, address);
                    // Safety: we know it to be Unpin; we won't unpin anything
                    let start_write = unsafe { pin::Pin::new_unchecked(&mut start_write) };
                    match start_write.poll(cx)? {
                        Poll::Ready(()) => *this.write_read_state = Some(State::Send),
                        Poll::Pending => return Poll::Pending,
                    }
                }
                Some(State::Send) => {
                    let mut send = commands::send(&mut this.i2c, output);
                    // Safety: we know it to be Unpin; we won't unpin anything
                    let send = unsafe { pin::Pin::new_unchecked(&mut send) };
                    match send.poll(cx)? {
                        Poll::Ready(()) => *this.write_read_state = Some(State::StartRead),
                        Poll::Pending => return Poll::Pending,
                    }
                }
                Some(State::StartRead) => {
                    let mut start_read = commands::start_read(&mut this.i2c, address);
                    // Safety: we know it to be Unpin; we won't unpin anything
                    let start_read = unsafe { pin::Pin::new_unchecked(&mut start_read) };
                    match start_read.poll(cx)? {
                        Poll::Ready(()) => *this.write_read_state = Some(State::EndOfPacket),
                        Poll::Pending => return Poll::Pending,
                    }
                }
                Some(State::EndOfPacket) => {
                    let mut end_of_packet = commands::end_of_packet(&mut this.i2c);
                    // Safety: we know it to be Unpin; we won't unpin anything
                    let end_of_packet = unsafe { pin::Pin::new_unchecked(&mut end_of_packet) };
                    match end_of_packet.poll(cx)? {
                        Poll::Ready(()) => {
                            *this.write_read_state = if !input.is_empty() {
                                Some(State::Receive)
                            } else {
                                Some(State::Stop)
                            }
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                }
                Some(State::Receive) => {
                    let mut receive = commands::receive(&mut this.i2c, input);
                    // Safety: we know it to be Unpin; we won't unpin anything
                    let receive = unsafe { pin::Pin::new_unchecked(&mut receive) };
                    match receive.poll(cx)? {
                        Poll::Ready(()) => *this.write_read_state = Some(State::Stop),
                        Poll::Pending => return Poll::Pending,
                    }
                }
                Some(State::Stop) => {
                    let mut stop = commands::stop(&mut this.i2c);
                    // Safety: we know it to be Unpin; we won't unpin anything
                    let stop = unsafe { pin::Pin::new_unchecked(&mut stop) };
                    match stop.poll(cx)? {
                        Poll::Ready(()) => {
                            *this.write_read_state = None;
                            return Poll::Ready(Ok(()));
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                }
            }
        }
    }
}
