//! Blinking LED using a general purpose timer

#![no_std]
#![no_main]

#[cfg(target_arch = "arm")]
extern crate panic_halt;
#[cfg(target_arch = "arm")]
extern crate t4_startup;

use core::time::Duration;
use imxrt_async_hal as hal;

#[cortex_m_rt::entry]
fn main() -> ! {
    let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
    let pins = teensy4_pins::t40::into_pins(pads);
    let mut led = hal::gpio::GPIO::new(pins.p13).output();
    let mut ccm = hal::ral::ccm::CCM::take().map(hal::ccm::CCM::new).unwrap();
    let mut blink_timer = hal::GPT::new(hal::ral::gpt::GPT1::take().unwrap(), &mut ccm.handle);
    let blink_loop = async {
        loop {
            blink_timer.delay(Duration::from_millis(250)).await;
            led.toggle();
        }
    };
    async_embedded::task::block_on(blink_loop);
    unreachable!();
}
