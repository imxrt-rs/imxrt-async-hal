//! UART clock control

use super::{set_clock_gate, ClockGate, Disabled, Handle, UARTClock, CCGR_BASE};
use crate::ral;

#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
impl Disabled<UARTClock> {
    /// Enable the UART clocks
    pub fn enable(self, handle: &mut Handle) -> UARTClock {
        unsafe { enable(&*handle.0) };
        self.0
    }
}

impl UARTClock {
    /// Set the clock gate for the UART instance
    pub fn clock_gate(&mut self, uart: &mut ral::lpuart::Instance, gate: ClockGate) {
        unsafe { clock_gate(&**uart, gate) }
    }

    /// Returns the UART clock frequency (Hz)
    pub const fn frequency() -> u32 {
        super::OSCILLATOR_FREQUENCY_HZ
    }
}

/// Set the clock gate for a UART peripheral
///
/// # Safety
///
/// This could be called anywhere, by anyone who uses the globally-accessible UART memory.
/// Consider using the safer `UARTClock::clock_gate` API.
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
pub unsafe fn clock_gate(uart: *const ral::lpuart::RegisterBlock, gate: ClockGate) {
    // Make sure that the match expression will never hit the unreachable!() case.
    // The comments and conditional compiles show what we're currently considering in
    // that match. If your chip isn't listed, it's not something we considered.
    #[cfg(not(any(feature = "imxrt1010", feature = "imxrt1060")))]
    compile_error!("Ensure that LPUART clock gates are correct");

    let value = gate as u8;
    match uart {
        // imxrt1010, imxrt1060
        ral::lpuart::LPUART1 => set_clock_gate(CCGR_BASE.add(5), &[12], value),
        // imxrt1010, imxrt1060
        ral::lpuart::LPUART2 => set_clock_gate(CCGR_BASE.add(0), &[14], value),
        // imxrt1010, imxrt1060
        ral::lpuart::LPUART3 => set_clock_gate(CCGR_BASE.add(0), &[6], value),
        // imxrt1010, imxrt1060
        ral::lpuart::LPUART4 => set_clock_gate(CCGR_BASE.add(1), &[12], value),
        #[cfg(feature = "imxrt1060")]
        ral::lpuart::LPUART5 => set_clock_gate(CCGR_BASE.add(3), &[1], value),
        #[cfg(feature = "imxrt1060")]
        ral::lpuart::LPUART6 => set_clock_gate(CCGR_BASE.add(3), &[3], value),
        #[cfg(feature = "imxrt1060")]
        ral::lpuart::LPUART7 => set_clock_gate(CCGR_BASE.add(5), &[13], value),
        #[cfg(feature = "imxrt1060")]
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
