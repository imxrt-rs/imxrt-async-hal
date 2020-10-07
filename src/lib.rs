//! Asynchronous iMX RT peripherals

#![no_std]

//
// Modules
//
pub mod dma;
pub mod gpio;
mod gpt;
mod i2c;
pub mod instance;
mod pit;
mod spi;
mod uart;

pub use imxrt_ral as ral;

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
fn enable_periodic_clock_root(ccm: &ral::ccm::Instance) {
    static ONCE: once::Once = once::new();
    ONCE.call(|| {
        ral::modify_reg!(
            ral::ccm,
            ccm,
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
mod iomuxc {
    pub use imxrt_iomuxc::prelude::*;
}
