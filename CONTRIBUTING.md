# Contributing Guide

Thanks for contributing to the `imxrt-async-hal` project! Please open an issue
when

- you find a bug in the crate
- you have an idea for a feature
- something isn't clear in our documentation

The rest of this guide provides quick tips for working with these packages.
Before you get started, make sure that you have the dependencies described
in the [README](./README.md).

## Running tests

Unit and documentation tests run on your host computer. You must specify
the `imxrt106x` chip variant for documentation tests,

```
cargo test --features imxrt106x
```

## Update the README

We generate the README from the top-level library documentation. We can feel
confident that the examples in the README will compile.

```
cargo install cargo-readme
cargo readme > README.md
```

## Generate docs

Use the typical `cargo doc` to locally browse API docs. You must specify
a chip variant:

```
cargo doc --features imxrt106x [--open]
```

To generate the documentation that includes feature hints -- the docs that
docs.rs generates -- install a nightly compiler. Then, run

```
cargo +nightly rustdoc --features imxrt106x [--open] -- --cfg docsrs
```

## Resources

- [*The Async Rust Book*] teaches you the basics of async programming in Rust.
  However, it's not specific to embedded systems.

- [*The Embedded Rust Book*] to learn about embedded Rust development. However,
  it's not specific to async Rust.

- i.MX RT reference manuals are available from NXP. The reference manuals 
  describe the i.MX RT registers and peripheral capabilities. Go
  [here][imxrt-series], and select your processor. Then, go to
  "Documentation," and scroll down to "Reference Manual." You'll need a free
  NXP account to access the reference manuals.

- i.MX RT data sheets are available as free downloads [here][imxrt-series].
  The data sheets are useful for understanding high-level capabilities of the
  i.MX RT processors. Select your processor, then go to "Documentation," then
  "Data Sheet."

- For other code references, consider studying
  - the [Zephyr Project](https://www.zephyrproject.org/).
  - the ARM CMSIS Packs. Here's the [MIMXRT1062 pack][cmsis-pack]; NXP and ARM
    also provide CMSIS packs for the other i.MX RT variants.
  - NXP's MCUXpresso SDK, available [here][nxp-sdk].

[*The Async Rust Book*]: https://rust-lang.github.io/async-book/
[*The Embedded Rust Book*]: https://rust-embedded.github.io/book/intro/index.html

[imxrt-series]: https://www.nxp.com/products/processors-and-microcontrollers/arm-microcontrollers/i-mx-rt-crossover-mcus:IMX-RT-SERIES
[cmsis-pack]: https://developer.arm.com/embedded/cmsis/cmsis-packs/devices/NXP/MIMXRT1062XXXXA
[nxp-sdk]: https://www.nxp.com/design/software/development-software/mcuxpresso-software-and-tools/mcuxpresso-software-development-kit-sdk:MCUXpresso-SDK