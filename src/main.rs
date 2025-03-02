use std::fs::OpenOptions;

use format::{FormatOptions, Formatter, Label};

const GB: u32 = 1024 * 1024 * 1024;
const MB: u32 = 1024 * 1024;
const KB: u16 = 1024;

const DEFAULT_BOUNDARY_ALIGNEMENT: u32 = 1024 * 1024;

pub mod dir;
pub mod disk;
pub mod error;
pub mod format;

pub struct ExFat;

fn main() {
    let size: u64 = 32 * MB as u64;
    let bytes_per_sector = 512;
    // default cluster size based on sector size
    let cluster_size = if size <= 256 * MB as u64 {
        4 * KB
    } else if size <= 32 * GB as u64 {
        32 * KB
    } else {
        128 * KB
    } as u32;

    let mut formatter = Formatter::try_new(
        0,
        bytes_per_sector,
        cluster_size,
        size,
        DEFAULT_BOUNDARY_ALIGNEMENT,
        FormatOptions::new(false, false, size, Label::new("Hello".to_string()).unwrap()),
    )
    .unwrap();

    let mut file = OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .truncate(false)
        .open("test")
        .unwrap();

    formatter.write(&mut file).unwrap();
    println!("done");
}
