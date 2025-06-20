use exfat_fs::root::Root;
use std::fs::OpenOptions;
fn main() {
    // let size: u64 = 32 * MB as u64;
    // let hello_label = Label::new("Hello".to_string()).unwrap();

    // let format_options = FormatVolumeOptionsBuilder::default()
    //     .pack_bitmap(false)
    //     .full_format(false)
    //     .dev_size(size)
    //     .label(hello_label)
    //     .bytes_per_sector(512)
    //     .build()
    //     .unwrap();

    // let formatter = Exfat::try_from(format_options).unwrap();

    let file = OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .truncate(false)
        .open("test")
        .unwrap();

    //    formatter.write(&mut file).unwrap();

    let mut root = Root::open(file).unwrap();
    let len = root.items().len();
    println!(
        "Root directory parsed! Volume Label: `{}`, Number of items: `{}`",
        root.label().unwrap(),
        len
    );
}
