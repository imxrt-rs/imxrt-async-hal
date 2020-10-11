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
//! let CCM{
//!     mut handle,
//!     // All clocks below are disabled;
//!     // call enable() to enable them
//!     perclock,
//!     spi_clock,
//!     uart_clock,
//!     i2c_clock,
//!     ..
//! } = ral::ccm::CCM::take().map(CCM::new).unwrap();
//!
//! // Enable the periodic clock root for GPTs and PITs
//! let mut perclock = perclock.enable(&mut handle);
//! ```
//!
//! As shown above, all clocks start in a disabled state. Each clock supports
//! an `enable` method for enabling the clock root. Once you have an `enabled`
//! clock, you can use it to control clock gates for your peripheral:
//!
//! ```no_run
//! # use imxrt_async_hal as hal;
//! use hal::{
//!     ral::gpt::GPT2, // the RAL GPT2 instance
//!     GPT,            // the async GPT driver
//! };
//! # use hal::{
//! #     ral::ccm::CCM, // the RAL CCM instance
//! #     ccm,           // the async CCM API
//! # };
//!
//! let mut gpt2 = GPT2::take().unwrap();
//! # let ccm::CCM{ mut handle, perclock, .. } = CCM::take().map(ccm::CCM::new).unwrap();
//! # let mut perclock = perclock.enable(&mut handle);
//! // Turn on the clocks for the GPT2 timer
//! perclock.clock_gate_gpt(&mut gpt2, ccm::ClockActivity::On);
//! let mut gpt = GPT::new(gpt2, &perclock);
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

/// The root clocks and CCM handle
///
/// All root clocks are disabled. Call `enable`, and supply the
/// `handle`, to enable them.
#[non_exhaustive]
pub struct CCM {
    /// The handle to the CCM register block
    ///
    /// `Handle` is used throughout the HAL
    pub handle: Handle,
    /// The periodic clock handle
    ///
    /// `perclock` is used for timers, including [`GPT`](../struct.GPT.html) and [`PIT`](../struct.PIT.html).
    pub perclock: Disabled<PerClock>,
    /// The UART clock
    ///
    /// `uart_clock` is for [`UART`](../struct.UART.html) peripherals.
    pub uart_clock: Disabled<UARTClock>,
    /// The SPI clock
    ///
    /// `spi_clock` is for [`SPI`](../struct.SPI.html) peripherals.
    pub spi_clock: Disabled<SPIClock>,
    /// The I2C clock
    ///
    /// `i2c_clock` is for [`I2C`](../struct.I2C.html) peripherals.
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

/// Crystal oscillator frequency
const OSCILLATOR_FREQUENCY_HZ: u32 = 24_000_000;

/// A disabled clock of type `Clock`
///
/// Call `enable` on your instance to enable the clock.
pub struct Disabled<Clock>(Clock);

/// The periodic clock root
///
/// `PerClock` is the input clock for GPT and PIT. It runs at
/// 1MHz.
pub struct PerClock(());

impl PerClock {
    /// Assume that the clock is enabled, and acquire the enabled clock
    ///
    /// # Safety
    ///
    /// This may create an alias to memory that is mutably owned by another instance.
    /// Users should only `assume_enabled` when configuring clocks through another
    /// API.
    pub unsafe fn assume_enabled() -> Self {
        Self(())
    }
}

/// The UART clock
pub struct UARTClock(());

impl UARTClock {
    /// Assume that the clock is enabled, and acquire the enabled clock
    ///
    /// # Safety
    ///
    /// This may create an alias to memory that is mutably owned by another instance.
    /// Users should only `assume_enabled` when configuring clocks through another
    /// API.
    pub unsafe fn assume_enabled() -> Self {
        Self(())
    }
}

/// The SPI clock
pub struct SPIClock(());

impl SPIClock {
    /// Assume that the clock is enabled, and acquire the enabled clock
    ///
    /// # Safety
    ///
    /// This may create an alias to memory that is mutably owned by another instance.
    /// Users should only `assume_enabled` when configuring clocks through another
    /// API.
    pub unsafe fn assume_enabled() -> Self {
        Self(())
    }
}

/// The I2C clock
pub struct I2CClock(());

impl I2CClock {
    /// Assume that the clock is enabled, and acquire the enabled clock
    ///
    /// # Safety
    ///
    /// This may create an alias to memory that is mutably owned by another instance.
    /// Users should only `assume_enabled` when configuring clocks through another
    /// API.
    pub unsafe fn assume_enabled() -> Self {
        Self(())
    }
}

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
