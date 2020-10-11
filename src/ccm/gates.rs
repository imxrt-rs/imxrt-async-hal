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
