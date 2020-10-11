use std::env;

fn main() {
    let dma = [
        "CARGO_FEATURE_PIPE",
        "CARGO_FEATURE_SPI",
        "CARGO_FEATURE_UART",
    ]
    .iter()
    .map(env::var)
    .any(|var| var.is_ok());

    if dma {
        println!("cargo:rustc-cfg=dma")
    }
}
