# exFAT-fs
[![Crates.io Version](https://img.shields.io/crates/v/exfat-fs)](https://crates.io/crates/exfat-fs)

exFAT filesystem formatting in Rust.

## Features
- exFAT formatting
- `no-std` support

## Usage

```rust
use exfat_fs::{
    MB,
    Label,
    format::{Exfat, FormatVolumeOptionsBuilder},
};

use std::{io::Cursor, time::SystemTime};

let size: u64 = 32 * MB as u64;
let hello_label = Label::new("Hello".to_string()).unwrap();

let format_options = FormatVolumeOptionsBuilder::default()
    .pack_bitmap(false)
    .full_format(false)
    .dev_size(size)
    .label(hello_label)
    .bytes_per_sector(512)
    .build()
    .unwrap();

let mut formatter = Exfat::try_from::<SystemTime>(format_options).unwrap();

let mut file = Cursor::new(vec![0u8; size as usize]);

formatter.write::<SystemTime, Cursor<Vec<u8>>>(&mut file).unwrap();
```
