//! General purpose timers (GPT)
//!
//! The timer **divides the input clock by 5**. This may affect very precise
//! timing. For a more precise timer, see [`PIT`](crate::pit::PIT).
//!
//! Each GPT instance turns into three GPT timers. Use [`new`](crate::gpt::Gpt::new)
//! to acquire the three timers.
//!
//! # Example
//!
//! Use GPT1 to block for 250ms.
//!
//! ```no_run
//! use imxrt_async_hal as hal;
//! use hal::ral as ral;
//! use ral::{ccm, gpt};
//! use hal::Gpt;
//!
//! let ccm = ccm::CCM::take().unwrap();
//! // Select 24MHz crystal oscillator, divide by 24 == 1MHz clock
//! ral::modify_reg!(ral::ccm, ccm, CSCMR1, PERCLK_PODF: DIVIDE_24, PERCLK_CLK_SEL: 1);
//! // Enable GPT1 clock gate
//! ral::modify_reg!(ral::ccm, ccm, CCGR1, CG10: 0b11, CG11: 0b11);
//!
//! let gpt = hal::ral::gpt::GPT1::take().unwrap();
//! ral::write_reg!(
//!     ral::gpt,
//!     gpt,
//!     CR,
//!     EN_24M: 1, // Enable crystal oscillator
//!     CLKSRC: 0b101 // Crystal oscillator clock source
//! );
//! ral::write_reg!(ral::gpt, gpt, PR, PRESCALER24M: 4); // 1MHz / 5 == 200KHz
//! let (mut gpt, _, _) = Gpt::new(gpt);
//!
//! # async {
//! gpt.delay(250_000u32 / 5).await;
//! # };
//! ```

use crate::ral;
use core::{
    future::Future,
    marker::PhantomPinned,
    pin::Pin,
    sync::atomic,
    task::{Context, Poll, Waker},
};

/// The GPT timer
///
/// See the [module-level documentation](mod@crate::gpt) for more information.
#[cfg_attr(docsrs, doc(cfg(feature = "gpt")))]
pub struct Gpt {
    gpt: *const ral::gpt::RegisterBlock,
    output_compare: OutputCompare,
}

impl Gpt {
    /// Create a new `Gpt` from a RAL GPT instance
    pub fn new<N>(gpt: ral::gpt::Instance<N>) -> (Self, Self, Self) {
        let irq = match &*gpt as *const _ {
            ral::gpt::GPT1 => ral::interrupt::GPT1,
            ral::gpt::GPT2 => ral::interrupt::GPT2,
            _ => unreachable!("There are only two GPTs"),
        };

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
        (
            Gpt {
                gpt: &*gpt,
                output_compare: OutputCompare::Channel1,
            },
            Gpt {
                gpt: &*gpt,
                output_compare: OutputCompare::Channel2,
            },
            Gpt {
                gpt: &*gpt,
                output_compare: OutputCompare::Channel3,
            },
        )
    }

    /// Wait for `ticks` clock counts to elapse
    ///
    /// The elapsed time depends on your clock configuration.
    pub fn delay(&mut self, ticks: u32) -> Delay<'_> {
        Delay {
            gpt: unsafe { &*self.gpt },
            ticks,
            output_compare: self.output_compare,
            _pin: PhantomPinned,
        }
    }
}

/// Clear the output compare flag
#[inline(always)]
fn clear_trigger(gpt: &ral::gpt::RegisterBlock, output_compare: OutputCompare) {
    match output_compare {
        OutputCompare::Channel1 => ral::modify_reg!(ral::gpt, gpt, SR, OF1: 1),
        OutputCompare::Channel2 => ral::modify_reg!(ral::gpt, gpt, SR, OF2: 1),
        OutputCompare::Channel3 => ral::modify_reg!(ral::gpt, gpt, SR, OF3: 1),
    }
}
#[inline(always)]
fn is_triggered(gpt: &ral::gpt::RegisterBlock, output_compare: OutputCompare) -> bool {
    match output_compare {
        OutputCompare::Channel1 => ral::read_reg!(ral::gpt, gpt, SR, OF1 == 1),
        OutputCompare::Channel2 => ral::read_reg!(ral::gpt, gpt, SR, OF2 == 1),
        OutputCompare::Channel3 => ral::read_reg!(ral::gpt, gpt, SR, OF3 == 1),
    }
}
#[inline(always)]
fn enable_interrupt(gpt: &ral::gpt::RegisterBlock, output_compare: OutputCompare) {
    match output_compare {
        OutputCompare::Channel1 => ral::modify_reg!(ral::gpt, gpt, IR, OF1IE: 1),
        OutputCompare::Channel2 => ral::modify_reg!(ral::gpt, gpt, IR, OF2IE: 1),
        OutputCompare::Channel3 => ral::modify_reg!(ral::gpt, gpt, IR, OF3IE: 1),
    }
}
#[inline(always)]
fn disable_interrupt(gpt: &ral::gpt::RegisterBlock, output_compare: OutputCompare) {
    match output_compare {
        OutputCompare::Channel1 => ral::modify_reg!(ral::gpt, gpt, IR, OF1IE: 0),
        OutputCompare::Channel2 => ral::modify_reg!(ral::gpt, gpt, IR, OF2IE: 0),
        OutputCompare::Channel3 => ral::modify_reg!(ral::gpt, gpt, IR, OF3IE: 0),
    }
}
#[inline(always)]
fn interrupt_enabled(gpt: &ral::gpt::RegisterBlock, output_compare: OutputCompare) -> bool {
    match output_compare {
        OutputCompare::Channel1 => ral::read_reg!(ral::gpt, gpt, IR, OF1IE == 1),
        OutputCompare::Channel2 => ral::read_reg!(ral::gpt, gpt, IR, OF2IE == 1),
        OutputCompare::Channel3 => ral::read_reg!(ral::gpt, gpt, IR, OF3IE == 1),
    }
}
#[inline(always)]
fn set_ticks(gpt: &ral::gpt::RegisterBlock, output_compare: OutputCompare, ticks: u32) {
    match output_compare {
        OutputCompare::Channel1 => ral::write_reg!(ral::gpt, gpt, OCR1, ticks),
        OutputCompare::Channel2 => ral::write_reg!(ral::gpt, gpt, OCR2, ticks),
        OutputCompare::Channel3 => ral::write_reg!(ral::gpt, gpt, OCR3, ticks),
    }
}

#[inline(always)]
fn waker(
    gpt: &ral::gpt::RegisterBlock,
    output_compare: OutputCompare,
) -> &'static mut Option<Waker> {
    static mut WAKERS: [[Option<Waker>; 3]; 2] = [[None, None, None], [None, None, None]];
    match &*gpt as *const _ {
        ral::gpt::GPT1 => unsafe { &mut WAKERS[0][output_compare as usize] },
        ral::gpt::GPT2 => unsafe { &mut WAKERS[1][output_compare as usize] },
        _ => unreachable!("There are only two GPTs"),
    }
}

/// A future that waits for the timer to elapse
pub struct Delay<'a> {
    gpt: &'a ral::gpt::RegisterBlock,
    output_compare: OutputCompare,
    _pin: PhantomPinned,
    ticks: u32,
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
            let current_tick = ral::read_reg!(ral::gpt, self.gpt, CNT);
            let next_tick = current_tick.wrapping_add(self.ticks);
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
fn on_interrupt(gpt: &ral::gpt::RegisterBlock) {
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
