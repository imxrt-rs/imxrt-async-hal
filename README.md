# imxrt-async-hal

Embedded, async Rust for i.MX RT processors

#### [API Docs (main branch)][main-api-docs]

[main-api-docs]: https://imxrt-rs.github.io/imxrt-async-hal/

`imxrt-async-hal` brings async Rust support to NXP's i.MX RT processors.
The crate includes `await`able peripherals and timers. Once the I/O completes
or the timer elapses, an interrupt fires to wake the executor. By combining
interrupt-driven peripherals with a single-threaded executor, you can write
multiple, concurrent tasks for your embedded system.

See the [API docs][main-api-docs] for build dependencies, features, and
examples. To try examples on actual hardware, see the
[`examples` directory](./examples).

### License

Licensed under either of

- [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0) ([LICENSE-APACHE](./LICENSE-APACHE))
- [MIT License](http://opensource.org/licenses/MIT) ([LICENSE-MIT](./LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
