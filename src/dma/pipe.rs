//! Cross-task communication with DMA channels
//!
//! _(Note: this API is similar to typical 'channels' that you'll find in the Rust ecosystem. We
//! use 'pipe' to disambiguate between this software channel and a hardware DMA channel.)_
//!
//! `pipe` provides a mechanism for sending data across tasks. The [`Sender`](struct.Sender.html)
//! half can send `Copy` data, and the [`Receiver`](struct.Receiver.html)
//! half can receive that same data. The tasks use a DMA channel to transfer the data across tasks.
//! Use [`new`](fn.new.html) to create both halves of a pipe.
//!
//! A `Sender` blocks until the `Receiver` is ready to receive data. Likewise, the `Receiver` blocks until
//! the `Sender` is ready to send data. This creates a synchronization point for the two tasks. When the transfer
//! completes, the data will have been transferred from the sender to the receiver.
//!
//! The implementation does not guarantee any order for waking the two waiting tasks. That is, after the transfer
//! completes, the sender task may wake before the receiver task; or, the receiver task may wake before the sender
//! task.
//!
//! To cancel a transfer, drop either the `Sender` or the `Receiver`. When one half is dropped, the remaining half will
//! immediately return an [`Error::Cancelled`](../enum.Error.html#variant.Cancelled). The remaining half can never be used
//! again, as it will always, immediately return `Error::Cancelled`.
//!
//! # Example
//!
//! Transmit an incrementing counter every 100ms using DMA channel 13. The sender is delayed by SYSTICK, which delays
//! the receiver.
//!
//! ```no_run
//! use imxrt_async_hal as hal;
//! use hal::dma;
//! use hal::ral::{ccm, dma0, dmamux};
//!
//! let mut ccm = ccm::CCM::take().unwrap();
//!
//! let core_peripherals = cortex_m::Peripherals::take().unwrap();
//! let mut systick = hal::SystemTick::new(core_peripherals.SYST);
//!
//! let mut channels = dma::channels(
//!     dma0::DMA0::take().unwrap(),
//!     dmamux::DMAMUX::take().unwrap(),
//!     &mut ccm,
//! );
//!
//! let (mut tx, mut rx) = dma::pipe::new(channels[13].take().unwrap());
//! let sender = async {
//!     let mut counter: i32 = 0;
//!     loop {
//!         tx.send(&counter).await.unwrap();
//!         systick.delay_ms(100).await;
//!         counter = counter.wrapping_add(1);
//!     }
//! };
//!
//! let receiver = async {
//!     loop {
//!         // Unblocks every 100ms, since that's the send rate.
//!         let counter = rx.receive().await.unwrap();
//!     }
//! };
//!
//! # fn block_on<F: core::future::Future>(f: F) {};
//! block_on(futures::future::join(sender, receiver));
//! ```

use crate::dma::{self, interrupt::State};
use core::{
    future::Future,
    marker::PhantomData,
    mem::MaybeUninit,
    pin::Pin,
    sync::atomic,
    task::{Context, Poll},
};

const SENDER_STATE: usize = 0;
const RECEIVER_STATE: usize = 1;

/// Alias for a `Result` that might return an [`Error`](../enum.Error.html).
pub type Result<T> = core::result::Result<T, dma::Error>;

/// The sending half of a pipe
///
/// Use [`new`](fn.new.html) to create both halves of a pipe.
///
/// Once `Sender` is dropped, the associated [`Receiver`](struct.Receiver.html) will never block,
/// and always return an error.
pub struct Sender<E> {
    /// Aliased in Receiver
    channel: dma::Channel,
    _element: PhantomData<E>,
}

impl<E> Sender<E> {
    fn new(channel: dma::Channel) -> Self {
        Sender {
            channel,
            _element: PhantomData,
        }
    }
}

/// The receiving half of a pipe
///
/// Use [`new`](fn.new.html) to create both halves of a pipe.
///
/// Once `Receiver` is dropped, the associated [`Sender`](struct.Sender.html) will never block,
/// and always return an error.
pub struct Receiver<E> {
    /// Aliased in Sender
    channel: dma::Channel,
    _element: PhantomData<E>,
}

impl<E> Receiver<E> {
    fn new(channel: dma::Channel) -> Self {
        Receiver {
            channel,
            _element: PhantomData,
        }
    }
}

/// Create a pipe for sending and receiving data
///
/// # Example
///
/// Demonstrate pipe construction, and how to send and receive data. For a larger example, see the
/// [module-level documentation](index.html).
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::dma;
/// use hal::ral::{dma0, dmamux, ccm};
///
/// let mut ccm = ccm::CCM::take().unwrap();
/// let mut channels = dma::channels(
///     dma0::DMA0::take().unwrap(),
///     dmamux::DMAMUX::take().unwrap(),
///     &mut ccm,
/// );
/// let (mut tx, mut rx) = dma::pipe::new(channels[29].take().unwrap());
/// # async {
///
/// // In the sending task
/// tx.send(&5i32).await.unwrap();
///
/// // In the receiving task
/// assert_eq!(rx.receive().await.unwrap(), 5i32);
/// # };
/// ```
pub fn new<E: Copy + Unpin>(mut channel: dma::Channel) -> (Sender<E>, Receiver<E>) {
    channel.set_always_on();
    channel.set_interrupt_on_completion(true);
    channel.set_disable_on_completion(true);
    let rx_channel = unsafe { dma::Channel::new(channel.channel()) };
    let tx_channel = channel;
    (Sender::new(tx_channel), Receiver::new(rx_channel))
}

struct Send<'t, E> {
    channel: &'t mut dma::Channel,
    value: &'t E,
}

