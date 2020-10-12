//! Asynchronous i.MX RT peripherals for embedded systems
//!
//! `imxrt-async-hal` brings async Rust support to NXP's i.MX RT processor family.
//! The crate includes peripherals and timers. Peripheral I/O blocks on `await`, and
//! timer delays can be `await`ed.
//!
//! The crate registers and manages the interrupt handlers necessary for
//! waking the executor. The implementation registers interrupt handlers statically,
//! using the [`cortex-m-rt`] interfaces. This means that your end system should also
//! depend on `cortex-m-rt`, or at least be `cortex-m-rt` compatible.
//!
//! [`cortex-m-rt`]: https://crates.io/crates/cortex-m-rt
//!
//! The crate does not include an executor, or any API for driving the futures. You will
//! need to select your own Cortex-M-compatible executor. The executor should be thread safe,
//! prepared to handle wakes from interrupt handlers.
//!
//! # Feature Flags
//!
//! You're required to specify a feature flag that describes your i.MX RT chip variant.
//! You may only select one chip feature.
//!
//! The current implementation supports
//!
//! - `"imxrt106x"` for i.MX RT 1060 variants
//!
//! Each peripheral has it's own feature flag, which is enabled by default. However, you may
//! want to disable some peripherals because
//!
//! - you have your own async implementation you'd like to use, or
//! - you have your own interrupt-driven implementation, and the interrupt handler that this
//!   crate registers causes a duplicate definition
//!
//! To select peripherals, disable the crate's default features. Then, select one or more of
//! the peripheral features:
//!
//! - `"gpio"`
//! - `"gpt"`
//! - `"i2c"`
//! - `"pipe"`
//! - `"pit"`
//! - `"spi"`
//! - `"uart"`
//!
//! When you're developing a binary for your embedded system, you should specify the `"rt"`
//! feature flag. Otherwise, when developing libraries against the crate, you may skip the
//! `"rt"` flag.
//!
//! # Core APIs
//!
//! The `imxrt-async-hal` relies on some core APIs to prepare peripherals. This section briefly
//! describes the RAL, IOMUX, and CCM APIs, which are used throughout the crate's interface. In summary,
//!
//! - Acquire your peripheral instances through `ral`
//! - Acquire your peripheral pads through the [`iomuxc`](iomuxc/index.html)
//! - Enable your clocks and clock gates through [`ccm`](ccm/index.html)
//!
//! The RAL is described below. See the documentation of the other modules for more details.
//!
//! ## RAL
//!
//! Peripheral selection depends on the [`imxrt-ral`] crate. The RAL is re-exported in the `ral` module.
//! The API provide the lowest-level access for configuring peripherals.
//!
//! [`imxrt-ral`]: https://docs.rs/imxrt-ral/latest/imxrt_ral/
//!
//! All peripherals in this crate require a corresponding RAL instance. Those instances may be
//! wrapped in a strongly-typed [`instance`](instance/index.html) to identify the instance ID at compile
//! time. Unless you're performing more advanced peripheral configuration, or not using one of these async APIs,
//! you should simply use `take()` to acquire the peripheral, then pass it into a `imxrt-async-hal` API.
//!
//! ```no_run
//! use imxrt_async_hal as hal;
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
//! # perclock.clock_gate_gpt(&mut gpt2, ccm::ClockActivity::On);
//! let mut gpt = GPT::new(gpt2, &perclock);
//! ```
//!
//! # Example
//!
//! Simultaneously blink an LED while echoing all UART data back to
//! the sender.
//!
//! Note that this example comments out some code that would be necessary for a real embedded
//! system. See the accompanying comments for more information.
//!
//! ```no_run
//! // #![no_std]  // Required for a real embedded system
//! // #![no_main] // Required for a real embedded system
//!
//! use imxrt_async_hal as hal;
//! use futures::future;
//! const BAUD: u32 = 115_200;
//! # mod executor { pub fn block_on<F: core::future::Future>(f: F) {} }
//!
//! /* #[cortex_m_rt::entry], or your entry decorator */
//! fn main() /* -> ! */ { // Never return may be required by your runtime's entry decorator
//!     let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
//!     let mut led = hal::gpio::GPIO::new(pads.b0.p03).output();
//!     let mut gpt = hal::ral::gpt::GPT2::take().unwrap();
//!
//!     let hal::ccm::CCM {
//!         mut handle,
//!         perclock,
//!         uart_clock,
//!         ..
//!     } = hal::ral::ccm::CCM::take().map(hal::ccm::CCM::new).unwrap();
//!     let mut perclock = perclock.enable(&mut handle);
//!     perclock.clock_gate_gpt(&mut gpt, hal::ccm::ClockActivity::On);
//!
//!     let mut timer = hal::GPT::new(gpt, &perclock);
//!     let mut channels = hal::dma::channels(
//!         hal::ral::dma0::DMA0::take()
//!             .map(|mut dma| {
//!                 handle.clock_gate_dma(&mut dma, hal::ccm::ClockActivity::On);
//!                 dma
//!             })
//!             .unwrap(),
//!         hal::ral::dmamux::DMAMUX::take().unwrap(),
//!     );
//!
//!     let mut uart_clock = uart_clock.enable(&mut handle);
//!     let uart2 = hal::ral::lpuart::LPUART2::take()
//!         .map(|mut inst| {
//!             uart_clock.clock_gate(&mut inst, hal::ccm::ClockActivity::On);
//!             inst
//!         })
//!         .and_then(hal::instance::uart)
//!         .unwrap();
//!     let mut uart = hal::UART::new(
//!         uart2,
//!         pads.ad_b1.p02, // TX
//!         pads.ad_b1.p03, // RX
//!         channels[7].take().unwrap(),
//!         &uart_clock,
//!     );
//!     uart.set_baud(BAUD).unwrap();
//!
//!     let blinking_loop = async {
//!         loop {
//!             timer.delay_us(250_000).await;
//!             led.toggle();
//!         }
//!     };
//!
//!     let echo_loop = async {
//!         loop {
//!             let mut buffer = [0; 1];
//!             uart.read(&mut buffer).await.unwrap();
//!             uart.write(&buffer).await.unwrap();
//!         }
//!     };
//!
//!     executor::block_on(future::join(blinking_loop, echo_loop));
//!     unreachable!();
//! }
//! ```
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

