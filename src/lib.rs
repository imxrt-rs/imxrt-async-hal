//! Embedded, async Rust for i.MX RT processors
//!
//! `imxrt-async-hal` brings async Rust support to NXP's i.MX RT processors.
//! The crate includes `await`able peripherals and timers. Once the I/O completes
//! or the timer elapses, an interrupt fires to wake the executor. By combining
//! interrupt-driven peripherals with a single-threaded executor, you can write
//! multiple, concurrent tasks for your embedded system.
//!
//! The crate registers interrupt handlers to support async execution. The implementation
//! registers interrupt handlers statically, using the [`cortex-m-rt`] interfaces. This
//! means that your final program should also depend on `cortex-m-rt`, or bet at least
//! `cortex-m-rt` compatible.
//!
//! [`cortex-m-rt`]: https://crates.io/crates/cortex-m-rt
//!
//! The crate does not include an executor, or any API for driving futures. You will
//! need to select your own executor that supports a Cortex-M system.
//! The executor should be thread safe, prepared to handle wakes from interrupt handlers.
//!
//! See the project's examples to try this code on your hardware. This crate has been
//! primarily developed using a Teensy 4 (i.MX RT 1062). It compiles for other
//! i.MX RT chip variants.
//!
//! # Dependencies
//!
//! - A Rust installation; recommended installation using `rustup`. We support the
//!   latest, stable Rust toolchain.
//!
//! - The `thumbv7em-none-eabihf` Rust target, which may be installed using
//!   `rustup`: `rustup target add thumbv7em-none-eabihf`
//!
//!   The target is only necessary when building for an embedded system. The
//!   main crate should build and test on your host.
//!
//! - An embedded system with a compatible i.MX RT processor.
//!
//! # Feature flags
//!
//! You're **required** to specify a feature that describes your i.MX RT chip variant.
//! You may select only one chip feature.
//!
//! The crate compiles for the following chips:
//!
//! - `"imxrt1010"` for i.MX RT **1010** variants
//! - `"imxrt1060"` for i.MX RT **1060** variants
//!
//! Each peripheral has it's own feature, which is enabled by default. However, you may
//! want to disable some peripherals because you have your own interrupt-driven peripheral,
//! and the interrupt handler that this crate provides causes a duplicate definition
//!
//! To select peripherals, disable the crate's default features. Then, select one or more of
//! the peripheral features from the table. The checkmarks indicate a chip's support for
//! that peripheral.
//!
//! | **Chip**  | `"gpio"` | `"gpt"` | `"i2c"` | `"pipe"` | `"pit"` | `"spi"` | `"uart"` |
//! | --------- | -------- | ------- | ------- | -------- | ------- | ------- | -------- |
//! | imxrt1010 |    ✓     |    ✓    |    ✓    |    ✓     |    ✓    |    ✓    |     ✓    |
//! | imxrt1060 |    ✓     |    ✓    |    ✓    |    ✓     |    ✓    |    ✓    |     ✓    |
//!
//! When developing a binary for your embedded system, you should enable this crate's `"rt"`
//! feature. Otherwise, when developing libraries against the crate, you may skip the
//! `"rt"` feature.
//!
//! # Examples
//!
//! Each module contains a small getting started example to demonstrate driver setup and
//! the driver API. For larger examples that run on hardware, see the `imxrt-async-hal`
//! repository.
//!
//! ## License
//!
//! Licensed under either of
//!
//! - [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0)
//! - [MIT License](http://opensource.org/licenses/MIT)
//!
//! at your option.
//!
//! Unless you explicitly state otherwise, any contribution intentionally submitted
//! for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
//! dual licensed as above, without any additional terms or conditions.

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

// Developer note: you'll find compile_error!s like this scattered
// throughout the implementation. The errors will point you towards
// things that you need to consider when adding a new chip. Once
// you've added support for that new chip, you should update the
// comditional compile.
#[cfg(not(any(feature = "imxrt1010", feature = "imxrt1060")))]
compile_error!(concat!(
    "You must select a chip feature flag! Available chips:\n",
    "  - imxrt1010\n",
    "  - imxrt1060\n"
));

/// Decorates one or more functions that act as interrupt handlers.
///
/// `interrupts!` may only be used once per module. It should only include
/// functions wrapped by `handler!`. The function names should reflect the
/// IRQ name as provided by the RAL's `interrupt` macro.
#[cfg(any(
    feature = "gpio",
    feature = "gpt",
    feature = "i2c",
    feature = "pit",
    feature = "pipe",
    feature = "spi",
    feature = "uart",
))]
macro_rules! interrupts {
    ($($handlers:item)*) => {
        #[cfg(all(target_arch = "arm", feature = "rt"))]
        use crate::ral::interrupt;
        $($handlers)*
    };
}

/// Decorator helper for an interrupt handler
#[cfg(any(
    feature = "gpio",
    feature = "gpt",
    feature = "i2c",
    feature = "pit",
    feature = "pipe",
    feature = "spi",
    feature = "uart",
))]
macro_rules! handler {
    (unsafe fn $isr_name:ident () $body:block) => {
        #[cfg_attr(all(target_arch = "arm", feature = "rt"), crate::rt::interrupt)]
        #[cfg_attr(any(not(target_arch = "arm"), not(feature = "rt")), allow(unused, non_snake_case))]
        unsafe fn $isr_name() $body
    };
    (fn $isr_name:ident () $ body:block) => {
        #[cfg_attr(all(target_arch = "arm", feature = "rt"), crate::rt::interrupt)]
        #[cfg_attr(any(not(target_arch = "arm"), not(feature = "rt")), allow(unused, non_snake_case))]
        fn $isr_name() $body
    };
}

