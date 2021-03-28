use crate::ral;

use core::{
    future::Future,
    pin::Pin,
    sync::atomic,
    task::{Context, Poll, Waker},
};

/// Periodic interrupt timer (PIT)
///
/// The PIT timer channels are the most precise timers in the HAL. PIT timers run on the periodic clock
/// frequency.
///
/// A single hardware PIT instance has four PIT channels. Use [`new`](PIT::new()) to acquire these four
/// channels.
///
/// # Example
///
/// Delay for 250ms using PIT channel 3.
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::ral;
/// use hal::PIT;
///
/// let ccm = ral::ccm::CCM::take().unwrap();
/// // Select 24MHz crystal oscillator, divide by 24 == 1MHz clock
/// ral::modify_reg!(ral::ccm, ccm, CSCMR1, PERCLK_PODF: DIVIDE_24, PERCLK_CLK_SEL: 1);
/// // Enable PIT clock gate
/// ral::modify_reg!(ral::ccm, ccm, CCGR1, CG6: 0b11);
/// let (_, _, _, mut pit) = ral::pit::PIT::take()
///     .map(PIT::new)
///     .unwrap();
///
/// # async {
/// pit.delay(250_000).await;
/// # };
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "pit")))]
pub struct PIT {
    channel: register::ChannelInstance,
}

impl PIT {
    /// Acquire four PIT channels from the RAL's PIT instance
    pub fn new(pit: ral::pit::Instance) -> (PIT, PIT, PIT, PIT) {
        ral::write_reg!(ral::pit, pit, MCR, MDIS: MDIS_0);
        // Reset all PIT channels
        //
        // PIT channels may be used by a systems boot ROM, or another
        // user. Set them to a known, good state.
        ral::write_reg!(ral::pit, pit, TCTRL0, 0);
        ral::write_reg!(ral::pit, pit, TCTRL1, 0);
        ral::write_reg!(ral::pit, pit, TCTRL2, 0);
        ral::write_reg!(ral::pit, pit, TCTRL3, 0);

        unsafe {
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::PIT);
            (
                PIT {
                    channel: register::ChannelInstance::zero(),
                },
                PIT {
                    channel: register::ChannelInstance::one(),
                },
                PIT {
                    channel: register::ChannelInstance::two(),
                },
                PIT {
                    channel: register::ChannelInstance::three(),
                },
            )
        }
    }
    /// Wait for the counts to elapse
    ///
    /// The elapsed time is a function of your clock selection and clock frequency.
    pub async fn delay(&mut self, count: u32) {
        Delay {
            channel: &mut self.channel,
            count,
        }
        .await
    }
}

static mut WAKERS: [Option<Waker>; 4] = [None, None, None, None];

struct Delay<'a> {
    channel: &'a mut register::ChannelInstance,
    count: u32,
}

impl<'a> Future for Delay<'a> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if ral::read_reg!(register, self.channel, TFLG, TIF == 1) {
            // Complete! W1C
            ral::write_reg!(register, self.channel, TFLG, TIF: 1);
            Poll::Ready(())
        } else if ral::read_reg!(register, self.channel, TCTRL) != 0 {
            // We're active; do nothing
            Poll::Pending
        } else {
            // Neither complete nor active; prepare to run
            ral::write_reg!(register, self.channel, LDVAL, self.count);
            unsafe {
                WAKERS[self.channel.index()] = Some(cx.waker().clone());
            }
            atomic::compiler_fence(atomic::Ordering::SeqCst);
            ral::modify_reg!(register, self.channel, TCTRL, TIE: 1);
            ral::modify_reg!(register, self.channel, TCTRL, TEN: 1);
            Poll::Pending
        }
    }
}

impl<'a> Drop for Delay<'a> {
    fn drop(&mut self) {
        ral::write_reg!(register, self.channel, TCTRL, 0);
    }
}

interrupts! {
    handler!{unsafe fn PIT() {
        use register::ChannelInstance;

        [
            ChannelInstance::zero(),
            ChannelInstance::one(),
            ChannelInstance::two(),
            ChannelInstance::three(),
        ]
            .iter_mut()
            .zip(WAKERS.iter_mut())
            .filter(|(channel, _)| ral::read_reg!(register, channel, TFLG, TIF == 1))
            .for_each(|(channel, waker)| {
                ral::write_reg!(register, channel, TCTRL, 0);
                if let Some(waker) = waker.take() {
                    waker.wake();
                }
            });
    }}
}

/// The auto-generated RAL API is cumbersome. This is a macro-compatible API that makes it
/// easier to work with.
///
/// The approach here is to
///
/// - take the RAL flags, and remove the channel number (copy-paste from RAL)
/// - expose a 'Channel' as a collection of PIT channel registers (copy-paste from RAL)
mod register {
    #![allow(unused, non_snake_case, non_upper_case_globals)] // Compatibility with RAL

    use crate::ral::{RORegister, RWRegister};

    #[repr(C)]
    pub struct ChannelRegisterBlock {
        /// Timer Load Value Register
        pub LDVAL: RWRegister<u32>,

        /// Current Timer Value Register
        pub CVAL: RORegister<u32>,

        /// Timer Control Register
        pub TCTRL: RWRegister<u32>,

        /// Timer Flag Register
        pub TFLG: RWRegister<u32>,
    }

