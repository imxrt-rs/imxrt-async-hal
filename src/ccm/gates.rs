//! Clock gate implementations for all instances and RAL types
//!
//! Intentionally not using the `instance` API so as to make this
//! module more portable across projects

use super::{set_clock_gate, ClockActivity, ClockGate, CCGR_BASE};
use crate::ral;

impl ClockGate for ral::lpi2c::RegisterBlock {
    unsafe fn clock_gate(&self, activity: ClockActivity) {
        let value = activity as u8;
        match &*self as *const _ {
            ral::lpi2c::LPI2C1 => set_clock_gate(CCGR_BASE.add(2), &[3], value),
            ral::lpi2c::LPI2C2 => set_clock_gate(CCGR_BASE.add(2), &[4], value),
            ral::lpi2c::LPI2C3 => set_clock_gate(CCGR_BASE.add(2), &[5], value),
            ral::lpi2c::LPI2C4 => set_clock_gate(CCGR_BASE.add(6), &[12], value),
            _ => unreachable!(),
        }
    }
}

impl ClockGate for ral::lpspi::RegisterBlock {
    unsafe fn clock_gate(&self, activity: ClockActivity) {
        let ccgr = CCGR_BASE.add(1);
        let gate = match &*self as *const _ {
            ral::lpspi::LPSPI1 => 0,
            ral::lpspi::LPSPI2 => 1,
            ral::lpspi::LPSPI3 => 2,
            ral::lpspi::LPSPI4 => 3,
            _ => unreachable!(),
        };
        set_clock_gate(ccgr, &[gate], activity as u8);
    }
}
