//! SPI clock control

use super::{set_clock_gate, ClockActivity, Disabled, Handle, SPIClock, CCGR_BASE};
use crate::ral;

const CLOCK_DIVIDER: u32 = 5;
/// If changing this, make sure to update `clock`
const CLOCK_HZ: u32 = 528_000_000 / CLOCK_DIVIDER;

impl Disabled<SPIClock> {
    /// Enable the SPI clocks
    pub fn enable(self, handle: &mut Handle) -> SPIClock {
        unsafe { enable(&*handle.0) };
        self.0
    }
}

impl SPIClock {
    /// Set the clock gate activity for the SPI instance
    pub fn clock_gate(&mut self, spi: &mut ral::lpspi::Instance, activity: ClockActivity) {
        unsafe { clock_gate(&**spi, activity) }
    }

    /// Returns the SPI clock frequency (Hz)
    pub const fn frequency() -> u32 {
        CLOCK_HZ
    }
}

/// Set the clock gate activity for a SPI peripheral
///
/// # Safety
///
/// This could be called anywhere, by anyone who uses the globally-accessible SPI memory.
/// Consider using the safer `SPIClock::clock_gate` API.
pub unsafe fn clock_gate(spi: *const ral::lpspi::RegisterBlock, activity: ClockActivity) {
    let ccgr = CCGR_BASE.add(1);
    let gate = match spi {
        ral::lpspi::LPSPI1 => 0,
        ral::lpspi::LPSPI2 => 1,
        ral::lpspi::LPSPI3 => 2,
        ral::lpspi::LPSPI4 => 3,
        _ => unreachable!(),
    };
    set_clock_gate(ccgr, &[gate], activity as u8);
}

/// Enable the SPI clock root
///
/// # Safety
///
/// This modifies easily-accessible global state. Consider using `SPIClock::enable`
/// for a safery API.
#[inline(always)]
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