    pub struct ChannelInstance {
        addr: u32,
        idx: usize,
        _marker: ::core::marker::PhantomData<*const ChannelRegisterBlock>,
    }

    impl ::core::ops::Deref for ChannelInstance {
        type Target = ChannelRegisterBlock;
        #[inline(always)]
        fn deref(&self) -> &ChannelRegisterBlock {
            unsafe { &*(self.addr as *const _) }
        }
    }

    const PIT_BASE_ADDRESS: u32 = 0x4008_4000;
    const PIT_CHANNEL_0_ADDRESS: u32 = PIT_BASE_ADDRESS + 0x100;
    const PIT_CHANNEL_1_ADDRESS: u32 = PIT_BASE_ADDRESS + 0x110;
    const PIT_CHANNEL_2_ADDRESS: u32 = PIT_BASE_ADDRESS + 0x120;
    const PIT_CHANNEL_3_ADDRESS: u32 = PIT_BASE_ADDRESS + 0x130;

    impl ChannelInstance {
        const unsafe fn new(addr: u32, idx: usize) -> Self {
            ChannelInstance {
                addr,
                idx,
                _marker: core::marker::PhantomData,
            }
        }
        pub const fn index(&self) -> usize {
            self.idx
        }
        pub const unsafe fn zero() -> Self {
            Self::new(PIT_CHANNEL_0_ADDRESS, 0)
        }
        pub const unsafe fn one() -> Self {
            Self::new(PIT_CHANNEL_1_ADDRESS, 1)
        }
        pub const unsafe fn two() -> Self {
            Self::new(PIT_CHANNEL_2_ADDRESS, 2)
        }
        pub const unsafe fn three() -> Self {
            Self::new(PIT_CHANNEL_3_ADDRESS, 3)
        }
    }

    /// Timer Load Value Register
    pub mod LDVAL {

        /// Timer Start Value
        pub mod TSV {
            /// Offset (0 bits)
            pub const offset: u32 = 0;
            /// Mask (32 bits: 0xffffffff << 0)
            pub const mask: u32 = 0xffffffff << offset;
            /// Read-only values (empty)
            pub mod R {}
            /// Write-only values (empty)
            pub mod W {}
            /// Read-write values (empty)
            pub mod RW {}
        }
    }

    /// Current Timer Value Register
    pub mod CVAL {

        /// Current Timer Value
        pub mod TVL {
            /// Offset (0 bits)
            pub const offset: u32 = 0;
            /// Mask (32 bits: 0xffffffff << 0)
            pub const mask: u32 = 0xffffffff << offset;
            /// Read-only values (empty)
            pub mod R {}
            /// Write-only values (empty)
            pub mod W {}
            /// Read-write values (empty)
            pub mod RW {}
        }
    }

    /// Timer Control Register
    pub mod TCTRL {

        /// Timer Enable
        pub mod TEN {
            /// Offset (0 bits)
            pub const offset: u32 = 0;
            /// Mask (1 bit: 1 << 0)
            pub const mask: u32 = 1 << offset;
            /// Read-only values (empty)
            pub mod R {}
            /// Write-only values (empty)
            pub mod W {}
            /// Read-write values
            pub mod RW {

                /// 0b0: Timer n is disabled.
                pub const TEN_0: u32 = 0b0;

                /// 0b1: Timer n is enabled.
                pub const TEN_1: u32 = 0b1;
            }
        }

        /// Timer Interrupt Enable
        pub mod TIE {
            /// Offset (1 bits)
            pub const offset: u32 = 1;
            /// Mask (1 bit: 1 << 1)
            pub const mask: u32 = 1 << offset;
            /// Read-only values (empty)
            pub mod R {}
            /// Write-only values (empty)
            pub mod W {}
            /// Read-write values
            pub mod RW {

                /// 0b0: Interrupt requests from Timer n are disabled.
                pub const TIE_0: u32 = 0b0;

                /// 0b1: Interrupt will be requested whenever TIF is set.
                pub const TIE_1: u32 = 0b1;
            }
        }

        /// Chain Mode
        pub mod CHN {
            /// Offset (2 bits)
            pub const offset: u32 = 2;
            /// Mask (1 bit: 1 << 2)
            pub const mask: u32 = 1 << offset;
            /// Read-only values (empty)
            pub mod R {}
            /// Write-only values (empty)
            pub mod W {}
            /// Read-write values
            pub mod RW {

                /// 0b0: Timer is not chained.
                pub const CHN_0: u32 = 0b0;

                /// 0b1: Timer is chained to previous timer. For example, for Channel 2, if this field is set, Timer 2 is chained to Timer 1.
                pub const CHN_1: u32 = 0b1;
            }
        }
    }

    /// Timer Flag Register
    pub mod TFLG {

        /// Timer Interrupt Flag
        pub mod TIF {
            /// Offset (0 bits)
            pub const offset: u32 = 0;
            /// Mask (1 bit: 1 << 0)
            pub const mask: u32 = 1 << offset;
            /// Read-only values (empty)
            pub mod R {}
            /// Write-only values (empty)
            pub mod W {}
            /// Read-write values
            pub mod RW {

                /// 0b0: Timeout has not yet occurred.
                pub const TIF_0: u32 = 0b0;

                /// 0b1: Timeout has occurred.
                pub const TIF_1: u32 = 0b1;
            }
        }
    }
}
