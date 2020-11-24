//! An example of how `core::mem::forget` can make things go wrong
//!
//! DO NOT USE THIS CODE! It's only for demonstration. The `unsafe`
//! bits should be your cue.
//!
//! Compile this example without optimizations. Connect a serial
//! device to pins 14 and 15. Run the example, and send a character
//! to the Teensy 4. After you send the first character, you should
//! see the LED turn on.
//!
//! The example shows how a forgotten DMA transfer will still generate
//! data even after the future was forgotten. The LED turns on as soon
//! as the buffer reads non-zero.

#![no_std]
#![no_main]

#[cfg(target_arch = "arm")]
extern crate panic_halt;
#[cfg(target_arch = "arm")]
extern crate t4_startup;

use core::{
    future::Future,
    pin::Pin,
    task::{Context, RawWaker, RawWakerVTable, Waker},
};
use imxrt_async_hal as hal;
use teensy4_pins::common::{P13, P14, P15};

const BAUD: u32 = 115_200;

static VTABLE: RawWakerVTable = {
    unsafe fn clone(data: *const ()) -> RawWaker {
        RawWaker::new(data, &VTABLE)
    }
    unsafe fn wake(_: *const ()) {}
    unsafe fn wake_by_ref(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}
    RawWakerVTable::new(clone, wake, wake_by_ref, drop)
};

static DATA: u32 = 0;
type Buffer = [u8; 64];

#[inline(never)]
fn prepare_receive(uart: &mut hal::UART<P14, P15>, buffer: &mut Buffer) {
    let mut read = uart.read(buffer);
    let pin = unsafe { Pin::new_unchecked(&mut read) };
    let waker =
        unsafe { Waker::from_raw(RawWaker::new(&DATA as *const u32 as *const (), &VTABLE)) };
    let mut ctx = Context::from_waker(&waker);
    let _ = pin.poll(&mut ctx);
    core::mem::forget(read); // If you remove this line, the LED doesn't turn on.
}

#[inline(never)]
fn watch_stack(led: &mut hal::gpio::GPIO<P13, hal::gpio::Output>, buffer: &Buffer) -> ! {
    loop {
        for idx in 0..buffer.len() {
            let elem = &buffer[idx];
            let value = unsafe { core::ptr::read_volatile(elem) };
            if value != 0 {
                loop {
                    led.set();
                }
            }
        }
    }
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
    let pins = teensy4_pins::t40::into_pins(pads);
    let mut led = hal::gpio::GPIO::new(pins.p13).output();

    let hal::ccm::CCM {
        mut handle,
        uart_clock,
        ..
    } = hal::ral::ccm::CCM::take()
        .map(hal::ccm::CCM::from_ral)
        .unwrap();

    let mut channels = hal::dma::channels(
        hal::ral::dma0::DMA0::take()
            .map(|mut dma| {
                handle.set_clock_gate_dma(&mut dma, hal::ccm::ClockGate::On);
                dma
            })
            .unwrap(),
        hal::ral::dmamux::DMAMUX::take().unwrap(),
    );

    let mut uart_clock = uart_clock.enable(&mut handle);
    let uart2 = hal::ral::lpuart::LPUART2::take()
        .map(|mut inst| {
            uart_clock.set_clock_gate(&mut inst, hal::ccm::ClockGate::On);
            inst
        })
        .and_then(hal::instance::uart)
        .unwrap();
    let mut uart = hal::UART::new(
        uart2,
        pins.p14,
        pins.p15,
        channels[7].take().unwrap(),
        &uart_clock,
    );
    uart.set_baud(BAUD).unwrap();

    let mut buffer: Buffer = [0; 64];
    prepare_receive(&mut uart, &mut buffer);
    watch_stack(&mut led, &buffer);
}
