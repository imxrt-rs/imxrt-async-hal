//! Asynchronous iMX RT peripherals

#![no_std]

/// Decorates one or more functions that will be statically registered
/// in the interrupt table
///
/// `interrupts!` may only be used once per module. It should only include
/// function definitions. The function names should reflect the IRQ name as
/// provided by the RAL's `interrupt` macro.
macro_rules! interrupts {
    ($($isr:item)*) => {
        #[cfg(all(target_arch = "arm", feature = "rt"))]
        use crate::ral::interrupt;

        $(
            #[cfg_attr(all(target_arch = "arm", feature = "rt"), crate::rt::interrupt)]
            #[cfg_attr(any(not(target_arch = "arm"), not(feature = "rt")), allow(unused, non_snake_case))]
            $isr
        )*
    };
}

//
// Modules
//
pub mod ccm;
pub mod dma;
pub mod gpio;
mod gpt;
mod i2c;
pub mod instance;
mod pit;
mod spi;
mod uart;

pub use imxrt_ral as ral;

#[cfg(target_arch = "arm")]
use cortex_m_rt as rt;

//
// Module re-exports
//
pub use gpt::GeneralPurposeTimer as GPT;
pub use i2c::{ClockSpeed as I2CClockSpeed, Error as I2CError, I2C};
pub use pit::PeriodicTimer as PIT;
pub use spi::{Error as SPIError, Pins as SPIPins, SPI};
pub use uart::{Error as UARTError, UART};

const OSCILLATOR_FREQUENCY_HZ: u32 = 24_000_000;
const PERIODIC_CLOCK_FREQUENCY_HZ: u32 = OSCILLATOR_FREQUENCY_HZ / PERIODIC_CLOCK_DIVIDER;
const PERIODIC_CLOCK_DIVIDER: u32 = 24;

/// Enable the periodic clock root
fn enable_periodic_clock_root(ccm: &crate::ccm::Handle) {
    static ONCE: once::Once = once::new();
    ONCE.call(|| {
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CSCMR1,
            PERCLK_CLK_SEL: PERCLK_CLK_SEL_1,
            PERCLK_PODF: PERIODIC_CLOCK_DIVIDER - 1
        );
    });
}

/// A `once` sentinel, since it doesn't exist in `core::sync`.
mod once {
    use core::sync::atomic::{AtomicBool, Ordering};
    pub struct Once(AtomicBool);
    pub const fn new() -> Once {
        Once(AtomicBool::new(false))
    }
    impl Once {
        pub fn call<R, F: FnOnce() -> R>(&self, f: F) -> Option<R> {
            let already_called = self.0.swap(true, Ordering::SeqCst);
            if already_called {
                None
            } else {
                Some(f())
            }
        }
    }
}

/// The ARM clock frequency
///
/// See [`set_arm_clock`](fn.set_arm_clock.html) to specify the ARM clock speed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ARMClock {
    hz: u32,
}

/// The IPG clock frequency
///
/// See [`set_arm_clock`](fn.set_arm_clock.html) to specify the IPG clock speed.
/// Since the IPG clock speed is based on the ARM clock, the same function prepares
/// both clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IPGClock {
    hz: u32,
}

/// Pad multiplexing and configuration
///
/// The `iomuxc` module is a re-export of the [`imxrt-iomuxc`] crate. It combines
/// the i.MX RT processor-specific components with the `imxrt-iomuxc` general API.
/// It then adds a safe function, [`take`](fn.take.html), which lets you convert
/// the RAL's `iomuxc::Instance` into all of the processor [`Pads`](struct.Pads.html).
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::{ral::iomuxc::IOMUXC, iomuxc};
///
/// let pads = iomuxc::new(IOMUXC::take().unwrap());
/// ```
///
/// `Pads` can then be used in peripheral-specific APIs.
///
/// [`imxrt-iomuxc`]: https://docs.rs/imxrt-iomuxc/0.1/imxrt_iomuxc/
pub mod iomuxc {
    #[cfg(feature = "imxrt106x")]
    pub use imxrt_iomuxc::imxrt106x::*;
    pub use imxrt_iomuxc::prelude::*;

    /// Turn the `IOMUXC` instance into pads
    ///
    /// See the [module-level docs](index.html) for an example.
    #[cfg(any(feature = "imxrt106x"))]
    pub fn new(_: crate::ral::iomuxc::Instance) -> Pads {
        // Safety: ^--- there's a single instance. Either the user
        // used an `unsafe` method to steal it, or we own the only
        // instance.
        unsafe { Pads::new() }
    }
}
