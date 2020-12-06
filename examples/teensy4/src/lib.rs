//! Teensy 4 startup library for the imxrt-async-hal examples
//!
//! This provides a small amount of very necessary code that's included
//! in each example.

#![no_std]

/// Include the firmware configuration block
#[cfg(target_arch = "arm")]
extern crate teensy4_fcb;

use hal::ral;
use imxrt_async_hal as hal;

/// Specify the vector table offset before main() is called
///
/// # Safety
///
/// This does not touch any memory that we need to be initialized.
#[cfg_attr(target_arch = "arm", cortex_m_rt::pre_init)]
#[cfg_attr(not(target_arch = "arm"), allow(unused))]
unsafe fn pre_init() {
    extern "C" {
        static __svectors: u32;
    }
    const SCB_VTOR: *mut u32 = 0xE000_ED08 as *mut u32;
    core::ptr::write_volatile(SCB_VTOR, &__svectors as *const _ as u32);
}

/// Configure a GPT for another example
///
/// See the gpt example for more information on GPT timer
/// configuration.
pub fn new_gpt<N>(
    gpt: ral::gpt::Instance<N>,
    ccm: &ral::ccm::Instance,
) -> (hal::GPT, hal::GPT, hal::GPT) {
    // Select 24MHz crystal oscillator, divide by 24 == 1MHz clock
    ral::modify_reg!(ral::ccm, ccm, CSCMR1, PERCLK_PODF: DIVIDE_24, PERCLK_CLK_SEL: 1);

    // Enable GPT clock gates...
    match &*gpt as *const _ {
        ral::gpt::GPT1 => ral::modify_reg!(ral::ccm, ccm, CCGR1, CG10: 0b11, CG11: 0b11),
        ral::gpt::GPT2 => ral::modify_reg!(ral::ccm, ccm, CCGR0, CG12: 0b11, CG13: 0b11),
        _ => unreachable!("There are only two GPT peripherals"),
    }

    ral::write_reg!(
        ral::gpt,
        gpt,
        CR,
        EN_24M: 1, // Enable crystal oscillator
        CLKSRC: 0b101 // Crystal oscillator clock source
    );
    ral::write_reg!(ral::gpt, gpt, PR, PRESCALER24M: 4); // 1MHz / 5 == 200KHz

    hal::GPT::new(gpt)
}

/// Use a GPT to delay `ms` milliseconds
pub async fn gpt_delay_ms(gpt: &mut hal::GPT, ms: u32) {
    gpt.delay(ms * 1_000 / 5).await
}

/// Use a GPT to delay `us` microseconds
pub async fn gpt_delay_us(gpt: &mut hal::GPT, us: u32) {
    gpt.delay(us / 5).await
}
