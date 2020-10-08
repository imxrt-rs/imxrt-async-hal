//! GPIOs
//!
//! [`GPIO`s](struct.GPIO.html) can be in either input or output states. GPIO inputs can
//! read the high / low status of physical pins. Based on a [`Sensitivity`](enum.Sensitivity.html)
//! selection, GPIO inputs can wait for transitions on the input pin.
//!
//! ```no_run
//! use imxrt_async_hal as hal;
//! use hal::gpio::{GPIO, Sensitivity};
//!
//! # async {
//! let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
//! let mut input = GPIO::new(pads.b0.p03);
//! input.wait_for(Sensitivity::FallingEdge).await;
//! // Transitioned from high to low!
//! assert!(!input.is_set());
//! # };
//! ```
//!
//! `GPIO`s outputs can drive the associated pin high or low, and they can toggle the pin.
//!
//! ```no_run
//! use imxrt_async_hal as hal;
//!
//! let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
//! let input = hal::gpio::GPIO::new(pads.b0.p03);
//! let mut output = input.output();
//!
//! output.set();
//! assert!(output.is_set());
//!
//! output.toggle();
//! assert!(!output.is_set());
//! ```
//!
//! # Example
//!
//! In this example, we toggle the Teensy 4's LED for every falling edge on pin 14:
//!
//! ```no_run
//! use imxrt_async_hal as hal;
//! use hal::gpio::{GPIO, Sensitivity};
//!
//! # fn block_on<F: core::future::Future<Output = ()>>(f: F) {};
//! # let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
//! let mut led = GPIO::new(pads.b0.p03).output();
//! let mut input_pin = GPIO::new(pads.b0.p02);
//!
//! let blinking_loop = async {
//!     loop {
//!         input_pin.wait_for(Sensitivity::FallingEdge).await;
//!         led.toggle();
//!     }
//! };
//! block_on(blinking_loop);
//! ```

#[cfg(target_arch = "arm")]
use crate::ral::interrupt;

use crate::iomuxc::{consts::Unsigned, gpio::Pin};
use crate::ral::{
    self,
    gpio::{self, RegisterBlock},
};
use core::{
    future::Future,
    marker::PhantomData,
    pin,
    sync::atomic,
    task::{Context, Poll, Waker},
};

/// Indicates that a pin is configured as an input
pub enum Input {}
/// Indicates that a pin is configured as an output
pub enum Output {}

/// A wrapper around an i.MX RT processor pad, supporting simple I/O
///
/// Use [`new`](#method.new) with a pad from the [`iomuxc`](../iomuxc/index.html)
/// module, or a Teensy 4 [`pin`](../pins/index.html). All GPIOs start in the input state. Use
/// [`output`](#method.output) to become an output pin.
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::gpio::GPIO;
///
/// let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
/// let mut led = GPIO::new(pads.b0.p03);
/// assert!(!led.is_set());
/// let mut led = led.output();
/// led.set();
/// ```
///
/// When using a `GPIO` as an input, you can wait for transitions using [`wait_for`](#method.wait_for).
pub struct GPIO<P, D> {
    pin: P,
    dir: PhantomData<D>,
}

impl<P, D> GPIO<P, D>
where
    P: Pin,
{
    fn register_block(&self) -> *const RegisterBlock {
        const REGISTER_BLOCKS: [*const RegisterBlock; 9] = [
            gpio::GPIO1,
            gpio::GPIO2,
            gpio::GPIO3,
            gpio::GPIO4,
            gpio::GPIO5,
            gpio::GPIO6,
            gpio::GPIO7,
            gpio::GPIO8,
            gpio::GPIO9,
        ];
        REGISTER_BLOCKS[self.module().saturating_sub(1)]
    }

    #[inline(always)]
    fn offset(&self) -> u32 {
        1u32 << <P as Pin>::Offset::USIZE
    }

    /// The return is a non-zero number, since the GPIO identifiers
    /// start with '1.'
    #[inline(always)]
    fn module(&self) -> usize {
        <P as Pin>::Module::USIZE
    }

    /// Returns the ICR field offset for this pin
    fn icr_offset(&self) -> usize {
        (<P as Pin>::Offset::USIZE % 16) * 2
    }
}

