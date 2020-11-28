//! General purpose timers (GPT)
//!
//! The timer **divides the input clock by 5**. This may affect very precise
//! timing. For a more precise timer, see [`PIT`](crate::pit::PIT).
//!
//! Each GPT instance turns into three GPT timers. Use [`new`](crate::gpt::GPT::new)
//! to acquire the three timers.
//!
//! # Example
//!
//! Use GPT1 to block for 250ms.
//!
//! ```no_run
//! use imxrt_async_hal as hal;
//! use hal::ral::{ccm, gpt};
//! use hal::{ccm::{CCM, ClockGate}, GPT};
//!
//! let mut ccm = ccm::CCM::take().map(CCM::from_ral).unwrap();
//! let mut perclock = ccm.perclock.enable(&mut ccm.handle);
//! let (mut gpt, _, _) = gpt::GPT1::take().map(|mut gpt| {
//!     perclock.set_clock_gate_gpt(&mut gpt, ClockGate::On);
//!     GPT::new(gpt, &perclock)
//! }).unwrap();
//!
//! # async {
//! gpt.delay_us(250_000u32).await;
//! gpt.delay(core::time::Duration::from_millis(250)).await; // Equivalent
//! # };
//! ```

use crate::ral;
use core::{
    future::Future,
    marker::PhantomPinned,
    pin::Pin,
    sync::atomic,
    task::{Context, Poll, Waker},
    time::Duration,
};

/// The GPT timer
///
/// See the [module-level documentation](mod@crate::gpt) for more information.
#[cfg_attr(docsrs, doc(cfg(feature = "gpt")))]
pub struct GPT {
    gpt: ral::gpt::Instance,
    hz: u32,
    output_compare: OutputCompare,
}

/// GPT clock divider
///
/// This crystal oscillator is very sensitive. Not all values
/// seem to work. 5 is one of them that does. So is 3. 10 does
/// not work. The field is supposed to support values up to 0xF.
///
/// The seL4 project also notes issues with this divider value.
/// Can't find anything in the errata...
const DIVIDER: u32 = 5;

fn steal(gpt: &ral::gpt::Instance) -> ral::gpt::Instance {
    // Safety: we already have a GPT instance, so users won't notice
    // that we're stealing the instance again...
    unsafe {
        match &**gpt as *const _ {
            ral::gpt::GPT1 => ral::gpt::GPT1::steal(),
            ral::gpt::GPT2 => ral::gpt::GPT2::steal(),
            _ => unreachable!("There are only two GPTs"),
        }
    }
}

impl GPT {
    /// Create a new `GPT` from a RAL GPT instance
    pub fn new(gpt: ral::gpt::Instance, clock: &crate::ccm::PerClock) -> (Self, Self, Self) {
        let irq = match &*gpt as *const _ {
            ral::gpt::GPT1 => ral::interrupt::GPT1,
            ral::gpt::GPT2 => ral::interrupt::GPT2,
            _ => unreachable!("There are only two GPTs"),
        };

        ral::write_reg!(
            ral::gpt,
            gpt,
            CR,
            EN_24M: 1, // Enable crystal oscillator
            CLKSRC: 0b101 // Crystal Oscillator
        );
        ral::write_reg!(ral::gpt, gpt, PR, PRESCALER24M: DIVIDER - 1);

        // Clear all statuses
        ral::write_reg!(ral::gpt, gpt, SR, 0b11_1111);
        ral::modify_reg!(
            ral::gpt, gpt, IR,
            ROVIE: 0 // Rollover interrupt disabled
        );
        ral::modify_reg!(
            ral::gpt, gpt, CR,
            FRR: 1, // Free-running mode, no matter the output compare channel
            WAITEN: 1, // Run in wait mode
            ENMOD: 0, // Counter maintains value when disabled
            EN: 1 // Start the timer
        );

        unsafe { cortex_m::peripheral::NVIC::unmask(irq) };
        let hz = clock.frequency() / DIVIDER;
        (
            GPT {
                gpt: steal(&gpt),
                hz,
                output_compare: OutputCompare::Channel1,
            },
            GPT {
                gpt: steal(&gpt),
                hz,
                output_compare: OutputCompare::Channel2,
            },
            GPT {
                gpt,
                hz,
                output_compare: OutputCompare::Channel3,
            },
        )
    }

    /// Wait for the specified `duration` to elapse
    ///
    /// If the microseconds represented by the duration cannot fit in a `u32`, the
    /// delay will saturate at `u32::max_value()` microseconds.
    pub fn delay(&mut self, duration: Duration) -> Delay<'_> {
        use core::convert::TryFrom;
        self.delay_us(u32::try_from(duration.as_micros()).unwrap_or(u32::max_value()))
    }
    /// Wait for `microseconds` to elapse
    pub fn delay_us(&mut self, microseconds: u32) -> Delay<'_> {
        Delay {
            gpt: &self.gpt,
            delay_ns: microseconds.saturating_mul(1_000),
            hz: self.hz,
            output_compare: self.output_compare,
            _pin: PhantomPinned,
        }
    }

    /// Returns the `GPT` clock period
    pub fn clock_period(&self) -> Duration {
        Duration::from_micros((1_000_000 / self.hz) as u64)
    }
}

