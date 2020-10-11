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
mod perclock;

use crate::ral;

/// Handle to the CCM register block
///
/// `Handle` also supports clock gating for peripherals that
/// don't have an obvious clock root, like DMA.
pub struct Handle(pub(crate) ral::ccm::Instance);

impl Handle {
    pub fn clock_gate_dma(
        &mut self,
        dma: &mut crate::ral::dma0::Instance,
        activity: ClockActivity,
    ) {
        dma.clock_gate(&mut Enabled(()), activity);
    }
}

/// The CCM components
#[non_exhaustive]
pub struct CCM {
    /// The handle to the CCM register block
    ///
    /// `Handle` is used throughout the HAL
    pub handle: Handle,
    /// The periodic clock handle
    pub perclock: PerClock,
}

impl CCM {
    /// Construct a new CCM from the RAL's CCM instance
    pub const fn new(ccm: ral::ccm::Instance) -> Self {
        CCM {
            handle: Handle(ccm),
            perclock: PerClock(()),
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

/// Describes a type that can have its clock gated by the CCM
trait ClockGate<Clock = ()> {
    /// Gate the clock based, setting the value to the clock activity
    fn clock_gate(&mut self, handle: &mut Enabled<Clock>, activity: ClockActivity);
}

/// Crystal oscillator frequency
// TODO should be private
pub(crate) const OSCILLATOR_FREQUENCY_HZ: u32 = 24_000_000;

/// An enabled clock of type `Clock`
pub struct Enabled<Clock>(Clock);

/// The periodic clock root
///
/// `PerClock` is the input clock for GPT and PIT. It runs at
/// 1MHz.
pub struct PerClock(());