impl<P> GPIO<P, Input>
where
    P: Pin,
{
    /// Create a GPIO from a pad that supports a GPIO configuration
    ///
    /// All pads may be used as a GPIO, so `new` should work with every `iomuxc` pad.
    ///
    /// ```no_run
    /// use imxrt_async_hal as hal;
    /// use hal::gpio::GPIO;
    ///
    /// let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
    /// let input_pin = GPIO::new(pads.b0.p03);
    /// ```
    pub fn new(mut pin: P) -> Self {
        crate::iomuxc::gpio::prepare(&mut pin);

        static ONCE: crate::once::Once = crate::once::new();
        ONCE.call(|| unsafe {
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::GPIO1_Combined_0_15);
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::GPIO1_Combined_16_31);
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::GPIO2_Combined_0_15);
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::GPIO2_Combined_16_31);
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::GPIO3_Combined_0_15);
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::GPIO3_Combined_16_31);
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::GPIO4_Combined_0_15);
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::GPIO4_Combined_16_31);
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::GPIO5_Combined_0_15);
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::GPIO5_Combined_16_31);
        });
        Self {
            pin,
            dir: PhantomData,
        }
    }

    /// Transition the GPIO from an input to an output
    pub fn output(self) -> GPIO<P, Output> {
        // Safety: critical section ensures consistency
        cortex_m::interrupt::free(|_| unsafe {
            ral::modify_reg!(ral::gpio, self.register_block(), GDIR, |gdir| gdir
                | self.offset());
        });
        GPIO {
            pin: self.pin,
            dir: PhantomData,
        }
    }

    /// Returns `true` if this input pin is high
    pub fn is_set(&self) -> bool {
        // Safety: read is atomic
        unsafe { ral::read_reg!(ral::gpio, self.register_block(), PSR) & self.offset() != 0 }
    }

    fn set_sensitivity(&mut self, sensitivity: Sensitivity) {
        if Sensitivity::EitherEdge == sensitivity {
            unsafe {
                ral::modify_reg!(ral::gpio, self.register_block(), EDGE_SEL, |edge_sel| {
                    edge_sel | self.offset()
                });
            }
        } else {
            unsafe {
                ral::modify_reg!(ral::gpio, self.register_block(), EDGE_SEL, |edge_sel| {
                    edge_sel & !self.offset()
                });
            }
            let icr = match sensitivity {
                Sensitivity::Low => 0,
                Sensitivity::High => 1,
                Sensitivity::RisingEdge => 2,
                Sensitivity::FallingEdge => 3,
                _ => unreachable!("Sensitivity::EitherEdge handled above"),
            };
            let icr_offset = self.icr_offset();
            let icr_modify = |reg| reg & !(0b11 << icr_offset) | (icr << icr_offset);
            if <P as Pin>::Offset::USIZE < 16 {
                unsafe {
                    ral::modify_reg!(ral::gpio, self.register_block(), ICR1, icr_modify);
                }
            } else {
                unsafe {
                    ral::modify_reg!(ral::gpio, self.register_block(), ICR2, icr_modify);
                }
            }
        }
    }

    /// Sets the sensitivity for the input GPIO, and await for the input event.
    ///
    /// ```no_run
    /// use imxrt_async_hal as hal;
    /// use hal::gpio::{GPIO, Sensitivity};
    ///
    /// let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
    /// let mut input_pin = GPIO::new(pads.ad_b1.p02);
    /// // ...
    /// # async {
    /// input_pin.wait_for(Sensitivity::RisingEdge).await;
    /// # };
    /// ```
    pub async fn wait_for(&mut self, sensitivity: Sensitivity) {
        InputSensitive::new(self, sensitivity).await
    }
}

impl<P> GPIO<P, Output>
where
    P: Pin,
{
    /// Transition the pin from an output to an input
    pub fn input(self) -> GPIO<P, Input> {
        // Safety: critical section ensures consistency
        cortex_m::interrupt::free(|_| unsafe {
            ral::modify_reg!(ral::gpio, self.register_block(), GDIR, |gdir| gdir
                & !self.offset());
        });
        GPIO {
            pin: self.pin,
            dir: PhantomData,
        }
    }

    /// Drive the GPIO high
    pub fn set(&mut self) {
        // Safety: atomic write
        unsafe { ral::write_reg!(ral::gpio, self.register_block(), DR_SET, self.offset()) };
    }

    /// Drive the GPIO low
    pub fn clear(&mut self) {
        // Safety: atomic write
        unsafe { ral::write_reg!(ral::gpio, self.register_block(), DR_CLEAR, self.offset()) };
    }

    /// Returns `true` if the pin is driving high
    pub fn is_set(&self) -> bool {
        // Safety: atomic read
        unsafe { ral::read_reg!(ral::gpio, self.register_block(), DR) & self.offset() != 0u32 }
    }

    /// Alternate the state of the pin
    ///
    /// Using `toggle` will be more efficient than checking [`is_set`](#method.is_set)
    /// and then selecting the opposite state.
    pub fn toggle(&mut self) {
        // Safety: atomic write
        unsafe { ral::write_reg!(ral::gpio, self.register_block(), DR_TOGGLE, self.offset()) }
    }
}

/// Interrupt sensitivity selection
///
/// See [`GPIO::wait_for`](#method.wait_for) for more information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sensitivity {
    /// Interrupt when GPIO is low
    Low,
    /// Interrupt when GPIO is high
    High,
    /// Interrupt after GPIO rising edge
    RisingEdge,
    /// Interrupt after GPIO falling edge
    FallingEdge,
    /// Interrupt after either a rising or falling edge
    EitherEdge,
}

