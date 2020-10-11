//! Periodic clock implementations

use super::{ClockActivity, ClockGate, Disabled, Handle, PerClock};
use crate::ral;

const PERIODIC_CLOCK_FREQUENCY_HZ: u32 = super::OSCILLATOR_FREQUENCY_HZ / PERIODIC_CLOCK_DIVIDER;
const PERIODIC_CLOCK_DIVIDER: u32 = 24;

impl PerClock {
    /// Set the clock activity for the GPT
    pub fn clock_gate_gpt(&mut self, gpt: &mut crate::ral::gpt::Instance, activity: ClockActivity) {
        unsafe { gpt.clock_gate(activity) };
    }
    /// Set the clock activity for the PIT
    pub fn clock_gate_pit(&mut self, pit: &mut crate::ral::pit::Instance, activity: ClockActivity) {
        unsafe { pit.clock_gate(activity) };
    }
    /// Returns the periodic clock frequency (Hz)
    pub const fn frequency() -> u32 {
        PERIODIC_CLOCK_FREQUENCY_HZ
    }
}

impl Disabled<PerClock> {
    /// Enable the periodic clock root
    pub fn enable(self, ccm: &mut Handle) -> PerClock {
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CSCMR1,
            PERCLK_CLK_SEL: PERCLK_CLK_SEL_1,
            PERCLK_PODF: PERIODIC_CLOCK_DIVIDER - 1
        );
        self.0
    }
}
