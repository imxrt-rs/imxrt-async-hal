//! Demonstrates waiting for a GPIO input.
//!
//! Connect LEDs to pins 14, 15, and 16. Also connect
//! - pin 13 to pin 12
//! - pin 14 to pin 11
//! - pin 15 to pin 10
//!
//! On each falling edge of one LED, the adjacent LED will
//! turn on, creating a binary counter. LED toggling happens
//! when the input GPIO (pins 12 and below) detect a falling
//! edge.

#![no_std]
#![no_main]

#[cfg(target_arch = "arm")]
extern crate panic_halt;
#[cfg(target_arch = "arm")]
extern crate t4_startup;

use futures::future;
use hal::gpio;
use imxrt_async_hal as hal;

async fn connect<P, Q>(
    mut input: gpio::Gpio<P, gpio::Input>,
    mut output: gpio::Gpio<Q, gpio::Output>,
) -> !
where
    P: hal::iomuxc::gpio::Pin,
    Q: hal::iomuxc::gpio::Pin,
{
    loop {
        input.wait_for(gpio::Trigger::FallingEdge).await;
        output.toggle();
    }
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
    let pins = teensy4_pins::t40::into_pins(pads);
    let mut p13 = hal::gpio::Gpio::new(pins.p13).output();

    let ccm = hal::ral::ccm::CCM::take().unwrap();
    let (_, _, mut timer) = t4_startup::new_gpt(hal::ral::gpt::GPT2::take().unwrap(), &ccm);
    let ones = async {
        loop {
            t4_startup::gpt_delay_ms(&mut timer, 250).await;
            p13.toggle();
        }
    };

    let p12 = hal::gpio::Gpio::new(pins.p12);
    let p14 = hal::gpio::Gpio::new(pins.p14).output();
    let twos = connect(p12, p14);

    let p11 = hal::gpio::Gpio::new(pins.p11);
    let p15 = hal::gpio::Gpio::new(pins.p15).output();
    let fours = connect(p11, p15);

    let p10 = hal::gpio::Gpio::new(pins.p10);
    let p16 = hal::gpio::Gpio::new(pins.p16).output();
    let eights = connect(p10, p16);

    async_embedded::task::block_on(future::join4(ones, twos, fours, eights));
    unreachable!();
}
