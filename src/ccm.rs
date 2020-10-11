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

mod gates;

use crate::ral;

/// Handle to the CCM register block
pub struct Handle(pub(crate) ral::ccm::Instance);

/// The CCM components
#[non_exhaustive]
pub struct CCM {
    /// The handle to the CCM register block
    ///
    /// `Handle` is used throughout the HAL
    pub handle: Handle,
}

impl CCM {
    /// Construct a new CCM from the RAL's CCM instance
    pub const fn new(ccm: ral::ccm::Instance) -> Self {
        CCM {
            handle: Handle(ccm),
        }
    }
}

/// Describes a clock gate setting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[non_exhaustive] // There's one more variant that's not yet specified
pub(crate) enum ClockActivity {
    /// Clock is off during all modes
    ///
    /// Stop enter hardware handshake is disabled.
    Off = 0b00,
    /// Clock is on in all modes, except stop mode
    On = 0b11,
}

/// Describes a type that can have its clock gated by the CCM
pub(crate) trait ClockGate {
    /// Gate the clock based, setting the value to the clock activity
    fn gate(&mut self, handle: &mut Handle, activity: ClockActivity);
}

pub(crate) const OSCILLATOR_FREQUENCY_HZ: u32 = 24_000_000;
pub(crate) const PERIODIC_CLOCK_FREQUENCY_HZ: u32 =
    OSCILLATOR_FREQUENCY_HZ / PERIODIC_CLOCK_DIVIDER;
pub(crate) const PERIODIC_CLOCK_DIVIDER: u32 = 24;

/// Enable the periodic clock root
pub(crate) fn enable_periodic_clock<Gate: ClockGate>(
    ccm: &mut crate::ccm::Handle,
    gate: &mut Gate,
) {
    static ONCE: crate::once::Once = crate::once::new();
    ONCE.call(|| {
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CSCMR1,
            PERCLK_CLK_SEL: PERCLK_CLK_SEL_1,
            PERCLK_PODF: PERIODIC_CLOCK_DIVIDER - 1
        );
    });
    gate.gate(ccm, ClockActivity::On);
}
