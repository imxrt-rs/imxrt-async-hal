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

/// Periodic clock (PIT, GPT) types
pub mod perclock {
    pub use imxrt_ccm::perclock::Selection;
}
