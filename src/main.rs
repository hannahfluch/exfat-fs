use boot::sector::BootSector;

const GB: u32 = 1024 * 1024 * 1024;
const MB: u32 = 1024 * 1024;
const KB: u16 = 1024;

const DEFAULT_BOUNDARY_ALIGNEMENT: u32 = 1024 * 1024;

pub mod boot;
pub mod error;

pub struct ExFat;

fn main() {
    let size: u32 = 256 * MB;
    let bytes_per_sector = 512;
    // default cluster size based on sector size
    let cluster_size = if size <= 256 * MB {
        4 * KB
    } else if size <= 32 * GB {
        32 * KB
    } else {
        128 * KB
    } as u32;

    let _boot_sector = BootSector::try_new(
        0,
        bytes_per_sector,
        cluster_size,
        size,
        DEFAULT_BOUNDARY_ALIGNEMENT,
        false,
    );
}
