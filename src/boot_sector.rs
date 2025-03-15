use bitflags::bitflags;
use bytemuck::{Pod, Zeroable};
/// The Main/Backup Boot Sector structure for an exFAT volume.
/// This structure defines the essential parameters required for the file system.
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub(crate) struct BootSector {
    /// The jump instruction for CPUs to execute bootstrapping instructions in `boot_code`.
    /// - Must be `0xEB 0x76 0x90` in order (low-order byte first).
    pub(crate) jump_boot: [u8; 3],

    /// The name of the file system on the volume.
    /// - Must be `"EXFAT   "` (including three trailing spaces).
    pub(crate) filesystem_name: [u8; 8],

    /// Reserved field corresponding to the FAT12/16/32 BIOS Parameter Block.
    /// - Must be all zeroes to prevent misinterpretation by FAT-based systems.
    pub(crate) _reserved: [u8; 53],

    /// The sector offset from the beginning of the media to the partition that contains the exFAT volume.
    /// - A value of `0` indicates that this field should be ignored.
    pub(crate) partition_offset: u64,

    /// The total size of the exFAT volume in sectors.
    /// - Must be at least `2^20 / (2^BytesPerSectorShift)`, ensuring a minimum volume size of 1MB.
    /// - Cannot exceed `2^64 - 1`.
    pub(crate) volume_length: u64,

    /// The sector offset from the start of the volume to the First FAT.
    /// - Minimum value: `24` (accounts for boot sectors).
    /// - Maximum value: `ClusterHeapOffset - (FatLength * NumberOfFats)`.
    pub(crate) fat_offset: u32,

    /// The number of sectors occupied by each FAT.
    /// - Ensures there is enough space for all clusters in the Cluster Heap.
    pub(crate) fat_length: u32,

    /// The sector offset from the start of the volume to the Cluster Heap.
    /// - Defines where the data region (cluster storage) begins.
    pub(crate) cluster_heap_offset: u32,

    /// The number of clusters in the Cluster Heap.
    /// - Determines the minimum size required for a FAT.
    /// - Must be the lesser of `(VolumeLength - ClusterHeapOffset) / 2^SectorsPerClusterShift`
    ///   or `2^32 - 11`.
    pub(crate) cluster_count: u32,

    /// The cluster index of the first cluster in the root directory.
    /// - Must be between `2` (first valid cluster) and `ClusterCount + 1`.
    pub(crate) first_cluster_of_root_directory: u32,

    /// A unique serial number for identifying the volume.
    /// - Typically derived from the date/time of formatting.
    pub(crate) volume_serial_number: VolumeSerialNumber,

    /// The revision number of the exFAT structures on the volume.
    /// - The high byte represents the major version, and the low byte represents the minor version.
    /// - Example: `0x01 0x00` represents version 1.0.
    pub(crate) file_system_revision: FileSystemRevision,

    /// A set of flags that indicate file system status. See [`VolumeFlags`]
    pub(crate) volume_flags: u16,
    /// The sector size in a power-of-two exponent.
    /// - Example: `9` → `2^9 = 512` bytes per sector.
    /// - Valid range: `9` (512 bytes) to `12` (4096 bytes).
    pub(crate) bytes_per_sector_shift: u8,

    /// The number of sectors per cluster in a power-of-two exponent.
    /// - Example: `4` → `2^4 = 16` sectors per cluster.
    /// - Valid range: `0` (1 sector per cluster) to `25 - BytesPerSectorShift`.
    pub(crate) sectors_per_cluster_shift: u8,

    /// The number of File Allocation Tables (FATs) in the volume.
    /// - `1`: Only the First FAT is present.
    /// - `2`: Used in **TexFAT**, which has a Second FAT and a Second Allocation Bitmap.
    pub(crate) number_of_fats: u8,

    /// Extended INT 13h drive number, useful for bootstrapping.
    /// - Typically contains `0x80`.
    pub(crate) drive_select: u8,

    /// The percentage of allocated clusters in the Cluster Heap.
    /// - Values range from `0` to `100` (rounded down).
    /// - `0xFF` means the percentage is unknown.
    pub(crate) percent_in_use: u8,

    /// Reserved for future use. Must be set to zero.
    pub(crate) _reserved2: [u8; 7],

    /// The bootstrapping code that is executed if the volume is bootable.
    /// - If not used for booting, should be filled with `0xF4` (Halt instruction).
    pub(crate) boot_code: [u8; 390],

    /// Identifies this sector as a boot sector.
    /// - Must be `0xAA55` to be considered valid.
    pub(crate) boot_signature: u16,
}

bitflags! {
    /// A set of flags that indicate file system status.
    #[derive(Copy, Clone, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
    pub struct VolumeFlags: u16 {
        /// - **Bit 0**: `ActiveFat` (0 = First FAT, 1 = Second FAT used in TexFAT).
        const ACTIVE_FAT = 1 << 0;
        /// - **Bit 1**: `VolumeDirty` (0 = clean, 1 = dirty).
        const VOLUME_DIRTY = 1 << 1;
        /// - **Bit 2**: `MediaFailure` (0 = no failures, 1 = known media failures).
        const MEDIA_FAILURE = 1 << 2;
        /// - **Bit 3**: `ClearToZero` (should be cleared before modifying file system structures).
        const CLEAR_TO_ZERO = 1 << 3;
    }
}

use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

/// Structure representing the file system revision.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct FileSystemRevision {
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
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub(crate) struct VolumeSerialNumber(u32);

impl VolumeSerialNumber {
    pub(crate) fn try_new() -> Result<VolumeSerialNumber, SystemTimeError> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        Ok(VolumeSerialNumber((now.as_secs() as u32).to_le()))
    }
}
