//! Blinking LED from a PIT timer

#![no_std]
#![no_main]

#[cfg(target_arch = "arm")]
extern crate panic_halt;
#[cfg(target_arch = "arm")]
extern crate t4_startup;

use imxrt_async_hal as hal;

#[cortex_m_rt::entry]
fn main() -> ! {
    let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
    let pins = teensy4_pins::t40::into_pins(pads);
    let mut led = hal::gpio::GPIO::new(pins.p13).output();

    let mut ccm = hal::ral::ccm::CCM::take().map(hal::ccm::CCM::new).unwrap();
    let mut perclock = ccm.perclock.enable(&mut ccm.handle);

    let mut pit = hal::ral::pit::PIT::take().unwrap();
    perclock.clock_gate_pit(&mut pit, hal::ccm::ClockActivity::On);

    let (mut pit0, _, _, _) = hal::PIT::new(pit, &perclock);
    let blink_loop = async {
        loop {
            pit0.delay(core::time::Duration::from_millis(250)).await;
            led.toggle();
        }
    };

    async_embedded::task::block_on(blink_loop);
    unreachable!();
}
