//! Direct Memory Access (DMA) for async I/O
//!
//! DMA [`Channel`s](Channel) power some asynchronous I/O operations.
//! Use [`channels`](channels()) to acquire all of the processor's DMA channels.
//! Then, use the `Channel`s in APIs that require them. The implementation handles
//! DMA receive and transfer operations, and ensures that the lifetime of your buffers
//! is correct.

#![allow(non_snake_case)] // Compatibility with RAL

pub(crate) use imxrt_dma::peripheral::{Bidirectional, Destination, Source};
pub use imxrt_dma::{
    peripheral::{full_duplex, receive, transfer, FullDuplex, Rx, Tx},
    Element,
};

use crate::ral;
pub use imxrt_dma::{BandwidthControl, Channel, Error};

#[cfg(not(feature = "imxrt1010"))]
pub const CHANNEL_COUNT: usize = 32;
#[cfg(feature = "imxrt1010")]
pub const CHANNEL_COUNT: usize = 16;

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

#[cfg(not(feature = "imxrt1010"))]
interrupts! {
    handler!{unsafe fn DMA0_DMA16() {
        imxrt_dma::on_interrupt(0);
        imxrt_dma::on_interrupt(16);
    }}

    handler!{unsafe fn DMA1_DMA17() {
        imxrt_dma::on_interrupt(1);
        imxrt_dma::on_interrupt(17);
    }}

    handler!{unsafe fn DMA2_DMA18() {
        imxrt_dma::on_interrupt(2);
        imxrt_dma::on_interrupt(18);
    }}

    handler!{unsafe fn DMA3_DMA19() {
        imxrt_dma::on_interrupt(3);
        imxrt_dma::on_interrupt(19);
    }}

    handler!{unsafe fn DMA4_DMA20() {
        imxrt_dma::on_interrupt(4);
        imxrt_dma::on_interrupt(20);
    }}

    handler!{unsafe fn DMA5_DMA21() {
        imxrt_dma::on_interrupt(5);
        imxrt_dma::on_interrupt(21);
    }}

    handler!{unsafe fn DMA6_DMA22() {
        imxrt_dma::on_interrupt(6);
        imxrt_dma::on_interrupt(22);
    }}

    handler!{unsafe fn DMA7_DMA23() {
        imxrt_dma::on_interrupt(7);
        imxrt_dma::on_interrupt(23);
    }}

    handler!{unsafe fn DMA8_DMA24() {
        imxrt_dma::on_interrupt(8);
        imxrt_dma::on_interrupt(24);
    }}

    handler!{unsafe fn DMA9_DMA25() {
        imxrt_dma::on_interrupt(9);
        imxrt_dma::on_interrupt(25);
    }}

    handler!{unsafe fn DMA10_DMA26() {
        imxrt_dma::on_interrupt(10);
        imxrt_dma::on_interrupt(26);
    }}

    handler!{unsafe fn DMA11_DMA27() {
        imxrt_dma::on_interrupt(11);
        imxrt_dma::on_interrupt(27);
    }}

    handler!{unsafe fn DMA12_DMA28() {
        imxrt_dma::on_interrupt(12);
        imxrt_dma::on_interrupt(28);
    }}

    handler!{unsafe fn DMA13_DMA29() {
        imxrt_dma::on_interrupt(13);
        imxrt_dma::on_interrupt(29);
    }}

    handler!{unsafe fn DMA14_DMA30() {
        imxrt_dma::on_interrupt(14);
        imxrt_dma::on_interrupt(30);
    }}

    handler!{unsafe fn DMA15_DMA31() {
        imxrt_dma::on_interrupt(15);
        imxrt_dma::on_interrupt(31);
    }}
}

#[cfg(feature = "imxrt1010")]
interrupts! {
    handler!{unsafe fn DMA0() {
        imxrt_dma::on_interrupt(0);
    }}

    handler!{unsafe fn DMA1() {
        imxrt_dma::on_interrupt(1);
    }}

    handler!{unsafe fn DMA2() {
        imxrt_dma::on_interrupt(2);
    }}

    handler!{unsafe fn DMA3() {
        imxrt_dma::on_interrupt(3);
    }}

    handler!{unsafe fn DMA4() {
        imxrt_dma::on_interrupt(4);
    }}

    handler!{unsafe fn DMA5() {
        imxrt_dma::on_interrupt(5);
    }}

    handler!{unsafe fn DMA6() {
        imxrt_dma::on_interrupt(6);
    }}

    handler!{unsafe fn DMA7() {
        imxrt_dma::on_interrupt(7);
    }}

    handler!{unsafe fn DMA8() {
        imxrt_dma::on_interrupt(8);
    }}

    handler!{unsafe fn DMA9() {
        imxrt_dma::on_interrupt(9);
    }}

    handler!{unsafe fn DMA10() {
        imxrt_dma::on_interrupt(10);
    }}

    handler!{unsafe fn DMA11() {
        imxrt_dma::on_interrupt(11);
    }}

    handler!{unsafe fn DMA12() {
        imxrt_dma::on_interrupt(12);
    }}

    handler!{unsafe fn DMA13() {
        imxrt_dma::on_interrupt(13);
    }}

    handler!{unsafe fn DMA14() {
        imxrt_dma::on_interrupt(14);
    }}

    handler!{unsafe fn DMA15() {
        imxrt_dma::on_interrupt(15);
    }}
}
