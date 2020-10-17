//! SPI clock control

use super::{set_clock_gate, ClockGate, Disabled, Handle, SPIClock, CCGR_BASE};
use crate::ral;

const CLOCK_DIVIDER: u32 = 5;
/// If changing this, make sure to update `clock`
const CLOCK_HZ: u32 = 528_000_000 / CLOCK_DIVIDER;

#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
impl Disabled<SPIClock> {
    /// Enable the SPI clocks
    pub fn enable(self, handle: &mut Handle) -> SPIClock {
        unsafe { enable(&*handle.0) };
        self.0
    }
}

#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
impl SPIClock {
    /// Set the clock gate for the SPI instance
    pub fn clock_gate(&mut self, spi: &mut ral::lpspi::Instance, gate: ClockGate) {
        unsafe { clock_gate(&**spi, gate) }
    }

    /// Returns the SPI clock frequency (Hz)
    pub const fn frequency() -> u32 {
        CLOCK_HZ
    }
}

/// Set the clock gate for a SPI peripheral
///
/// # Safety
///
/// This could be called anywhere, by anyone who uses the globally-accessible SPI memory.
/// Consider using the safer `SPIClock::clock_gate` API.
#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
pub unsafe fn clock_gate(spi: *const ral::lpspi::RegisterBlock, value: ClockGate) {
    // Make sure that the match expression will never hit the unreachable!() case.
    // The comments and conditional compiles show what we're currently considering in
    // that match. If your chip isn't listed, it's not something we considered.
    #[cfg(not(any(feature = "imxrt1010", feature = "imxrt1060")))]
    compile_error!("Ensure that LPSPI clock gates are correct");

    let ccgr = CCGR_BASE.add(1);
    let gate = match spi {
        // imxrt1010, imxrt1060
        ral::lpspi::LPSPI1 => 0,
        // imxrt1010, imxrt1060
        ral::lpspi::LPSPI2 => 1,
        #[cfg(feature = "imxrt1060")]
        ral::lpspi::LPSPI3 => 2,
        #[cfg(feature = "imxrt1060")]
        ral::lpspi::LPSPI4 => 3,
        _ => unreachable!(),
    };

    set_clock_gate(ccgr, &[gate], value as u8);
}

/// Enable the SPI clock root
///
/// # Safety
///
/// This modifies easily-accessible global state. Consider using `SPIClock::enable`
/// for a safery API.
#[inline(always)]
#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
pub unsafe fn enable(ccm: *const ral::ccm::RegisterBlock) {
    // Select clock, and commit prescalar
    ral::modify_reg!(
        ral::ccm,
        ccm,
        CBCMR,
        LPSPI_PODF: CLOCK_DIVIDER - 1,
        LPSPI_CLK_SEL: LPSPI_CLK_SEL_2 // PLL2
    );
}
