use std::fs::OpenOptions;

use exfat::{DEFAULT_BOUNDARY_ALIGNEMENT, GB, KB, MB};
use format::{FormatOptions, Formatter, Label};

pub mod dir;
pub mod disk;
pub mod error;
pub mod format;

pub struct ExFat;

fn main() {
    let size: u64 = 32 * MB as u64;
    let bytes_per_sector = 512;

    let mut formatter = Formatter::try_new(
        0,
        bytes_per_sector,
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
