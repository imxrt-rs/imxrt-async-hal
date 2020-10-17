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
//! Many clocks start in a disabled state. Each clock supports
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
//! perclock.clock_gate_gpt(&mut gpt2, ccm::ClockGate::On);
//! let mut gpt = GPT::new(gpt2, &perclock);
//! ```

#[cfg(feature = "i2c")]
mod i2c;
#[cfg(any(feature = "gpt", feature = "pit"))]
mod perclock;
#[cfg(feature = "imxrt1060")]
mod pll1;
#[cfg(feature = "spi")]
mod spi;
#[cfg(feature = "uart")]
mod uart;

#[cfg(feature = "i2c")]
pub use i2c::{clock_gate as clock_gate_i2c, enable as enable_i2c};
#[cfg(feature = "gpt")]
pub use perclock::clock_gate_gpt;
#[cfg(feature = "pit")]
pub use perclock::clock_gate_pit;
#[cfg(any(feature = "gpt", feature = "pit"))]
pub use perclock::enable as enable_perclock;
#[cfg(feature = "spi")]
pub use spi::{clock_gate as clock_gate_spi, enable as enable_spi};
#[cfg(feature = "uart")]
pub use uart::{clock_gate as clock_gate_uart, enable as enable_uart};

use crate::ral;

/// Handle to the CCM register block
///
/// `Handle` also supports clock gating for peripherals that
/// don't have an obvious clock root, like DMA.
pub struct Handle(pub(crate) ral::ccm::Instance);

impl Handle {
    /// Set the clock gate for the DMA controller
    ///
    /// You should set the clock gate before creating DMA channels. Otherwise, the DMA
    /// peripheral may not work.
    #[cfg(any(feature = "pipe", feature = "spi", feature = "uart"))]
    #[cfg_attr(
        docsrs,
        doc(cfg(any(feature = "pipe", feature = "spi", feature = "uart")))
    )]
    pub fn clock_gate_dma(&mut self, dma: &mut ral::dma0::Instance, gate: ClockGate) {
        unsafe { clock_gate_dma(&**dma, gate) };
    }
}

/// Set the clock gate for the DMA controller
///
/// # Safety
///
/// This could be called by anyone who can access the DMA register block, which is always
/// available. Consider using [`Handle::clock_gate_dma`](struct.Handle.html#method.clock_gate_dma)
/// which supports a safer interface.
#[cfg(any(feature = "pipe", feature = "spi", feature = "uart"))]
#[cfg_attr(
    docsrs,
    doc(cfg(any(feature = "pipe", feature = "spi", feature = "uart")))
)]
pub unsafe fn clock_gate_dma(_: *const ral::dma0::RegisterBlock, gate: ClockGate) {
    set_clock_gate(CCGR_BASE.add(5), &[3], gate as u8);
}

/// The root clocks and CCM handle
///
/// Most root clocks are disabled. Call `enable`, and supply the
/// `handle`, to enable them.
#[non_exhaustive]
pub struct CCM {
    /// The handle to the CCM register block
    ///
    /// `Handle` is used throughout the HAL
    pub handle: Handle,
    /// PLL1, which controls the ARM clock
    #[cfg(feature = "imxrt1060")]
    pub pll1: PLL1,
    /// The periodic clock handle
    ///
    /// `perclock` is used for timers, including [`GPT`](../struct.GPT.html) and [`PIT`](../struct.PIT.html).
    #[cfg(any(feature = "gpt", feature = "pit"))]
    #[cfg_attr(docsrs, doc(cfg(any(feature = "gpt", feature = "pit"))))]
    pub perclock: Disabled<PerClock>,
    /// The UART clock
    ///
    /// `uart_clock` is for [`UART`](../struct.UART.html) peripherals.
    #[cfg(feature = "uart")]
    #[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
    pub uart_clock: Disabled<UARTClock>,
    /// The SPI clock
    ///
    /// `spi_clock` is for [`SPI`](../struct.SPI.html) peripherals.
    #[cfg(feature = "spi")]
    #[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
    pub spi_clock: Disabled<SPIClock>,
    /// The I2C clock
    ///
    /// `i2c_clock` is for [`I2C`](../struct.I2C.html) peripherals.
    #[cfg(feature = "i2c")]
    #[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
    pub i2c_clock: Disabled<I2CClock>,
}

