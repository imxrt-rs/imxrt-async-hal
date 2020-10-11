//! UART clock control

use super::{set_clock_gate, ClockActivity, Disabled, Handle, UARTClock, CCGR_BASE};
use crate::ral;

#[cfg(feature = "uart")]
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
impl Disabled<UARTClock> {
    /// Enable the UART clocks
    pub fn enable(self, handle: &mut Handle) -> UARTClock {
        unsafe { enable(&*handle.0) };
        self.0
    }
}

#[cfg(feature = "uart")]
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
impl UARTClock {
    /// Set the clock gate activity for the UART instance
    pub fn clock_gate(&mut self, uart: &mut ral::lpuart::Instance, activity: ClockActivity) {
        unsafe { clock_gate(&**uart, activity) }
    }

    /// Returns the UART clock frequency (Hz)
    pub const fn frequency() -> u32 {
        super::OSCILLATOR_FREQUENCY_HZ
    }
}

/// Set the clock gate activity for a UART peripheral
///
/// # Safety
///
/// This could be called anywhere, by anyone who uses the globally-accessible UART memory.
/// Consider using the safer `UARTClock::clock_gate` API.
#[cfg(feature = "uart")]
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
pub unsafe fn clock_gate(uart: *const ral::lpuart::RegisterBlock, activity: ClockActivity) {
    let value = activity as u8;
    match uart {
        ral::lpuart::LPUART1 => set_clock_gate(CCGR_BASE.add(5), &[12], value),
        ral::lpuart::LPUART2 => set_clock_gate(CCGR_BASE.add(0), &[14], value),
        ral::lpuart::LPUART3 => set_clock_gate(CCGR_BASE.add(0), &[6], value),
        ral::lpuart::LPUART4 => set_clock_gate(CCGR_BASE.add(1), &[12], value),
        ral::lpuart::LPUART5 => set_clock_gate(CCGR_BASE.add(3), &[1], value),
        ral::lpuart::LPUART6 => set_clock_gate(CCGR_BASE.add(3), &[3], value),
        ral::lpuart::LPUART7 => set_clock_gate(CCGR_BASE.add(5), &[13], value),
        ral::lpuart::LPUART8 => set_clock_gate(CCGR_BASE.add(6), &[7], value),
        _ => unreachable!(),
    }
}

/// Enable the UART clock root
///
/// # Safety
///
/// This modifies easily-accessible global state. Consider using `UartClock::enable`
/// for a safery API.
#[inline(always)]
#[cfg(feature = "uart")]
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
pub unsafe fn enable(ccm: *const ral::ccm::RegisterBlock) {
    ral::modify_reg!(
        ral::ccm,
        ccm,
        CSCDR1,
        UART_CLK_SEL: UART_CLK_SEL_1, // Oscillator
        UART_CLK_PODF: DIVIDE_1
    );
}
