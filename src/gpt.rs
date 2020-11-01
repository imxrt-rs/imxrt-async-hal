use crate::ral;
use core::{
    future::Future,
    pin::Pin,
    sync::atomic,
    task::{Context, Poll, Waker},
    time::Duration,
};

/// General purpose timers (GPT)
///
/// The timer **divides the input clock by 5**. This may affect very precise
/// timing. For a more precise timer, see [`PIT`](struct.PIT.html).
///
/// # Example
///
/// Use GPT1 to block for 250ms.
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::ral::{ccm, gpt};
/// use hal::{ccm::{CCM, ClockGate}, GPT};
///
/// let mut ccm = ccm::CCM::take().map(CCM::from_ral).unwrap();
/// let mut perclock = ccm.perclock.enable(&mut ccm.handle);
/// let mut gpt = gpt::GPT1::take().map(|mut gpt| {
///     perclock.clock_gate_gpt(&mut gpt, ClockGate::On);
///     GPT::new(gpt, &perclock)
/// }).unwrap();
///
/// # async {
/// gpt.delay_us(250_000u32).await;
/// gpt.delay(core::time::Duration::from_millis(250)).await; // Equivalent
/// # };
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "gpt")))]
pub struct GeneralPurposeTimer {
    gpt: ral::gpt::Instance,
    hz: u32,
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

impl GeneralPurposeTimer {
    /// Create a new `GPT` from a RAL GPT instance
    pub fn new(gpt: ral::gpt::Instance, clock: &crate::ccm::PerClock) -> Self {
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
        GeneralPurposeTimer {
            gpt,
            hz: clock.frequency() / DIVIDER,
        }
    }

    /// Wait for the specified `duration` to elapse
    ///
    /// If the microseconds represented by the duration cannot fit in a `u32`, the
    /// delay will saturate at `u32::max_value()` microseconds.
    pub async fn delay(&mut self, duration: Duration) {
        use core::convert::TryFrom;
        self.delay_us(u32::try_from(duration.as_micros()).unwrap_or(u32::max_value()))
            .await
    }
    /// Wait for `microseconds` to elapse
    pub async fn delay_us(&mut self, microseconds: u32) {
        Delay::new(&self.gpt, microseconds, self.hz).await
    }

    /// Returns the `GPT` clock period
    pub fn clock_period(&self) -> Duration {
        Duration::from_micros((1_000_000 / self.hz) as u64)
    }
}

/// Clear the output compare flag
#[inline(always)]
fn clear_trigger(gpt: &ral::gpt::Instance) {
    ral::modify_reg!(ral::gpt, gpt, SR, OF1: 1);
}
#[inline(always)]
fn is_triggered(gpt: &ral::gpt::Instance) -> bool {
    ral::read_reg!(ral::gpt, gpt, SR, OF1 == 1)
}
#[inline(always)]
fn enable_interrupt(gpt: &ral::gpt::Instance) {
    ral::modify_reg!(ral::gpt, gpt, IR, OF1IE: 1);
}
#[inline(always)]
fn disable_interrupt(gpt: &ral::gpt::Instance) {
    ral::modify_reg!(ral::gpt, gpt, IR, OF1IE: 0);
}

enum State {
    Expired,
    Ready { ns: u32 },
    Waiting(Waker),
}

impl State {
    const fn new() -> Self {
        State::Expired
    }
    fn ready(&mut self, us: u32) {
        *self = State::Ready {
            ns: us.saturating_mul(1_000),
        };
    }
}

#[inline(always)]
fn state(gpt: &ral::gpt::Instance) -> &'static mut State {
    static mut STATES: [State; 2] = [State::new(), State::new()];
    match &**gpt as *const _ {
        ral::gpt::GPT1 => unsafe { &mut STATES[0] },
        ral::gpt::GPT2 => unsafe { &mut STATES[1] },
        _ => unreachable!("There are only two GPTs"),
    }
}

/// A future that waits for the timer to elapse
struct Delay<'a> {
    gpt: &'a ral::gpt::Instance,
    hz: u32,
}

impl<'a> Delay<'a> {
    fn new(gpt: &'a ral::gpt::Instance, us: u32, hz: u32) -> Self {
        state(gpt).ready(us);
        Delay { gpt, hz }
    }
}

impl<'a> Delay<'a> {
    fn set_delay(&self, delay_ns: u32) {
        let period_ns = 1_000_000_000 / self.hz;
        let ticks = delay_ns
            .checked_div(period_ns)
            .unwrap_or(0)
            .saturating_sub(1);
        let current_tick = ral::read_reg!(ral::gpt, self.gpt, CNT);
        let next_tick = current_tick.wrapping_add(ticks);
        ral::write_reg!(ral::gpt, self.gpt, OCR1, next_tick);
    }
}

impl<'a> Future for Delay<'a> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let state = state(&self.gpt);
        match state {
            State::Ready { ns } => {
                self.set_delay(*ns);
                *state = State::Waiting(cx.waker().clone());
                atomic::compiler_fence(atomic::Ordering::Release);
                enable_interrupt(&self.gpt);
                Poll::Pending
            }
            State::Expired if is_triggered(&self.gpt) => {
                clear_trigger(&self.gpt);
                Poll::Ready(())
            }
            State::Waiting(_) | State::Expired => Poll::Pending,
        }
    }
}

impl<'a> Drop for Delay<'a> {
    fn drop(&mut self) {
        disable_interrupt(&self.gpt);
        clear_trigger(&self.gpt);
        *state(&self.gpt) = State::new()
    }
}

#[inline(always)]
#[cfg_attr(not(target_arch = "arm"), allow(unused))]
fn on_interrupt(gpt: &ral::gpt::Instance, state: &mut State) {
    if is_triggered(gpt) {
        disable_interrupt(gpt);
        let waiting = core::mem::replace(state, State::Expired);
        if let State::Waiting(waker) = waiting {
            waker.wake();
        } else {
            panic!("Cannot expire a timer that's not waiting!");
        }
    }
}

interrupts! {
    handler!{unsafe fn GPT1() {
        let gpt = ral::gpt::GPT1::steal();
        on_interrupt(&gpt, state(&gpt));
    }}


    handler!{unsafe fn GPT2() {
        let gpt = ral::gpt::GPT2::steal();
        on_interrupt(&gpt, state(&gpt));
    }}
}
