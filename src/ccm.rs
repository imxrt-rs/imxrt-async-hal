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
pub(crate) enum ClockActivity {
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
pub(crate) trait ClockGate {
    /// Gate the clock based, setting the value to the clock activity
    fn gate(&mut self, handle: &mut Handle, activity: ClockActivity);
}
