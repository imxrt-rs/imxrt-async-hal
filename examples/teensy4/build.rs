use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let link_x = include_bytes!("t4link.x");
    let mut link_x_file = File::create(out_dir.join("t4link.x")).unwrap();
    link_x_file.write_all(link_x).unwrap();

    println!("cargo:rustc-link-search={}", out_dir.display());
}
