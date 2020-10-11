//! Periodic clock implementations

use super::{set_clock_gate, ClockActivity, Disabled, Handle, PerClock, CCGR_BASE};
use crate::ral;

const PERIODIC_CLOCK_FREQUENCY_HZ: u32 = super::OSCILLATOR_FREQUENCY_HZ / PERIODIC_CLOCK_DIVIDER;
const PERIODIC_CLOCK_DIVIDER: u32 = 24;

impl PerClock {
    /// Set the clock activity for the GPT
    pub fn clock_gate_gpt(&mut self, gpt: &mut crate::ral::gpt::Instance, activity: ClockActivity) {
        unsafe { clock_gate_gpt(&**gpt, activity) };
    }
    /// Set the clock activity for the PIT
    pub fn clock_gate_pit(&mut self, pit: &mut crate::ral::pit::Instance, activity: ClockActivity) {
        unsafe { clock_gate_pit(&**pit, activity) };
    }
    /// Returns the periodic clock frequency (Hz)
    pub const fn frequency() -> u32 {
        PERIODIC_CLOCK_FREQUENCY_HZ
    }
}

impl Disabled<PerClock> {
    /// Enable the periodic clock root
    pub fn enable(self, ccm: &mut Handle) -> PerClock {
        unsafe {
            enable(&*ccm.0);
        };
        self.0
    }
}

/// Set the GPT clock gate activity
///
/// # Safety
///
/// This could be used by anyone who supplies a GPT register block, which is globally
/// available. Consider using [`PerClock::clock_gate_gpt`](struct.PerClock.html#method.clock_gate_gpt)
/// for a safer interface.
pub unsafe fn clock_gate_gpt(gpt: *const ral::gpt::RegisterBlock, activity: ClockActivity) {
    let value = activity as u8;
    match gpt {
        ral::gpt::GPT1 => set_clock_gate(CCGR_BASE.add(1), &[10, 11], value),
        ral::gpt::GPT2 => set_clock_gate(CCGR_BASE.add(0), &[12, 13], value),
        _ => unreachable!(),
    }
}

/// Set the PIT clock gate activity
///
/// # Safety
///
/// This could be used by anyone who supplies a PIT register block, which is globally
/// available. Consider using [`PerClock::clock_gate_pit`](struct.PerClock.html#method.clock_gate_pit)
/// for a safer interface.
pub unsafe fn clock_gate_pit(pit: *const ral::pit::RegisterBlock, activity: ClockActivity) {
    match pit {
        ral::pit::PIT => set_clock_gate(CCGR_BASE.add(1), &[6], activity as u8),
        _ => unreachable!(),
    }
}

/// Enable the periodic clock root
///
/// # Safety
///
/// This modifies globally-accessible memory, and it may affect the behaviors of any enabled
/// PITs or GPTs. You should not use this method if you've already enabled those timers.
pub unsafe fn enable(ccm: *const ral::ccm::RegisterBlock) {
    ral::modify_reg!(
        ral::ccm,
        ccm,
        CSCMR1,
        PERCLK_CLK_SEL: PERCLK_CLK_SEL_1,
        PERCLK_PODF: PERIODIC_CLOCK_DIVIDER - 1
    );
}
