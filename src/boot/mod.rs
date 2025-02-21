// boot regions

use bitflags::bitflags;

use crate::MB;
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

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

pub mod meta;
pub mod sector;

/// Structure representing the file system revision.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FileSystemRevision {
    /// Minor version of the exFAT file system (low-order byte).
    vermin: u8,
    /// Major version of the exFAT file system (high-order byte).
    vermaj: u8,
}
impl Default for FileSystemRevision {
    fn default() -> Self {
        Self {
            vermin: 0,
            vermaj: 1,
        }
    }
}

/// Structure representing the unique volume serial number.
#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct VolumeSerialNumber(u32);

impl VolumeSerialNumber {
    pub fn try_new() -> Result<VolumeSerialNumber, SystemTimeError> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        Ok(VolumeSerialNumber((now.as_secs() as u32).to_le()))
    }
}

bitflags! {

    #[derive(Copy, Clone, Debug)]
    #[repr(C)]
    pub struct VolumeFlags: u16 {
        /// The ActiveFat field shall describe which FAT and Allocation Bitmap are active (and implementations shall use), as follows:
        /// - 0, which means the First FAT and First Allocation Bitmap are active
        /// - 1, which means the Second FAT and Second Allocation Bitmap are active and is possible only when the NumberOfFats field contains the value 2
        const ACTIVE_FAT = 1;
        /// The VolumeDirty field shall describe whether the volume is dirty or not, as follows:
        /// - 0, which means the volume is probably in a consistent state
        /// - 1, which means the volume is probably in an inconsistent state
        const DIRTY = 1 << 1;
        /// The MediaFailure field shall describe whether an implementation has discovered media failures or not, as follows:
        /// - 0, which means the hosting media has not reported failures or any known failures are already recorded in the FAT as "bad" clusters
        /// - 1, which means the hosting media has reported failures (i.e. has failed read or write operations)
        const MEDIA_FAILURE = 1 << 2;
        // remaininig bits are reserved
    }
}
