//! A UART echo server
//!
//! You should see the LED blinking. You should also be able to
//! write serial data to pin 15, and receive the same data back
//! on pin 14.
//!
//! Advanced tests that require quick changes include
//!
//! - changing the DMA channels to more / less than 16 channels apart
//! - increasing the read / write buffer size

#![no_std]
#![no_main]

#[cfg(target_arch = "arm")]
extern crate panic_halt;
#[cfg(target_arch = "arm")]
extern crate t4_startup;

use imxrt_async_hal as hal;

use futures::future;

const BAUD: u32 = 115_200;

#[cortex_m_rt::entry]
fn main() -> ! {
    let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
    let pins = teensy4_pins::t40::into_pins(pads);
    let mut led = hal::gpio::GPIO::new(pins.p13).output();
    let gpt = hal::ral::gpt::GPT2::take().unwrap();
    let mut ccm = hal::ral::ccm::CCM::take().map(hal::ccm::CCM::new).unwrap();
    let mut timer = hal::GPT::new(gpt, &mut ccm.handle);
    let mut channels = hal::dma::channels(
        hal::ral::dma0::DMA0::take().unwrap(),
        hal::ral::dmamux::DMAMUX::take().unwrap(),
        &mut ccm.handle,
    );

    let uart2 = hal::ral::lpuart::LPUART2::take()
        .and_then(hal::instance::uart)
        .unwrap();
    let mut uart = hal::UART::new(
        uart2,
        pins.p14,
        pins.p15,
        channels[7].take().unwrap(),
        &mut ccm.handle,
    );
    uart.set_baud(BAUD).unwrap();

    let blinking_loop = async {
        loop {
            timer.delay_us(250_000).await;
            led.toggle();
        }
    };

    let echo_loop = async {
        loop {
            let mut buffer = [0; 1];
            uart.read(&mut buffer).await.unwrap();
            uart.write(&buffer).await.unwrap();
        }
    };

    async_embedded::task::block_on(future::join(blinking_loop, echo_loop));
    unreachable!();
}