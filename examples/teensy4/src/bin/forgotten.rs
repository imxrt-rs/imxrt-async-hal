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
//! as the buffer reads non-zero. The example is sensitive to how the
//! stack is laid out in each function call.

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
use hal::ral;
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

/// Prepare to receive UART data from a DMA transfer into stack memory
#[inline(never)]
fn prepare_receive(uart: &mut hal::UART<P14, P15>) {
    let mut buffer: [u8; 8] = [0; 8];
    let mut read = uart.read(&mut buffer);
    let pin = unsafe { Pin::new_unchecked(&mut read) };
    let waker =
        unsafe { Waker::from_raw(RawWaker::new(&DATA as *const u32 as *const (), &VTABLE)) };
    let mut ctx = Context::from_waker(&waker);
    let _ = pin.poll(&mut ctx);
    core::mem::forget(read); // If you remove this line, the LED doesn't turn on.
}

/// Watch the stack memory, and turn on the LED when its non-zero
#[inline(never)]
fn watch_stack(led: &mut hal::gpio::GPIO<P13, hal::gpio::Output>) -> ! {
    let buffer: [u8; 256] = [0; 256];
    loop {
        for elem in &buffer {
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

    let ccm = hal::ral::ccm::CCM::take().unwrap();
    ral::modify_reg!(ral::ccm, ccm, CSCDR1, UART_CLK_SEL: 1 /* XTAL */, UART_CLK_PODF: 0);
    ral::modify_reg!(ral::ccm, ccm, CCGR0, CG14: 0b11);

    let hal::ccm::CCM { mut handle, .. } = unsafe { Some(hal::ral::ccm::CCM::steal()) }
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

    let uart2 = hal::ral::lpuart::LPUART2::take()
        .and_then(hal::instance::uart)
        .unwrap();
    let mut uart = hal::UART::new(uart2, pins.p14, pins.p15, channels[7].take().unwrap());
    uart.set_baud(BAUD, 24_000_000 /* XTAL Hz */).unwrap();

    prepare_receive(&mut uart);
    watch_stack(&mut led);
}