/// Clear the output compare flag
#[inline(always)]
fn clear_trigger(gpt: &ral::gpt::Instance, output_compare: OutputCompare) {
    match output_compare {
        OutputCompare::Channel1 => ral::modify_reg!(ral::gpt, gpt, SR, OF1: 1),
        OutputCompare::Channel2 => ral::modify_reg!(ral::gpt, gpt, SR, OF2: 1),
        OutputCompare::Channel3 => ral::modify_reg!(ral::gpt, gpt, SR, OF3: 1),
    }
}
#[inline(always)]
fn is_triggered(gpt: &ral::gpt::Instance, output_compare: OutputCompare) -> bool {
    match output_compare {
        OutputCompare::Channel1 => ral::read_reg!(ral::gpt, gpt, SR, OF1 == 1),
        OutputCompare::Channel2 => ral::read_reg!(ral::gpt, gpt, SR, OF2 == 1),
        OutputCompare::Channel3 => ral::read_reg!(ral::gpt, gpt, SR, OF3 == 1),
    }
}
#[inline(always)]
fn enable_interrupt(gpt: &ral::gpt::Instance, output_compare: OutputCompare) {
    match output_compare {
        OutputCompare::Channel1 => ral::modify_reg!(ral::gpt, gpt, IR, OF1IE: 1),
        OutputCompare::Channel2 => ral::modify_reg!(ral::gpt, gpt, IR, OF2IE: 1),
        OutputCompare::Channel3 => ral::modify_reg!(ral::gpt, gpt, IR, OF3IE: 1),
    }
}
#[inline(always)]
fn disable_interrupt(gpt: &ral::gpt::Instance, output_compare: OutputCompare) {
    match output_compare {
        OutputCompare::Channel1 => ral::modify_reg!(ral::gpt, gpt, IR, OF1IE: 0),
        OutputCompare::Channel2 => ral::modify_reg!(ral::gpt, gpt, IR, OF2IE: 0),
        OutputCompare::Channel3 => ral::modify_reg!(ral::gpt, gpt, IR, OF3IE: 0),
    }
}
#[inline(always)]
fn interrupt_enabled(gpt: &ral::gpt::Instance, output_compare: OutputCompare) -> bool {
    match output_compare {
        OutputCompare::Channel1 => ral::read_reg!(ral::gpt, gpt, IR, OF1IE == 1),
        OutputCompare::Channel2 => ral::read_reg!(ral::gpt, gpt, IR, OF2IE == 1),
        OutputCompare::Channel3 => ral::read_reg!(ral::gpt, gpt, IR, OF3IE == 1),
    }
}
#[inline(always)]
fn set_ticks(gpt: &ral::gpt::Instance, output_compare: OutputCompare, ticks: u32) {
    match output_compare {
        OutputCompare::Channel1 => ral::write_reg!(ral::gpt, gpt, OCR1, ticks),
        OutputCompare::Channel2 => ral::write_reg!(ral::gpt, gpt, OCR2, ticks),
        OutputCompare::Channel3 => ral::write_reg!(ral::gpt, gpt, OCR3, ticks),
    }
}

#[inline(always)]
fn waker(gpt: &ral::gpt::Instance, output_compare: OutputCompare) -> &'static mut Option<Waker> {
    static mut WAKERS: [[Option<Waker>; 3]; 2] = [[None, None, None], [None, None, None]];
    match &**gpt as *const _ {
        ral::gpt::GPT1 => unsafe { &mut WAKERS[0][output_compare as usize] },
        ral::gpt::GPT2 => unsafe { &mut WAKERS[1][output_compare as usize] },
        _ => unreachable!("There are only two GPTs"),
    }
}

/// A future that waits for the GPT timer to elapse
///
/// Use [`delay_us`](crate::gpt::GPT::delay_us) to create a `Delay`.
pub struct Delay<'a> {
    gpt: &'a ral::gpt::Instance,
    output_compare: OutputCompare,
    hz: u32,
    delay_ns: u32,
    _pin: PhantomPinned,
}

impl<'a> Future for Delay<'a> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if is_triggered(&self.gpt, self.output_compare) {
            clear_trigger(&self.gpt, self.output_compare);
            Poll::Ready(())
        } else if interrupt_enabled(&self.gpt, self.output_compare) {
            Poll::Pending
        } else {
            *waker(&self.gpt, self.output_compare) = Some(cx.waker().clone());
            let period_ns = 1_000_000_000 / self.hz;
            let ticks = self
                .delay_ns
                .checked_div(period_ns)
                .unwrap_or(0)
                .saturating_sub(1);
            let current_tick = ral::read_reg!(ral::gpt, self.gpt, CNT);
            let next_tick = current_tick.wrapping_add(ticks);
            set_ticks(&self.gpt, self.output_compare, next_tick);
            atomic::compiler_fence(atomic::Ordering::Release);
            enable_interrupt(&self.gpt, self.output_compare);
            Poll::Pending
        }
    }
}

impl<'a> Drop for Delay<'a> {
    fn drop(&mut self) {
        disable_interrupt(&self.gpt, self.output_compare);
        clear_trigger(&self.gpt, self.output_compare);
    }
}

#[inline(always)]
#[cfg_attr(not(target_arch = "arm"), allow(unused))]
fn on_interrupt(gpt: &ral::gpt::Instance) {
    [
        OutputCompare::Channel1,
        OutputCompare::Channel2,
        OutputCompare::Channel3,
    ]
    .iter()
    .copied()
    .filter(|&output_compare| is_triggered(&gpt, output_compare))
    .for_each(|output_compare| {
        disable_interrupt(gpt, output_compare);
        let waker = waker(&gpt, output_compare);
        if let Some(waker) = waker.take() {
            waker.wake();
        }
    });
}

interrupts! {
    handler!{unsafe fn GPT1() {
        let gpt = ral::gpt::GPT1::steal();
        on_interrupt(&gpt);
    }}


    handler!{unsafe fn GPT2() {
        let gpt = ral::gpt::GPT2::steal();
        on_interrupt(&gpt);
    }}
}

/// Output compare channels
#[derive(Clone, Copy)]
#[repr(usize)]
enum OutputCompare {
    Channel1 = 0,
    Channel2 = 1,
    Channel3 = 2,
}
