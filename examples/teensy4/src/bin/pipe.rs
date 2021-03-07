//! DMA-based channels for message passing, called 'pipes'
//!
//! Expected output:
//!
//! 1. The LED blinks 10 times at 5 hz
//! 2. The LED goes dark for 0.5 seconds
//! 3. The LED blinks 9 times at 2 Hz
//! 4. The LED blinks forever at 1 Hz
//!
//! If there's ever an error, it's indicated by a steady LED.
//!
//! The demo shows how you can use channels to send messages between tasks.
//! Dropping channels will close the other end, unblocking it if necessary.

#![no_std]
#![no_main]

#[cfg(target_arch = "arm")]
extern crate panic_halt;
#[cfg(target_arch = "arm")]
extern crate t4_startup;

use futures::future;
use imxrt_async_hal as hal;

#[cortex_m_rt::entry]
fn main() -> ! {
    let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
    let pins = teensy4_pins::t40::into_pins(pads);
    let mut led = hal::gpio::GPIO::new(pins.p13).output();

    let ccm = hal::ral::ccm::CCM::take().unwrap();
    let (_, mut timer, _) = t4_startup::new_gpt(hal::ral::gpt::GPT1::take().unwrap(), &ccm);

    let hal::ccm::CCM { mut handle, .. } = unsafe { Some(hal::ral::ccm::CCM::steal()) }
        .map(hal::ccm::CCM::from_ral)
        .unwrap();

    let mut dmas = hal::dma::channels(
        hal::ral::dma0::DMA0::take()
            .map(|mut dma| {
                handle.set_clock_gate_dma(&mut dma, hal::ccm::ClockGate::On);
                dma
            })
            .unwrap(),
        hal::ral::dmamux::DMAMUX::take().unwrap(),
    );

    let (mut tx, mut rx) = hal::dma::pipe::new(dmas[13].take().unwrap());
    let (mut tx2, mut rx2) = hal::dma::pipe::new(dmas[14].take().unwrap());
    let (mut tx3, mut rx3) = hal::dma::pipe::new(dmas[29].take().unwrap());
    let sender = async {
        let mut counter: i32 = 0;
        loop {
            tx.send(&counter).await.unwrap();
            t4_startup::gpt_delay_us(&mut timer, 100_000).await;
            counter = counter.wrapping_add(1);
            if counter == 20 {
                drop(tx);
                break;
            }
        }
        t4_startup::gpt_delay_us(&mut timer, 500_000).await;
        loop {
            let actual: i32 = rx2.receive().await.unwrap();
            if actual == 118 {
                drop(rx2);
                break;
            }
            t4_startup::gpt_delay_us(&mut timer, 250_000).await;
        }
        loop {
            t4_startup::gpt_delay_us(&mut timer, 1_000_000).await;
            tx3.send(&0i32).await.unwrap();
        }
    };

    let receiver = async {
        let mut expected: i32 = 0;
        loop {
            let actual = match rx.receive().await {
                Ok(a) => a,
                Err(hal::dma::Error::Cancelled) => {
                    led.clear();
                    break;
                }
                Err(_) => loop {
                    led.set();
                },
            };
            if actual == expected {
                led.toggle()
            }
            expected = expected.wrapping_add(1);
        }
        let mut value: i32 = 100;
        loop {
            match tx2.send(&value).await {
                Ok(()) => {
                    value = value.wrapping_add(1);
                    led.toggle();
                }
                Err(hal::dma::Error::Cancelled) => {
                    led.clear();
                    break;
                }
                _ => loop {
                    led.set();
                },
            }
        }
        loop {
            led.toggle();
            rx3.receive().await.unwrap();
        }
    };

    async_embedded::task::block_on(future::join(sender, receiver));
    unreachable!();
}
