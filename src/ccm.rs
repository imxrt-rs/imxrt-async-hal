//! Clock control module (CCM)
//!
//! The CCM wraps the RAL's CCM instance. It provides control and selection
//! for peripheral root clocks, as well as individual clock gates for
//! periphral instances. Consider contstructing a CCM early in initialization,
//! since it's used throughout the HAL APIs:
//!
//! ```no_run
//! use imxrt_async_hal as hal;
//! use hal::{ccm::CCM, ral};
//!
//! let mut ccm = ral::ccm::CCM::take().map(CCM::new).unwrap();
//! ```

mod i2c;
mod perclock;
mod spi;
mod uart;

pub use i2c::{clock_gate as clock_gate_i2c, enable as enable_i2c};
pub use perclock::{clock_gate_gpt, clock_gate_pit, enable as enable_perclock};
pub use spi::{clock_gate as clock_gate_spi, enable as enable_spi};
pub use uart::{clock_gate as clock_gate_uart, enable as enable_uart};

use crate::ral;

/// Handle to the CCM register block
///
/// `Handle` also supports clock gating for peripherals that
/// don't have an obvious clock root, like DMA.
pub struct Handle(pub(crate) ral::ccm::Instance);

impl Handle {
    /// Set the clock gate activity for the DMA controller
    pub fn clock_gate_dma(&mut self, dma: &mut ral::dma0::Instance, activity: ClockActivity) {
        unsafe { clock_gate_dma(&**dma, activity) };
    }
}

/// Set the clock activity for the DMA controller
///
/// # Safety
///
/// This could be called by anyone who can access the DMA register block, which is always
/// available. Consider using [`Handle::clock_gate_dma`](struct.Handle.html#method.clock_gate_dma)
/// which supports a safer interface.
pub unsafe fn clock_gate_dma(_: *const ral::dma0::RegisterBlock, activity: ClockActivity) {
    set_clock_gate(CCGR_BASE.add(5), &[3], activity as u8);
}

/// The CCM components
#[non_exhaustive]
pub struct CCM {
    /// The handle to the CCM register block
    ///
    /// `Handle` is used throughout the HAL
    pub handle: Handle,
    /// The periodic clock handle
    pub perclock: Disabled<PerClock>,
    /// The UART clock
    pub uart_clock: Disabled<UARTClock>,
    /// The SPI clock
    pub spi_clock: Disabled<SPIClock>,
    /// The I2C clock
    pub i2c_clock: Disabled<I2CClock>,
}

impl CCM {
    /// Construct a new CCM from the RAL's CCM instance
    pub const fn new(ccm: ral::ccm::Instance) -> Self {
        CCM {
            handle: Handle(ccm),
            perclock: Disabled(PerClock(())),
            uart_clock: Disabled(UARTClock(())),
            spi_clock: Disabled(SPIClock(())),
            i2c_clock: Disabled(I2CClock(())),
        }
    }
}

/// Describes a clock gate setting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ClockActivity {
    /// Clock is off during all modes
    ///
    /// Stop enter hardware handshake is disabled.
    Off = 0b00,
    /// Clock is on in run mode, but off in wait and stop modes
    OnlyRun = 0b01,
    /// Clock is on in all modes, except stop mode
    On = 0b11,
}

/// Describes a type that can have its clock gated by the CCM
pub trait ClockGate {
    /// Gate the clock based, setting the value to the clock activity
    ///
    /// # Safety
    ///
    /// `clock_gate` modifies global state that may be mutably aliased elsewhere.
    /// Consider using the safer CCM APIs to specify your clock gate activity.
    unsafe fn clock_gate(&self, activity: ClockActivity);
}

/// Crystal oscillator frequency
// TODO should be private
pub(crate) const OSCILLATOR_FREQUENCY_HZ: u32 = 24_000_000;

/// A disabled clock of type `Clock`
///
/// Call `enable` on your instance to enable the clock.
pub struct Disabled<Clock>(Clock);

/// The periodic clock root
///
/// `PerClock` is the input clock for GPT and PIT. It runs at
/// 1MHz.
pub struct PerClock(());

/// The UART clock
pub struct UARTClock(());

/// The SPI clock
pub struct SPIClock(());

/// The I2C clock
pub struct I2CClock(());

/// Starting address of the clock control gate registers
const CCGR_BASE: *mut u32 = 0x400F_C068 as *mut u32;

/// # Safety
///
/// Should only be used when you have a mutable reference to an enabled clock.
/// Should only be used on a valid clock gate register.
#[inline(always)]
unsafe fn set_clock_gate(ccgr: *mut u32, gates: &[usize], value: u8) {
    const MASK: u32 = 0b11;
    let mut register = core::ptr::read_volatile(ccgr);

    for gate in gates {
        let shift: usize = gate * 2;
        register &= !(MASK << shift);
        register |= (MASK & (value as u32)) << shift;
    }

    core::ptr::write_volatile(ccgr, register);
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
