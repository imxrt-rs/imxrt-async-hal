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

use futures::future;
use hal::ral;
use imxrt_async_hal as hal;
const BAUD: u32 = 115_200;

const CLOCK_FREQUENCY_HZ: u32 = 24_000_000; // XTAL
const CLOCK_DIVIDER: u32 = 1;

#[cortex_m_rt::entry]
fn main() -> ! {
    let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
    let pins = teensy4_pins::t40::into_pins(pads);
    let mut led = hal::gpio::GPIO::new(pins.p13).output();
    let gpt = hal::ral::gpt::GPT2::take().unwrap();

    let ccm = hal::ral::ccm::CCM::take().unwrap();
    ral::modify_reg!(ral::ccm, ccm, CSCDR1, UART_CLK_SEL: 1 /* Oscillator */, UART_CLK_PODF: CLOCK_DIVIDER - 1);
    // LPUART2 clock gate on
    ral::modify_reg!(ral::ccm, ccm, CCGR0, CG14: 0b11);
    // DMA clock gate on
    ral::modify_reg!(ral::ccm, ccm, CCGR5, CG3: 0b11);

    let (mut timer, _, _) = t4_startup::new_gpt(gpt, &ccm);

    let mut channels = hal::dma::channels(
        hal::ral::dma0::DMA0::take().unwrap(),
        hal::ral::dmamux::DMAMUX::take().unwrap(),
    );

    let uart2 = hal::ral::lpuart::LPUART2::take()
        .and_then(hal::instance::uart)
        .unwrap();
    let mut uart = hal::UART::new(uart2, pins.p14, pins.p15);
    let mut channel = channels[7].take().unwrap();
    channel.set_interrupt_on_completion(true);
    uart.set_baud(BAUD, CLOCK_FREQUENCY_HZ / CLOCK_DIVIDER)
        .unwrap();

    let blinking_loop = async {
        loop {
            t4_startup::gpt_delay_ms(&mut timer, 250).await;
            led.toggle();
        }
    };

    let echo_loop = async {
        loop {
            let mut buffer = [0; 1];
            uart.dma_read(&mut channel, &mut buffer).await.unwrap();
            uart.dma_write(&mut channel, &buffer).await.unwrap();
        }
    };

    async_embedded::task::block_on(future::join(blinking_loop, echo_loop));
    unreachable!();
}
