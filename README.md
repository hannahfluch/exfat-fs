# exFAT-fs
[![Crates.io Version](https://img.shields.io/crates/v/exfat-fs)](https://crates.io/crates/exfat-fs)

exFAT filesystem formatting in Rust.

## Usage

```rust
use exfat_fs::{
    MB,
    format::{Exfat, FormatVolumeOptionsBuilder, Label},
};

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

let mut formatter = Exfat::try_from(format_options).unwrap();
formatter.write(&mut file).unwrap();
```
