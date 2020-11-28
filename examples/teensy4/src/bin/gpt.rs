//! Blinking the LED, and toggling pin 14, using two general purpose timers

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
    let mut pin14 = hal::gpio::GPIO::new(pins.p14).output();

    let mut ccm = hal::ral::ccm::CCM::take()
        .map(hal::ccm::CCM::from_ral)
        .unwrap();
    let mut perclock = ccm.perclock.enable(&mut ccm.handle);

    let mut gpt = hal::ral::gpt::GPT1::take().unwrap();
    perclock.set_clock_gate_gpt(&mut gpt, hal::ccm::ClockGate::On);

    let (mut blink_timer, mut gpio_timer, _) = hal::GPT::new(gpt, &perclock);
    let blink_loop = async {
        loop {
            blink_timer.delay(Duration::from_millis(250)).await;
            led.toggle();
        }
    };
    let gpio_loop = async {
        loop {
            gpio_timer.delay_us(333_000).await;
            pin14.toggle();
        }
    };
    async_embedded::task::block_on(futures::future::join(blink_loop, gpio_loop));
    unreachable!();
}