//
// Modules
//
pub mod ccm {
    //! Clock control module (CCM)
    //!
    //! The clocks and types exposed in `ccm` support clock control and peripheral clock
    //! gating. Use [`CCM::from_ral`](CCM) to acquire the clock roots and the
    //! CCM handle. Then, enable your clocks.
    //!
    //! ```no_run
    //! use imxrt_async_hal as hal;
    //! use hal::{ccm, ral};
    //!
    //! let ccm::CCM {
    //!     mut handle,
    //!     uart_clock,
    //!     ..
    //! } = ral::ccm::CCM::take().map(ccm::CCM::from_ral).unwrap();
    //!
    //! let mut uart_clock = uart_clock.enable(&mut handle);
    //! ```
    //!
    //! Clocks can enable peripheral clock gates, and they may be used in APIs that require
    //! you to first initialize clocks.
    //!
    //! ```no_run
    //! # use imxrt_async_hal as hal;
    //! # use hal::{ccm, ral};
    //! # let ccm::CCM {
    //! #     mut handle,
    //! #     uart_clock,
    //! #     ..
    //! # } = ral::ccm::CCM::take().map(ccm::CCM::from_ral).unwrap();
    //! # let mut uart_clock = uart_clock.enable(&mut handle);
    //! type UART2 = hal::instance::UART<hal::iomuxc::consts::U2>;
    //! let mut lpuart2: UART2 = ral::lpuart::LPUART2::take().and_then(hal::instance::uart).unwrap();
    //!
    //! // Enable the clock gate:
    //! uart_clock.set_clock_gate(&mut lpuart2, ccm::ClockGate::On);
    //!
    //! // Create the peripheral... see UART documentation for more information.
    //! ```

    pub use imxrt_ccm::{
        ral::{I2CClock, PerClock, SPIClock, UARTClock, CCM},
        ClockGate, Handle,
    };
}
#[cfg(any(feature = "pipe", feature = "spi", feature = "uart"))]
#[cfg_attr(
    docsrs,
    doc(cfg(any(feature = "pipe", feature = "spi", feature = "uart")))
)]
pub mod dma;
#[cfg(feature = "gpio")]
#[cfg_attr(docsrs, doc(cfg(feature = "gpio")))]
pub mod gpio;
#[cfg(feature = "gpt")]
mod gpt;
#[cfg(feature = "i2c")]
mod i2c;
pub mod instance;
#[cfg(feature = "pit")]
mod pit;
#[cfg(feature = "spi")]
mod spi;
#[cfg(feature = "uart")]
mod uart;

pub use imxrt_ral as ral;

#[cfg(all(target_arch = "arm", feature = "rt"))]
use cortex_m_rt as rt;

//
// Module re-exports
//
#[cfg(feature = "gpt")]
pub use gpt::GPT;
#[cfg(feature = "i2c")]
pub use i2c::{ClockSpeed as I2CClockSpeed, Error as I2CError, I2C};
#[cfg(feature = "pit")]
pub use pit::PIT;
#[cfg(feature = "spi")]
pub use spi::{Error as SPIError, Pins as SPIPins, SPI};
#[cfg(feature = "uart")]
pub use uart::{Error as UARTError, UART};

/// A `once` sentinel, since it doesn't exist in `core::sync`.
#[cfg(any(feature = "gpio", feature = "i2c"))]
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

/// Pad multiplexing and configuration
///
/// The `iomuxc` module is a re-export of the [`imxrt-iomuxc`] crate. It combines
/// the i.MX RT processor-specific components with the `imxrt-iomuxc` general API.
/// It then adds a safe function, `take`, which lets you convert
/// the RAL's `iomuxc::Instance` into all of the processor [`Pads`](crate::iomuxc::pads::Pads).
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
    #[cfg(not(any(feature = "imxrt1010", feature = "imxrt1060")))]
    compile_error!("Ensure that your chip has imxrt-iomuxc support");

    pub mod pads {
        // The imxrt1010 module has a group of pads that are named 'gpio'. It
        // conflicts with the gpio module exported in the prelude. We're wrapping
        // the pads in a pads module to make the distinction clear.
        #[cfg(feature = "imxrt1010")]
        pub use imxrt_iomuxc::imxrt101x::*;
        #[cfg(feature = "imxrt1060")]
        pub use imxrt_iomuxc::imxrt106x::*;
    }
    pub use imxrt_iomuxc::prelude::*;

    /// Turn the `IOMUXC` instance into pads
    ///
    /// ```no_run
    /// use imxrt_async_hal as hal;
    /// use hal::{ral::iomuxc::IOMUXC, iomuxc};
    ///
    /// let pads = iomuxc::new(IOMUXC::take().unwrap());
    /// ```
    #[cfg_attr(docsrs, doc(cfg(any(feature = "imxrt1010", feature = "imxrt1060"))))]
    #[cfg(any(feature = "imxrt1010", feature = "imxrt1060"))]
    pub fn new(_: crate::ral::iomuxc::Instance) -> pads::Pads {
        // Safety: ^--- there's a single instance. Either the user
        // used an `unsafe` method to steal it, or we own the only
        // instance.
        unsafe { pads::Pads::new() }
    }
}