/// Decorates one or more functions that will be statically registered
/// in the interrupt table
///
/// `interrupts!` may only be used once per module. It should only include
/// function definitions. The function names should reflect the IRQ name as
/// provided by the RAL's `interrupt` macro.
#[cfg(any(dma, feature = "gpio", feature = "i2c"))]
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
#[cfg(dma)]
#[cfg_attr(docsrs, doc(cfg(dma)))]
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

#[cfg(target_arch = "arm")]
use cortex_m_rt as rt;

//
// Module re-exports
//
#[cfg(feature = "gpt")]
pub use gpt::GeneralPurposeTimer as GPT;
#[cfg(feature = "i2c")]
pub use i2c::{ClockSpeed as I2CClockSpeed, Error as I2CError, I2C};
#[cfg(feature = "pit")]
pub use pit::PeriodicTimer as PIT;
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
    #[cfg_attr(docsrs, doc(cfg(feature = "imxrt106x")))]
    #[cfg(feature = "imxrt106x")]
    pub use imxrt_iomuxc::imxrt106x::*;
    pub use imxrt_iomuxc::prelude::*;

    /// Turn the `IOMUXC` instance into pads
    ///
    /// ```no_run
    /// use imxrt_async_hal as hal;
    /// use hal::{ral::iomuxc::IOMUXC, iomuxc};
    ///
    /// let pads = iomuxc::new(IOMUXC::take().unwrap());
    /// ```
    #[cfg_attr(docsrs, doc(cfg(any(feature = "imxrt106x"))))]
    #[cfg(any(feature = "imxrt106x"))]
    pub fn new(_: crate::ral::iomuxc::Instance) -> Pads {
        // Safety: ^--- there's a single instance. Either the user
        // used an `unsafe` method to steal it, or we own the only
        // instance.
        unsafe { Pads::new() }
    }
}
