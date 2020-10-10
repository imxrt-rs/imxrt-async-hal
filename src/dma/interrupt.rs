//! DMA interrupts and shared state

use crate::dma::{Channel, Error};
use core::{
    future::Future,
    pin::Pin,
    sync::atomic,
    task::{Context, Poll, Waker},
};

pub const NUM_SHARED_STATES: usize = 2;

pub mod state {
    pub const UNDEFINED: u32 = 0;
    pub const READY: u32 = 1;
    pub const PENDING: u32 = 2;
    pub const COMPLETE: u32 = 3;
    pub const DROPPED: u32 = 4;
}

/// Shared state for DMA interrupts and futures
pub struct Shared {
    /// Shared state between the ISR and the futures
    ///
    /// Values are implementation specific, except for those defined
    /// in this module.
    state: atomic::AtomicU32,
    /// Task wakers
    pub waker: Option<Waker>,
}

impl Shared {
    const fn new() -> Self {
        Shared {
            state: atomic::AtomicU32::new(state::UNDEFINED),
            waker: None,
        }
    }
    pub fn set_state(&mut self, state: u32) {
        self.state.store(state, atomic::Ordering::SeqCst);
    }
    pub fn state(&self) -> u32 {
        self.state.load(atomic::Ordering::SeqCst)
    }
}

pub static mut SHARED_STATES: [[Shared; NUM_SHARED_STATES]; 32] = [
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
    [Shared::new(), Shared::new()],
];

#[inline(always)]
unsafe fn on_interrupt(idx: usize) {
    let mut channel = crate::dma::Channel::new(idx);
    if channel.is_interrupt() {
        channel.clear_interrupt();
    }
    if channel.is_complete() {
        let states = &mut SHARED_STATES[idx];
        states.iter_mut().for_each(|state| {
            state.set_state(state::COMPLETE);
            if let Some(waker) = state.waker.take() {
                waker.wake();
            }
        });
    }
}

interrupts! {
    unsafe fn DMA0_DMA16() {
        on_interrupt(0);
        on_interrupt(16);
    }

    unsafe fn DMA1_DMA17() {
        on_interrupt(1);
        on_interrupt(17);
    }

    unsafe fn DMA2_DMA18() {
        on_interrupt(2);
        on_interrupt(18);
    }

    unsafe fn DMA3_DMA19() {
        on_interrupt(3);
        on_interrupt(19);
    }

    unsafe fn DMA4_DMA20() {
        on_interrupt(4);
        on_interrupt(20);
    }

    unsafe fn DMA5_DMA21() {
        on_interrupt(5);
        on_interrupt(21);
    }

    unsafe fn DMA6_DMA22() {
        on_interrupt(6);
        on_interrupt(22);
    }

    unsafe fn DMA7_DMA23() {
        on_interrupt(7);
        on_interrupt(23);
    }

    unsafe fn DMA8_DMA24() {
        on_interrupt(8);
        on_interrupt(24);
    }

    unsafe fn DMA9_DMA25() {
        on_interrupt(9);
        on_interrupt(25);
    }

    unsafe fn DMA10_DMA26() {
        on_interrupt(10);
        on_interrupt(26);
    }

    unsafe fn DMA11_DMA27() {
        on_interrupt(11);
        on_interrupt(27);
    }

    unsafe fn DMA12_DMA28() {
        on_interrupt(12);
        on_interrupt(28);
    }

    unsafe fn DMA13_DMA29() {
        on_interrupt(13);
        on_interrupt(29);
    }

    unsafe fn DMA14_DMA30() {
        on_interrupt(14);
        on_interrupt(30);
    }

    unsafe fn DMA15_DMA31() {
        on_interrupt(15);
        on_interrupt(31);
    }
}

/// A future that wakes when a DMA interrupt fires
///
/// `Interrupt` is the building block for other DMA futures. It wakes when
/// the interrupt fires. You must make sure that the DMA channel is properly
/// configured before awaiting `Interrupt`!
///
/// `Interrupt` will disable the transaction when dropped.
pub struct Interrupt<'c, F: FnMut(&mut Channel)> {
    channel: &'c mut Channel,
    state: u32,
    on_drop: F,
}

impl<'c, F: FnMut(&mut Channel)> Future for Interrupt<'c, F> {
    type Output = Result<(), Error>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        match this.state {
            state::PENDING if this.channel.is_complete() => {
                this.state = state::COMPLETE;
                this.channel.clear_complete();
                Poll::Ready(Ok(()))
            }
            state::PENDING => Poll::Pending,
            state::READY if this.channel.is_enabled() => Poll::Ready(Err(Error::ScheduledTransfer)),
            state::READY => unsafe {
                SHARED_STATES[this.channel.channel()][0].waker = Some(cx.waker().clone());
                atomic::compiler_fence(atomic::Ordering::Release);
                this.channel.set_enable(true);
                if this.channel.is_error() {
                    this.channel.set_enable(false);
                    atomic::compiler_fence(atomic::Ordering::Acquire);
                    this.state = state::UNDEFINED;
                    SHARED_STATES[this.channel.channel()][0].waker = None;
                    let es = super::ErrorStatus::new(this.channel.error_status());
                    this.channel.clear_error();
                    Poll::Ready(Err(Error::Setup(es)))
                } else {
                    this.state = state::PENDING;
                    Poll::Pending
                }
            },
            _ => unreachable!(),
        }
    }
}

impl<'c, F: FnMut(&mut Channel)> Drop for Interrupt<'c, F> {
    fn drop(&mut self) {
        (self.on_drop)(&mut self.channel);
        self.channel.set_enable(false);
        self.channel.clear_complete();
        atomic::compiler_fence(atomic::Ordering::Release);
        unsafe {
            SHARED_STATES[self.channel.channel()][0].waker = None;
        }
    }
}

/// Create an `Interrupt` future that will await for the DMA transaction
/// to complete.
///
/// # Safety
///
/// Caller must ensure that the DMA transaction is fully defined. Failure
/// to properly define the transfer may result in an error (best case) or
/// reads and writes to some memory, somewhere (worst case).
pub unsafe fn interrupt<F: FnMut(&mut Channel)>(channel: &mut Channel, on_drop: F) -> Interrupt<F> {
    channel.set_disable_on_completion(true);
    channel.set_interrupt_on_completion(true);
    Interrupt {
        channel,
        state: state::READY,
        on_drop,
    }
}
