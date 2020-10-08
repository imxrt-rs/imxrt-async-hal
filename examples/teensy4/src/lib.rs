//! Teensy 4 startup library for the imxrt-async-hal examples
//!
//! This provides a small amount of very necessary code that's included
//! in each example.

#![no_std]

/// Include the firmware configuration block
#[cfg(target_arch = "arm")]
extern crate teensy4_fcb;

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
