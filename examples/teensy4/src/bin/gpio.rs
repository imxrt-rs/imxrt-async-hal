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

use core::time::Duration;
use futures::future;
use hal::gpio;
use imxrt_async_hal as hal;

const DELAY: Duration = Duration::from_millis(250);

async fn connect<P, Q>(
    mut input: gpio::GPIO<P, gpio::Input>,
    mut output: gpio::GPIO<Q, gpio::Output>,
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
    let mut p13 = hal::gpio::GPIO::new(pins.p13).output();
    let hal::ccm::CCM {
        mut handle,
        perclock,
        ..
    } = hal::ral::ccm::CCM::take()
        .map(hal::ccm::CCM::from_ral_ccm)
        .unwrap();
    let mut perclock = perclock.enable(&mut handle);
    let mut blink_timer = hal::ral::gpt::GPT1::take()
        .map(|mut inst| {
            perclock.clock_gate_gpt(&mut inst, hal::ccm::ClockGate::On);
            hal::GPT::new(inst, &perclock)
        })
        .unwrap();
    let ones = async {
        loop {
            blink_timer.delay(DELAY).await;
            p13.toggle();
        }
    };

    let p12 = hal::gpio::GPIO::new(pins.p12);
    let p14 = hal::gpio::GPIO::new(pins.p14).output();
    let twos = connect(p12, p14);

    let p11 = hal::gpio::GPIO::new(pins.p11);
    let p15 = hal::gpio::GPIO::new(pins.p15).output();
    let fours = connect(p11, p15);

    let p10 = hal::gpio::GPIO::new(pins.p10);
    let p16 = hal::gpio::GPIO::new(pins.p16).output();
    let eights = connect(p10, p16);

    async_embedded::task::block_on(future::join4(ones, twos, fours, eights));
    unreachable!();
}
