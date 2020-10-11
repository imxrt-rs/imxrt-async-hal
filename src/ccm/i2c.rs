//! I2C clock control

use super::{set_clock_gate, ClockActivity, Disabled, Handle, I2CClock, CCGR_BASE};
use crate::ral;

/// I2C peripheral clock frequency
///
/// If changing the root clock in `enable`, you'll need to update
/// this value.
const I2C_CLOCK_HZ: u32 = crate::ccm::OSCILLATOR_FREQUENCY_HZ / I2C_CLOCK_DIVIDER;
/// I2C peripheral clock divider
const I2C_CLOCK_DIVIDER: u32 = 3;

#[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
impl Disabled<I2CClock> {
    /// Enable the I2C clocks
    pub fn enable(self, handle: &mut Handle) -> I2CClock {
        unsafe { enable(&*handle.0) };
        self.0
    }
}

#[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
impl I2CClock {
    /// Set the clock gate activity for the I2C instance
    pub fn clock_gate(&mut self, i2c: &mut ral::lpi2c::Instance, activity: ClockActivity) {
        unsafe { clock_gate(&**i2c, activity) }
    }

    /// Returns the I2C clock frequency (Hz)
    pub const fn frequency() -> u32 {
        I2C_CLOCK_HZ
    }
}

/// Set the clock gate activity for a I2C peripheral
///
/// # Safety
///
/// This could be called anywhere, by anyone who uses the globally-accessible I2C memory.
/// Consider using the safer `I2CClock::clock_gate` API.
#[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
pub unsafe fn clock_gate(i2c: *const ral::lpi2c::RegisterBlock, activity: ClockActivity) {
    let value = activity as u8;
    match i2c {
        ral::lpi2c::LPI2C1 => set_clock_gate(CCGR_BASE.add(2), &[3], value),
        ral::lpi2c::LPI2C2 => set_clock_gate(CCGR_BASE.add(2), &[4], value),
        ral::lpi2c::LPI2C3 => set_clock_gate(CCGR_BASE.add(2), &[5], value),
        ral::lpi2c::LPI2C4 => set_clock_gate(CCGR_BASE.add(6), &[12], value),
        _ => unreachable!(),
    }
}

/// Enable the I2C clock root
///
/// # Safety
///
/// This modifies easily-accessible global state. Consider using `I2CClock::enable`
/// for a safery API.
#[inline(always)]
#[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
pub unsafe fn enable(ccm: *const ral::ccm::RegisterBlock) {
    // Select clock, and commit prescalar
    ral::modify_reg!(
        ral::ccm,
        ccm,
        CSCDR2,
        LPI2C_CLK_PODF: (I2C_CLOCK_DIVIDER.saturating_sub(1)),
        LPI2C_CLK_SEL: LPI2C_CLK_SEL_1 // 24MHz XTAL oscillator
    );
}
