# imxrt-async-hal Teensy 4 examples

The programs in this package demonstrate the `imxrt-async-hal` on the Teensy
4. The examples work on both the Teensy 4.0 and 4.1 boards. The examples might
require other hardware or electronics; see the top-level documentation of each
example for more details.

## Dependencies

You'll need all of the build dependencies for the `imxrt-async-hal`. See the
project documentation for more details.

You'll also need

- A capable `objcopy` for transforming Rust binaries into hex files. The
documentation and tooling in the guide uses the LLVM `objcopy` provided by
[`cargo binutils`]. Install [`cargo binutils`] if you want to precisely follow
this documentation.

[`cargo binutils`]: https://github.com/rust-embedded/cargo-binutils

- To download programs to your Teensy 4, you'll need either a build of
[`teensy_loader_cli`](https://github.com/PaulStoffregen/teensy_loader_cli), or
the [Teensy Loader Application](https://www.pjrc.com/teensy/loader.html). The
latter is available with the Teensyduino add-ons.

Note the `.cargo/config` configuration, which specifies the linker script.
If you'd like to re-create this kind of package for your own project, you'll
need the linker script, available at [`t4link.x`](./t4link.x), or an
equivalent memory map for your runtime.

## Building Examples

From this directory, use `cargo objcopy` to build a release binary, and output
a hex file:

```
cargo objcopy --target thumbv7em-none-eabihf --release --bin gpt -- -O ihex gpt.hex
```

Flash the hex file to your Teensy 4!