impl CCM {
    /// Construct a new CCM from the RAL's CCM instance
    pub const fn new(ccm: ral::ccm::Instance) -> Self {
        CCM {
            handle: Handle(ccm),
            #[cfg(feature = "imxrt1060")]
            pll1: PLL1(()),
            #[cfg(any(feature = "gpt", feature = "pit"))]
            perclock: Disabled(PerClock(())),
            #[cfg(feature = "uart")]
            uart_clock: Disabled(UARTClock(())),
            #[cfg(feature = "spi")]
            spi_clock: Disabled(SPIClock(())),
            #[cfg(feature = "i2c")]
            i2c_clock: Disabled(I2CClock(())),
        }
    }
}

/// Describes a clock gate setting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ClockGate {
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
#[allow(unused)] // Used when features are enabled
const OSCILLATOR_FREQUENCY_HZ: u32 = 24_000_000;

/// A disabled clock of type `Clock`
///
/// Call `enable` on your instance to enable the clock.
pub struct Disabled<Clock>(Clock);

/// The periodic clock root
///
/// `PerClock` is the input clock for GPT and PIT. It runs at
/// 1MHz.
#[cfg(any(feature = "gpt", feature = "pit"))]
#[cfg_attr(docsrs, doc(cfg(any(feature = "gpt", feature = "pit"))))]
pub struct PerClock(());

#[cfg(any(feature = "gpt", feature = "pit"))]
#[cfg_attr(docsrs, doc(cfg(any(feature = "gpt", feature = "pit"))))]
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
#[cfg(feature = "uart")]
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
pub struct UARTClock(());

#[cfg(feature = "uart")]
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
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
#[cfg(feature = "spi")]
#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
pub struct SPIClock(());

#[cfg(feature = "spi")]
#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
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
#[cfg(feature = "i2c")]
#[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
pub struct I2CClock(());

#[cfg(feature = "i2c")]
#[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
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

/// PLL1, which controls the ARM and IPG clocks
///
/// This clock is enabled by default. Use [`set_arm_clock`](#method.set_arm_clock)
/// to specify the ARM clock frequency.
#[cfg(feature = "imxrt1060")]
#[cfg_attr(docsrs, doc(cfg(feature = "imxrt1060")))]
pub struct PLL1(());
// TODO PLL1 might be a thing on non 106x chips. Ian is just
// hiding it completely for simplicity.

#[cfg(feature = "imxrt1060")]
impl PLL1 {
    /// Consume the PLL1 and set the ARM clock speed, returning the ARM and IPG clock frequencies
    ///
    /// # Safety
    ///
    /// This is safe to call as long as you're not depending on any register state from
    ///
    /// - CCM_ANALOG
    /// - DCDC
    ///
    /// This function may modify any register in those two additional register blocks, in addition to
    /// the CCM memory encapsulated in `Handle`.
    ///
    /// # Example
    ///
    /// Set the ARM clock to 600MHz:
    ///
    /// ```no_run
    /// use imxrt_async_hal as hal;
    /// use hal::{ral::ccm::CCM, ccm};
    /// let ccm::CCM { mut handle, pll1, .. } = CCM::take().map(ccm::CCM::new).unwrap();
    ///
    /// unsafe {
    ///     pll1.set_arm_clock(600_000_000, &mut handle);
    /// }
    /// ```
    pub unsafe fn set_arm_clock(self, clock_hz: u32, ccm: &mut Handle) -> (ARMClock, IPGClock) {
        let (arm, ipg) = pll1::set_arm_clock(
            clock_hz,
            &*ccm.0,
            ral::ccm_analog::CCM_ANALOG,
            ral::dcdc::DCDC,
        );
        (ARMClock { hz: arm }, IPGClock { hz: ipg })
    }
}

/// The ARM clock frequency
///
/// See [`PLL1`](struct.PLL1.html) to set the ARM clock and acquire this frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ARMClock {
    hz: u32,
}

/// The IPG clock frequency
///
/// /// See [`PLL1`](struct.PLL1.html) to set the IPG clock and acquire this frequency.
///
/// Since the IPG clock speed is based on the ARM clock, the same function prepares
/// both clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IPGClock {
    hz: u32,
}

/// Starting address of the clock control gate registers
#[allow(unused)] // Used when features are enabled
const CCGR_BASE: *mut u32 = 0x400F_C068 as *mut u32;

/// # Safety
///
/// Should only be used when you have a mutable reference to an enabled clock.
/// Should only be used on a valid clock gate register.
#[inline(always)]
#[allow(unused)] // Used when features are enabled
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