/// A future that awaits the input sensitivity selection
struct InputSensitive<'t, P> {
    gpio: &'t mut GPIO<P, Input>,
    waker: Option<Waker>,
    is_ready: bool,
    sensitivity: Sensitivity,
}

impl<'t, P> InputSensitive<'t, P> {
    fn new(gpio: &'t mut GPIO<P, Input>, sensitivity: Sensitivity) -> Self {
        InputSensitive {
            gpio,
            waker: None,
            is_ready: true,
            sensitivity,
        }
    }
}

impl<'t, P> Future for InputSensitive<'t, P>
where
    P: Pin,
{
    type Output = ();
    fn poll(self: pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.is_ready {
            this.is_ready = false;
            this.gpio.set_sensitivity(this.sensitivity);
            this.waker = Some(cx.waker().clone());
            unsafe {
                WAKERS[this.gpio.module().saturating_sub(1)][<P as Pin>::Offset::USIZE] =
                    &mut this.waker;
            }
            atomic::compiler_fence(atomic::Ordering::Release);
            cortex_m::interrupt::free(|_| unsafe {
                ral::modify_reg!(ral::gpio, this.gpio.register_block(), IMR, |imr| imr
                    | this.gpio.offset())
            });
            Poll::Pending
        } else if this.waker.is_none() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

/// Points to memory owned by the InputSensitive future
static mut WAKERS: [[*mut Option<Waker>; 32]; 5] = [[core::ptr::null_mut(); 32]; 5];

#[inline(always)]
unsafe fn on_interrupt(gpio: *const ral::gpio::RegisterBlock, mut module: usize) {
    module -= 1;
    let isr = ral::read_reg!(ral::gpio, gpio, ISR);
    ral::write_reg!(ral::gpio, gpio, ISR, isr);
    ral::modify_reg!(ral::gpio, gpio, IMR, |imr| imr & !isr);
    (0..32usize)
        .filter(|bit| (isr & (1 << bit) != 0) && !WAKERS[module][*bit].is_null())
        .filter_map(|bit| (*WAKERS[module][bit]).take())
        .for_each(|waker| waker.wake());
}

#[cfg_attr(target_arch = "arm", crate::rt::interrupt)]
#[cfg_attr(not(target_arch = "arm"), allow(unused, non_snake_case))]
unsafe fn GPIO1_Combined_0_15() {
    on_interrupt(ral::gpio::GPIO1, 1);
}

#[cfg_attr(target_arch = "arm", crate::rt::interrupt)]
#[cfg_attr(not(target_arch = "arm"), allow(unused, non_snake_case))]
unsafe fn GPIO1_Combined_16_31() {
    on_interrupt(ral::gpio::GPIO1, 1);
}

#[cfg_attr(target_arch = "arm", crate::rt::interrupt)]
#[cfg_attr(not(target_arch = "arm"), allow(unused, non_snake_case))]
unsafe fn GPIO2_Combined_0_15() {
    on_interrupt(ral::gpio::GPIO2, 2);
}

#[cfg_attr(target_arch = "arm", crate::rt::interrupt)]
#[cfg_attr(not(target_arch = "arm"), allow(unused, non_snake_case))]
unsafe fn GPIO2_Combined_16_31() {
    on_interrupt(ral::gpio::GPIO2, 2);
}

#[cfg_attr(target_arch = "arm", crate::rt::interrupt)]
#[cfg_attr(not(target_arch = "arm"), allow(unused, non_snake_case))]
unsafe fn GPIO3_Combined_0_15() {
    on_interrupt(ral::gpio::GPIO3, 3);
}

#[cfg_attr(target_arch = "arm", crate::rt::interrupt)]
#[cfg_attr(not(target_arch = "arm"), allow(unused, non_snake_case))]
unsafe fn GPIO3_Combined_16_31() {
    on_interrupt(ral::gpio::GPIO3, 3);
}

#[cfg_attr(target_arch = "arm", crate::rt::interrupt)]
#[cfg_attr(not(target_arch = "arm"), allow(unused, non_snake_case))]
unsafe fn GPIO4_Combined_0_15() {
    on_interrupt(ral::gpio::GPIO4, 4);
}

#[cfg_attr(target_arch = "arm", crate::rt::interrupt)]
#[cfg_attr(not(target_arch = "arm"), allow(unused, non_snake_case))]
unsafe fn GPIO4_Combined_16_31() {
    on_interrupt(ral::gpio::GPIO4, 4);
}

#[cfg_attr(target_arch = "arm", crate::rt::interrupt)]
#[cfg_attr(not(target_arch = "arm"), allow(unused, non_snake_case))]
unsafe fn GPIO5_Combined_0_15() {
    on_interrupt(ral::gpio::GPIO5, 5);
}

#[cfg_attr(target_arch = "arm", crate::rt::interrupt)]
#[cfg_attr(not(target_arch = "arm"), allow(unused, non_snake_case))]
unsafe fn GPIO5_Combined_16_31() {
    on_interrupt(ral::gpio::GPIO5, 5);
}
