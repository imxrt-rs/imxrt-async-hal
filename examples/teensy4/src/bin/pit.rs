//! Blinking LED from a PIT timer

#![no_std]
#![no_main]

#[cfg(target_arch = "arm")]
extern crate panic_halt;
#[cfg(target_arch = "arm")]
extern crate t4_startup;

use hal::ral;
use imxrt_async_hal as hal;

#[cortex_m_rt::entry]
fn main() -> ! {
    let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
    let pins = teensy4_pins::t40::into_pins(pads);
    let mut led = hal::gpio::GPIO::new(pins.p13).output();

    let ccm = ral::ccm::CCM::take().unwrap();
    // Select 24MHz crystal oscillator, divide by 24 == 1MHz clock
    ral::modify_reg!(ral::ccm, ccm, CSCMR1, PERCLK_PODF: DIVIDE_24, PERCLK_CLK_SEL: 1);
    // Enable PIT clock gate
    ral::modify_reg!(ral::ccm, ccm, CCGR1, CG6: 0b11);
    ral::ccm::CCM::release(ccm);

    let (mut pit, _, _, _) = ral::pit::PIT::take().map(hal::PIT::new).unwrap();
    let blink_loop = async {
        loop {
            const DELAY_MS: u32 = 250_000; // 1MHz clock, 1us period
            pit.delay(DELAY_MS).await;
            led.toggle();
        }
    };

    async_embedded::task::block_on(blink_loop);
    unreachable!();
}
