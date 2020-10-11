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
/// The timer ticks every **5us (200KHz)**. This may affect very precise timing.
/// For a more precise timer, see [`PIT`](struct.PIT.html).
///
/// # Example
///
/// Use GPT1 to block for 250ms.
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::ral::{ccm, gpt};
/// use hal::{ccm::CCM, GPT};
///
/// let mut ccm = ccm::CCM::take().map(CCM::new).unwrap();
/// let mut gpt = gpt::GPT1::take().map(|gpt| GPT::new(gpt, &mut ccm.handle)).unwrap();
///
/// # async {
/// gpt.delay_us(250_000u32).await;
/// gpt.delay(core::time::Duration::from_millis(250)).await; // Equivalent
/// # };
/// ```
pub struct GeneralPurposeTimer(ral::gpt::Instance);

/// GPT clock divider
///
/// This crystal oscillator is very sensitive. Not all values
/// seem to work. 5 is one of them that does. So is 3. 10 does
/// not work. The field is supposed to support values up to 0xF.
///
/// The seL4 project also notes issues with this divider value.
/// Can't find anything in the errata...
const DIVIDER: u32 = 5;

/// GPT effective frequency
const CLOCK_HZ: u32 = crate::PERIODIC_CLOCK_FREQUENCY_HZ / DIVIDER;
const CLOCK_PERIOD_US: u32 = 1_000_000u32 / CLOCK_HZ;
const _STATIC_ASSERT: [u32; 1] = [0; (CLOCK_PERIOD_US == 5) as usize];
const CLOCK_PERIOD: Duration = Duration::from_micros(CLOCK_PERIOD_US as u64);

impl GeneralPurposeTimer {
    /// Create a new `GPT` from a RAL GPT instance
    pub fn new(gpt: ral::gpt::Instance, ccm: &mut crate::ccm::Handle) -> Self {
        crate::enable_periodic_clock_root(ccm);
        let irq = match &*gpt as *const _ {
            ral::gpt::GPT1 => {
                ral::modify_reg!(ral::ccm, ccm.0, CCGR1, CG10: 0x3, CG11: 0x3);
                ral::interrupt::GPT1
            }
            ral::gpt::GPT2 => {
                ral::modify_reg!(ral::ccm, ccm.0, CCGR0, CG12: 0x3, CG13: 0x3);
                ral::interrupt::GPT2
            }
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
        GeneralPurposeTimer(gpt)
    }

    /// Wait for the specified `duration` to elapse
    ///
    /// If the microseconds represented by the duration cannot be represented by a `u32`, the
    /// delay will saturate at `u32::max_value()` microseconds.
    pub async fn delay(&mut self, duration: Duration) {
        use core::convert::TryFrom;
        self.delay_us(u32::try_from(duration.as_micros()).unwrap_or(u32::max_value()))
            .await
    }
    /// Wait for `microseconds` to elapse
    pub async fn delay_us(&mut self, microseconds: u32) {
        Delay::new(&self.0, microseconds).await
    }

    /// Returns the `GPT` clock period: 5us
    pub const fn clock_period(&self) -> Duration {
        CLOCK_PERIOD
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
    Ready { us: u32 },
    Waiting(Waker),
}

impl State {
    const fn new() -> Self {
        State::Expired
    }
    fn ready(&mut self, us: u32) {
        *self = State::Ready { us };
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
}

impl<'a> Delay<'a> {
    fn new(gpt: &'a ral::gpt::Instance, us: u32) -> Self {
        state(gpt).ready(us);
        Delay { gpt }
    }
}

impl<'a> Delay<'a> {
    fn set_delay(&self, delay: u32) {
        let ticks = delay
            .checked_div(CLOCK_PERIOD_US)
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
            State::Ready { us } => {
                self.set_delay(*us);
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
    unsafe fn GPT1() {
        let gpt = ral::gpt::GPT1::steal();
        on_interrupt(&gpt, state(&gpt));
    }


    unsafe fn GPT2() {
        let gpt = ral::gpt::GPT2::steal();
        on_interrupt(&gpt, state(&gpt));
    }
}
