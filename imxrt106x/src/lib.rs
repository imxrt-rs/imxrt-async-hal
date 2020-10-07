#![no_std]

pub use imxrt_async_hal::*;

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
    pub use imxrt_iomuxc::imxrt106x::*;
    pub use imxrt_iomuxc::prelude::*;

    /// Turn the `IOMUXC` instance into pads
    ///
    /// See the [module-level docs](index.html) for an example.
    pub fn new(_: crate::ral::iomuxc::Instance) -> Pads {
        // Safety: ^--- there's a single instance. Either the user
        // used an `unsafe` method to steal it, or we own the only
        // instance.
        unsafe { Pads::new() }
    }
}