impl<E: Copy> Sender<E> {
    /// Await the receive half, and transmit `value` once the receiver is ready
    ///
    /// Returns nothing if the transfer was successful, or an [`Error`](../enum.Error.html)
    /// if the transfer failed.
    pub async fn send<'t>(&'t mut self, value: &'t E) -> Result<()> {
        unsafe {
            self.channel.shared_mut()[SENDER_STATE].state = State::Ready;
        }
        Send {
            channel: &mut self.channel,
            value,
        }
        .await
    }
}

impl<'t, E: Copy> Future for Send<'t, E> {
    type Output = Result<()>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let (sender_state, receiver_state) = unsafe {
            let shared = this.channel.shared();
            (shared[SENDER_STATE].state, shared[RECEIVER_STATE].state)
        };
        match (sender_state, receiver_state) {
            (_, State::Dropped) => Poll::Ready(Err(dma::Error::Cancelled)),
            (State::Ready, _) => {
                let data = this.value as *const E as *const u8;
                let len = core::mem::size_of_val(this.value);

                this.channel.set_minor_loop_elements::<u8>(1);
                this.channel.set_transfer_iterations(len as u16);

                unsafe {
                    this.channel.set_source_transfer(dma::Transfer::buffer(
                        core::slice::from_raw_parts(data, len),
                    ));
                    let sender_shared = &mut this.channel.shared_mut()[SENDER_STATE];
                    sender_shared.waker = Some(cx.waker().clone());
                    sender_shared.state = State::Pending;
                    atomic::compiler_fence(atomic::Ordering::Release);
                    if State::Pending == receiver_state {
                        this.channel.set_enable(true);
                        this.channel.start();
                    }
                }
                Poll::Pending
            }
            (State::Complete, _) => Poll::Ready(Ok(())),
            (State::Pending, _) => Poll::Pending,
            _ => unreachable!(),
        }
    }
}

impl<'t, E> Drop for Send<'t, E> {
    fn drop(&mut self) {
        self.channel.set_enable(false);
        atomic::compiler_fence(atomic::Ordering::Release);
        // Safety: channel is disabled, so there is no ISR that can run.
        unsafe {
            self.channel.shared_mut()[SENDER_STATE].state = State::Undefined;
        }
    }
}

impl<E> Drop for Sender<E> {
    fn drop(&mut self) {
        // Safety: the Send future cannot outlive the Sender.
        // The Send future disables the transfer. By the time
        // this runs, we cannot be prempted by the DMA ISR.
        unsafe {
            let shared = self.channel.shared_mut();
            shared[SENDER_STATE].state = State::Dropped;
            if let Some(waker) = shared[RECEIVER_STATE].waker.take() {
                waker.wake();
            }
        }
    }
}

struct Receive<'t, E> {
    channel: &'t mut dma::Channel,
    value: MaybeUninit<E>,
}

impl<E: Copy + Unpin> Receiver<E> {
    /// Await the sender to send data, unblocking once the transfer completes
    ///
    /// Returns the transmitted data, or an [`Error`](../enum.Error.html) if the transfer failed.
    pub async fn receive(&mut self) -> Result<E> {
        unsafe {
            self.channel.shared_mut()[RECEIVER_STATE].state = State::Ready;
        }
        Receive {
            channel: &mut self.channel,
            value: MaybeUninit::uninit(),
        }
        .await
    }
}

impl<'t, E: Copy + Unpin> Future for Receive<'t, E> {
    type Output = Result<E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let (receiver_state, sender_state) = unsafe {
            let shared = this.channel.shared();
            (shared[RECEIVER_STATE].state, shared[SENDER_STATE].state)
        };
        match (receiver_state, sender_state) {
            (_, State::Dropped) => Poll::Ready(Err(dma::Error::Cancelled)),
            (State::Ready, _) => {
                let data = this.value.as_mut_ptr() as *mut u8;
                let len = core::mem::size_of::<E>();
                unsafe {
                    this.channel
                        .set_destination_transfer(dma::Transfer::buffer_mut(
                            core::slice::from_raw_parts_mut(data, len),
                        ));

                    let receiver_shared = &mut this.channel.shared_mut()[RECEIVER_STATE];
                    receiver_shared.waker = Some(cx.waker().clone());
                    receiver_shared.state = State::Pending;
                    atomic::compiler_fence(atomic::Ordering::Release);
                    if State::Pending == sender_state {
                        this.channel.set_enable(true);
                        this.channel.start();
                    }

                    Poll::Pending
                }
            }
            (State::Complete, _) => unsafe { Poll::Ready(Ok(this.value.assume_init())) },
            (State::Pending, _) => Poll::Pending,
            _ => unreachable!(),
        }
    }
}

impl<'t, E> Drop for Receive<'t, E> {
    fn drop(&mut self) {
        self.channel.set_enable(false);
        atomic::compiler_fence(atomic::Ordering::Release);
        // Safety: channel is disabled, so there is no ISR that can run.
        unsafe {
            self.channel.shared_mut()[RECEIVER_STATE].state = State::Undefined;
        }
    }
}

impl<E> Drop for Receiver<E> {
    fn drop(&mut self) {
        // Safety: the Receive future cannot outlive the Receiver.
        // The Receive future disables the transfer. By the time
        // this runs, we cannot be prempted by the DMA ISR.
        unsafe {
            let shared = self.channel.shared_mut();
            shared[RECEIVER_STATE].state = State::Dropped;
            if let Some(waker) = shared[SENDER_STATE].waker.take() {
                waker.wake();
            }
        }
    }
}
