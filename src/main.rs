use std::fs::OpenOptions;

use exfat_fs::{
    Label, MB,
    dir::Root,
    format::{Exfat, FormatVolumeOptionsBuilder},
};

fn main() {
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

    let mut file = OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .truncate(false)
        .open("test")
        .unwrap();

    formatter.write(&mut file).unwrap();
    println!("done");

    let _root = Root::open(file);
}
