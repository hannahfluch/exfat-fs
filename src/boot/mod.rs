// boot regions

use crate::MB;

/// Offset for main boot sector
pub const MAIN_BOOT_OFFSET: usize = 0;
/// Offset for the main extended boot sectors
pub const MAIN_EXTENDED_BOOT_OFFSET: usize = 1;
/// Maximum amount of clusters
pub const MAX_CLUSTER_COUNT: u32 = 0xFFFFFFF5;
/// Maximux size of clusters
pub const MAX_CLUSTER_SIZE: u32 = 32 * MB;

pub const SECTOR_SIZE: u64 = 0x1000;
pub const BOUNDARY_ALIGN: u64 = 1024 * 1024;

pub const FIRST_CLUSTER_INDEX: u8 = 2;
pub const UPCASE_TABLE_SIZE_BYTES: u16 = 5836;
pub const DRIVE_SELECT: u8 = 0x80;
pub const BOOT_SIGNATURE: u16 = 0xAA55;

pub mod sector;
