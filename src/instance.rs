//! Strongly-typed peripherals
//!
//! Peripheral instances from the RAL do not carry their peripheral identification in the
//! type system. For instance, an `LPUART2` peripheral and an `LPUART3` peripheral
//! are represented by the same Rust type, `ral::lpuart::Instance`.
//!
//! However, the [`iomuxc` APIs](../iomuxc/index.html) work with strongly-typed peripheral
//! instances, which are identified by a type-level constant. This interface expects `LPUART2`
//! and `LPUART3` to be unique types. To bridge these APIs,
//! and ensure that your peripheral instances work with your pin selections,
//! use the `instance` interface.
//!
//! An [`Instance`](struct.Instance.html) wraps a RAL peripheral instance, requiring that the
//! peripheral instance matches its type-level constant. The interface ensures that your
//! RAL instance matches the type-level constant:
//!
//! ```no_run
//! use imxrt_async_hal as hal;
//! use hal::{instance, iomuxc::consts};
//! use hal::ral::lpuart;
//!
//! // This number... ----------------v
//! let inst: instance::UART<consts::U2> =
//!     lpuart::LPUART2::take().and_then(instance::uart).unwrap();
//! //                ^----- ...matches here, so the unwrap is safe!
//! ```
//!
//! A mismatch between the expected instance and the RAL instance will return `None`, or
//! panic whe `unwrap`ped:
//!
//! ```should_panic, note that this actually panics because of an invalid memory access
//! # use imxrt_async_hal as hal;
//! # use hal::{instance, iomuxc::consts};
//! # use hal::ral::lpuart;
//! // This number... ----------------v
//! let inst: instance::UART<consts::U2> =
//!     lpuart::LPUART3::take().and_then(instance::uart).unwrap();
//! //                ^----- ...doesn't match! panic!
//! ```
//!
//! Typically, you may elide the types, since the peripheral APIs will match the expected types.
//! See the documentation of your peripheral for examples.

use core::marker::PhantomData;

use crate::{iomuxc::consts, ral};

/// A trait implemented on RAL instances
///
/// [`inst`](#method.inst) returns the peripheral instance as a run-time value.
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::ral::lpspi::LPSPI3;
/// use hal::instance::Inst;
///
/// let lpspi3 = LPSPI3::take().unwrap();
/// assert_eq!(lpspi3.inst(), 3);
/// ```
pub trait Inst: private::Sealed {
    /// Return the peripheral instance as a run-time value
    ///
    /// The exact number is specific to the peripheral, and the peripheral type.
    /// For instance, a `LPUART7` instance would return 7.
    fn inst(&self) -> usize;
}

mod private {
    pub trait Sealed {}
}

/// A strongly-typed RAL instance
///
/// `Instance` wraps a RAL peripheral instance with a type-level constant, so that it can
/// be used in APIs that require such information.
pub struct Instance<I, M> {
    inst: I,
    _m: PhantomData<M>,
}

impl<I, M> Instance<I, M> {
    /// Returns the wrapped RAL instance
    pub fn release(self) -> I {
        self.inst
    }
}

impl<I, M> core::ops::Deref for Instance<I, M> {
    type Target = I;
    fn deref(&self) -> &Self::Target {
        &self.inst
    }
}

impl<I, M> core::ops::DerefMut for Instance<I, M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inst
    }
}

fn instance<I, M>(inst: I) -> Option<Instance<I, M>>
where
    I: Inst,
    M: consts::Unsigned,
{
    if M::USIZE == inst.inst() {
        Some(Instance {
            inst,
            _m: PhantomData,
        })
    } else {
        None
    }
}

#[cfg(feature = "uart")]
impl Inst for ral::lpuart::Instance {
    fn inst(&self) -> usize {
        match &**self as *const _ {
            ral::lpuart::LPUART1 => 1,
            ral::lpuart::LPUART2 => 2,
            ral::lpuart::LPUART3 => 3,
            ral::lpuart::LPUART4 => 4,
            #[cfg(feature = "imxrt106x")]
            ral::lpuart::LPUART5 => 5,
            #[cfg(feature = "imxrt106x")]
            ral::lpuart::LPUART6 => 6,
            #[cfg(feature = "imxrt106x")]
            ral::lpuart::LPUART7 => 7,
            #[cfg(feature = "imxrt106x")]
            ral::lpuart::LPUART8 => 8,
            _ => unreachable!(),
        }
    }
}

#[cfg(feature = "uart")]
impl private::Sealed for ral::lpuart::Instance {}

/// Alias for an `Instance` around a `ral::lpuart::Instance`
///
/// See [`uart`](fn.uart.html) to acquire a `UART` instance.
#[cfg(feature = "uart")]
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
pub type UART<M> = Instance<ral::lpuart::Instance, M>;

/// Specify a `UART` instance
///
/// Returns `Some(...)` if `M` matches the `lpuart::Instance` identifier.
/// Otherwise, returns `None`.
#[cfg(feature = "uart")]
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
pub fn uart<M>(uart: ral::lpuart::Instance) -> Option<UART<M>>
where
    M: consts::Unsigned,
{
    instance(uart)
}

#[cfg(feature = "spi")]
impl Inst for ral::lpspi::Instance {
    fn inst(&self) -> usize {
        match &**self as *const _ {
            ral::lpspi::LPSPI1 => 1,
            ral::lpspi::LPSPI2 => 2,
            #[cfg(feature = "imxrt106x")]
            ral::lpspi::LPSPI3 => 3,
            #[cfg(feature = "imxrt106x")]
            ral::lpspi::LPSPI4 => 4,
            _ => unreachable!(),
        }
    }
}

#[cfg(feature = "spi")]
impl private::Sealed for ral::lpspi::Instance {}

/// Alias for an `Instance` around a `ral::lpspi::Instance`
///
/// See [`spi`](fn.spi.html) to acquire a `SPI` instance.
#[cfg(feature = "spi")]
#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
pub type SPI<M> = Instance<ral::lpspi::Instance, M>;

/// Specify a `SPI` instance
///
/// Returns `Some(...)` if `M` matches the `lpspi::Instance` identifier.
/// Otherwise, returns `None`.
#[cfg(feature = "spi")]
#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
pub fn spi<M>(spi: ral::lpspi::Instance) -> Option<SPI<M>>
where
    M: consts::Unsigned,
{
    instance(spi)
}

#[cfg(feature = "i2c")]
impl Inst for ral::lpi2c::Instance {
    fn inst(&self) -> usize {
        match &**self as *const _ {
            ral::lpi2c::LPI2C1 => 1,
            ral::lpi2c::LPI2C2 => 2,
            #[cfg(feature = "imxrt106x")]
            ral::lpi2c::LPI2C3 => 3,
            #[cfg(feature = "imxrt106x")]
            ral::lpi2c::LPI2C4 => 4,
            _ => unreachable!(),
        }
    }
}

#[cfg(feature = "i2c")]
impl private::Sealed for ral::lpi2c::Instance {}

/// Alias for an `Instance` around a `ral::lpi2c::Instance`
///
/// See [`i2c`](fn.i2c.html) to acquire an `I2C` instance.
#[cfg(feature = "i2c")]
#[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
pub type I2C<M> = Instance<ral::lpi2c::Instance, M>;

/// Specify an `I2C` instance
///
/// Returns `Some(...)` if `M` matches the `lpi2c::Instance` identifier.
/// Otherwise, returns `None`.
#[cfg(feature = "i2c")]
#[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
pub fn i2c<M>(i2c: ral::lpi2c::Instance) -> Option<I2C<M>>
where
    M: consts::Unsigned,
{
    instance(i2c)
}
