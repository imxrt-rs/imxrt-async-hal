//! Direct Memory Access (DMA) for async I/O
//!
//! DMA [`Channel`s](Channel) power some asynchronous I/O operations.
//! Use [`channels`](channels()) to acquire all of the processor's DMA channels.
//! Then, use the `Channel`s in APIs that require them. The implementation handles
//! DMA receive and transfer operations, and ensures that the lifetime of your buffers
//! is correct.
//!
//! The implementation also provides a [`pipe`], a hardware-backed
//! communication channels (not to be confused with DMA `Channel`s). Use `pipe` senders
//! and receivers to synchronize tasks, and transmit `Copy` data between tasks using
//! DMA hardware.

#![allow(non_snake_case)] // Compatibility with RAL

mod interrupt;
mod peripheral;
#[cfg(feature = "pipe")]
#[cfg_attr(docsrs, doc(cfg(feature = "pipe")))]
pub mod pipe;

pub(crate) use imxrt_dma::{Destination, Source};
use imxrt_dma::{Element, Transfer};
pub(crate) use peripheral::{receive, receive_raw, transfer, transfer_raw};

use crate::ral;
pub use imxrt_dma::{BandwidthControl, Channel, ErrorStatus};

#[cfg(not(feature = "imxrt1010"))]
pub const CHANNEL_COUNT: usize = 32;
#[cfg(feature = "imxrt1010")]
pub const CHANNEL_COUNT: usize = 16;

/// An error when preparing a transfer
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// There is already a scheduled transfer
    ///
    /// Cancel the transfer and try again.
    ScheduledTransfer,
    /// Error setting up the DMA transfer
    Setup(ErrorStatus),
    /// The operation was cancelled
    ///
    /// `Cancelled` is the return from a [`pipe`] sender or
    /// receiver when the other half is dropped.
    Cancelled,
}

/// Initialize and acquire the DMA channels
///
/// The return is 32 channels. However, **only the first [`CHANNEL_COUNT`] channels
/// are initialized to `Some(channel)`. The rest are `None`**.
///
/// You should enable the clock gates before calling `channels`. See the example for more
/// information on enabling clock gates.
///
/// # Example
///
/// Initialize and acquire the DMA channels, and move channel 7 to another function:
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::dma;
/// use hal::ral::{self, dma0, dmamux, ccm};
///
/// fn prepare_peripheral(channel: dma::Channel) { /* ... */ }
///
/// let ccm = ccm::CCM::take().unwrap();
/// // DMA clock gate on
/// ral::modify_reg!(ral::ccm, ccm, CCGR5, CG3: 0b11);
///
/// let mut dma = dma0::DMA0::take().unwrap();
/// let mut channels = dma::channels(
///     dma,
///     dmamux::DMAMUX::take().unwrap(),
/// );
///
/// prepare_peripheral(channels[7].take().unwrap());
/// ```
pub fn channels(dma: ral::dma0::Instance, mux: ral::dmamux::Instance) -> [Option<Channel>; 32] {
    drop(dma);
    drop(mux);

    let mut channels = [
        None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
        None, None,
    ];

    for (idx, channel) in channels.iter_mut().take(CHANNEL_COUNT).enumerate() {
        let mut c = unsafe { Channel::new(idx) };
        c.reset();
        *channel = Some(c);
    }

    // Correctly accounts for all i.MX RT variants that are
    // not in the 1010 (1011) family.
    #[cfg(not(feature = "imxrt1010"))]
    unsafe {
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA0_DMA16);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA1_DMA17);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA2_DMA18);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA3_DMA19);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA4_DMA20);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA5_DMA21);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA6_DMA22);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA7_DMA23);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA8_DMA24);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA9_DMA25);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA10_DMA26);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA11_DMA27);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA12_DMA28);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA13_DMA29);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA14_DMA30);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA15_DMA31);
    };
    // Acounts for the 1010 family (1011) outlier that only has 16 DMA channels.
    #[cfg(feature = "imxrt1010")]
    unsafe {
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA0);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA1);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA2);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA3);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA4);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA5);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA6);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA7);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA8);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA9);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA10);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA11);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA12);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA13);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA14);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA15);
    };

    channels
}
