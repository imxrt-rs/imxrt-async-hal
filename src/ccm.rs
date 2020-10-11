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
