//! Clock gate implementations for all instances and RAL types
//!
//! Intentionally not using the `instance` API so as to make this
//! module more portable across projects

use super::{ClockActivity, ClockGate, Handle};
use crate::ral;
use core::ptr;

/// Starting address of the clock control gate registers
const CCGR_BASE: *mut u32 = 0x400F_C068 as *mut u32;

/// # Safety
///
/// Should only be used when you have a mutable reference to the `ccm::Handle`.
/// Should only be used on a valid clock gate register.
#[inline(always)]
unsafe fn set_clock_gate(ccgr: *mut u32, gates: &[usize], value: u8) {
    const MASK: u32 = 0b11;
    let mut register = ptr::read_volatile(ccgr);

    for gate in gates {
        let shift: usize = gate * 2;
        register &= !(MASK << shift);
        register |= (MASK & (value as u32)) << shift;
    }

    ptr::write_volatile(ccgr, register);
}

impl ClockGate for ral::dma0::Instance {
    fn gate(&mut self, _: &mut Handle, activity: ClockActivity) {
        unsafe { set_clock_gate(CCGR_BASE.add(5), &[3], activity as u8) };
    }
}

impl ClockGate for ral::gpt::Instance {
    fn gate(&mut self, _: &mut Handle, activity: ClockActivity) {
        let value = activity as u8;
        match &**self as *const _ {
            ral::gpt::GPT1 => unsafe {
                set_clock_gate(CCGR_BASE.add(1), &[10, 11], value);
            },
            ral::gpt::GPT2 => unsafe { set_clock_gate(CCGR_BASE.add(0), &[12, 13], value) },
            _ => unreachable!(),
        }
    }
}

impl ClockGate for ral::lpi2c::Instance {
    fn gate(&mut self, _: &mut Handle, activity: ClockActivity) {
        let value = activity as u8;
        match &**self as *const _ {
            ral::lpi2c::LPI2C1 => unsafe { set_clock_gate(CCGR_BASE.add(2), &[3], value) },
            ral::lpi2c::LPI2C2 => unsafe { set_clock_gate(CCGR_BASE.add(2), &[4], value) },
            ral::lpi2c::LPI2C3 => unsafe { set_clock_gate(CCGR_BASE.add(2), &[5], value) },
            ral::lpi2c::LPI2C4 => unsafe { set_clock_gate(CCGR_BASE.add(6), &[12], value) },
            _ => unreachable!(),
        }
    }
}

impl ClockGate for ral::pit::Instance {
    fn gate(&mut self, _: &mut Handle, activity: ClockActivity) {
        match &**self as *const _ {
            ral::pit::PIT => unsafe { set_clock_gate(CCGR_BASE.add(1), &[6], activity as u8) },
            _ => unreachable!(),
        }
    }
}

impl ClockGate for ral::lpspi::Instance {
    fn gate(&mut self, _: &mut Handle, activity: ClockActivity) {
        unsafe {
            let ccgr = CCGR_BASE.add(1);
            let gate = match &**self as *const _ {
                ral::lpspi::LPSPI1 => 0,
                ral::lpspi::LPSPI2 => 1,
                ral::lpspi::LPSPI3 => 2,
                ral::lpspi::LPSPI4 => 3,
                _ => unreachable!(),
            };
            set_clock_gate(ccgr, &[gate], activity as u8);
        }
    }
}

impl ClockGate for ral::lpuart::Instance {
    fn gate(&mut self, _: &mut Handle, activity: ClockActivity) {
        let value = activity as u8;
        match &**self as *const _ {
            ral::lpuart::LPUART1 => unsafe { set_clock_gate(CCGR_BASE.add(5), &[12], value) },
            ral::lpuart::LPUART2 => unsafe { set_clock_gate(CCGR_BASE.add(0), &[14], value) },
            ral::lpuart::LPUART3 => unsafe { set_clock_gate(CCGR_BASE.add(0), &[6], value) },
            ral::lpuart::LPUART4 => unsafe { set_clock_gate(CCGR_BASE.add(1), &[12], value) },
            ral::lpuart::LPUART5 => unsafe { set_clock_gate(CCGR_BASE.add(3), &[1], value) },
            ral::lpuart::LPUART6 => unsafe { set_clock_gate(CCGR_BASE.add(3), &[3], value) },
            ral::lpuart::LPUART7 => unsafe { set_clock_gate(CCGR_BASE.add(5), &[13], value) },
            ral::lpuart::LPUART8 => unsafe { set_clock_gate(CCGR_BASE.add(6), &[7], value) },
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::set_clock_gate;

    #[test]
    fn test_set_clock_gate() {
        let mut reg = 0;

        unsafe {
            set_clock_gate(&mut reg, &[3, 7], 0b11);
        }
        assert_eq!(reg, (0b11 << 14) | (0b11 << 6));

        unsafe {
            set_clock_gate(&mut reg, &[3], 0b1);
        }
        assert_eq!(reg, (0b11 << 14) | (0b01 << 6));
    }
}
