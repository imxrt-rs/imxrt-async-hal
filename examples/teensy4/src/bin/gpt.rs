//! Blinking the LED, and toggling pin 14, using two general purpose timers

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
    let mut led = hal::gpio::Gpio::new(pins.p13).output();
    let mut pin14 = hal::gpio::Gpio::new(pins.p14).output();

    let ccm = ral::ccm::CCM::take().unwrap();
    // Select 24MHz crystal oscillator, divide by 24 == 1MHz clock
    ral::modify_reg!(ral::ccm, ccm, CSCMR1, PERCLK_PODF: DIVIDE_24, PERCLK_CLK_SEL: 1);
    // Enable GPT1 clock gate
    ral::modify_reg!(ral::ccm, ccm, CCGR1, CG10: 0b11, CG11: 0b11);

    let gpt = hal::ral::gpt::GPT1::take().unwrap();
    ral::write_reg!(
        ral::gpt,
        gpt,
        CR,
        EN_24M: 1, // Enable crystal oscillator
        CLKSRC: 0b101 // Crystal oscillator clock source
    );
    ral::write_reg!(ral::gpt, gpt, PR, PRESCALER24M: 4); // 1MHz / 5 == 200KHz

    let (mut blink_timer, mut gpio_timer, _) = hal::Gpt::new(gpt);
    let blink_loop = async {
        loop {
            blink_timer.delay(250_000u32 / 5).await;
            led.toggle();
        }
    };
    let gpio_loop = async {
        loop {
            gpio_timer.delay(333_000u32 / 5).await;
            pin14.toggle();
        }
    };
    async_embedded::task::block_on(futures::future::join(blink_loop, gpio_loop));
    unreachable!();
}
