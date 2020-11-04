# imxrt-async-hal

Embedded, async Rust for i.MX RT processors

`imxrt-async-hal` brings async Rust support to NXP's i.MX RT processors.
The crate includes `await`able peripherals and timers. Once the I/O completes
or the timer elapses, an interrupt fires to wake the executor. By combining
interrupt-driven peripherals with a single-threaded executor, you can write
multiple, concurrent tasks for your embedded system.

The crate registers interrupt handlers to support async execution. The implementation
registers interrupt handlers statically, using the [`cortex-m-rt`] interfaces. This
means that your final program should also depend on `cortex-m-rt`, or bet at least
`cortex-m-rt` compatible.

[`cortex-m-rt`]: https://crates.io/crates/cortex-m-rt

The crate does not include an executor, or any API for driving futures. You will
need to select your own executor that supports a Cortex-M system.
The executor should be thread safe, prepared to handle wakes from interrupt handlers.

See the project's examples to try this code on your hardware. This crate has been
primarily developed using a Teensy 4 (i.MX RT 1062). It compiles for other
i.MX RT chip variants.

## Dependencies

- A Rust installation; recommended installation using `rustup`. We support the
  latest, stable Rust toolchain.

- The `thumbv7em-none-eabihf` Rust target, which may be installed using
  `rustup`: `rustup target add thumbv7em-none-eabihf`

  The target is only necessary when building for an embedded system. The
  main crate should build and test on your host.

- An embedded system with a compatible i.MX RT processor.

## Feature flags

You're **required** to specify a feature that describes your i.MX RT chip variant.
You may select only one chip feature.

The crate compiles for the following chips:

- `"imxrt1010"` for i.MX RT **1010** variants
- `"imxrt1060"` for i.MX RT **1060** variants

Each peripheral has it's own feature, which is enabled by default. However, you may
want to disable some peripherals because you have your own interrupt-driven peripheral,
and the interrupt handler that this crate provides causes a duplicate definition

To select peripherals, disable the crate's default features. Then, select one or more of
the peripheral features from the table. The checkmarks indicate a chip's support for
that peripheral.

| **Chip**  | `"gpio"` | `"gpt"` | `"i2c"` | `"pipe"` | `"pit"` | `"spi"` | `"uart"` |
| --------- | -------- | ------- | ------- | -------- | ------- | ------- | -------- |
| imxrt1010 |    ✓     |    ✓    |    ✓    |    ✓     |    ✓    |    ✓    |     ✓    |
| imxrt1060 |    ✓     |    ✓    |    ✓    |    ✓     |    ✓    |    ✓    |     ✓    |

When developing a binary for your embedded system, you should enable this crate's `"rt"`
feature. Otherwise, when developing libraries against the crate, you may skip the
`"rt"` feature.

## Example

Simultaneously blink an LED while echoing all UART data back to the sender.

```rust
use imxrt_async_hal as hal;
use futures::future;
const BAUD: u32 = 115_200;

// Acquire all handles to the processor pads
let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
let mut led = hal::gpio::GPIO::new(pads.b0.p03).output();

// Acquire the clocks that we'll need to enable...
let hal::ccm::CCM {
    mut handle,
    perclock,
    uart_clock,
    ..
} = hal::ral::ccm::CCM::take().map(hal::ccm::CCM::from_ral).unwrap();

let mut gpt = hal::ral::gpt::GPT2::take().unwrap();
// Enable the periodic clock for the GPT
let mut perclock = perclock.enable(&mut handle);
perclock.set_clock_gate_gpt(&mut gpt, hal::ccm::ClockGate::On);
let mut timer = hal::GPT::new(gpt, &perclock);

// Acquire DMA channels, which are used to schedule UART transfers
let mut channels = hal::dma::channels(
    hal::ral::dma0::DMA0::take()
        .map(|mut dma| {
            handle.set_clock_gate_dma(&mut dma, hal::ccm::ClockGate::On);
            dma
        })
        .unwrap(),
    hal::ral::dmamux::DMAMUX::take().unwrap(),
);

// Enable the UART root clock, and prepare the UART2 driver
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
    pads.ad_b1.p02, // TX pad
    pads.ad_b1.p03, // RX pad
    channels[7].take().unwrap(),
    &uart_clock,
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

executor::block_on(future::join(blinking_loop, echo_loop));
```

### License

Licensed under either of

- [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0) ([LICENSE-APACHE](./LICENSE-APACHE))
- [MIT License](http://opensource.org/licenses/MIT) ([LICENSE-MIT](./LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

License: MIT OR Apache-2.0
