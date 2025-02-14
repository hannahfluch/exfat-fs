/// The Main/Backup Boot Sector structure for an exFAT volume.
/// This structure defines the essential parameters required for the file system.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct BootSector {
    /// The jump instruction for CPUs to execute bootstrapping instructions in `boot_code`.
    /// - Must be `0xEB 0x76 0x90` in order (low-order byte first).
    jump_boot: [u8; 3],

    /// The name of the file system on the volume.
    /// - Must be `"EXFAT   "` (including three trailing spaces).
    filesystem_name: [u8; 8],

    /// Reserved field corresponding to the FAT12/16/32 BIOS Parameter Block.
    /// - Must be all zeroes to prevent misinterpretation by FAT-based systems.
    _reserved: [u8; 53],

    /// The sector offset from the beginning of the media to the partition that contains the exFAT volume.
    /// - A value of `0` indicates that this field should be ignored.
    partition_offset: u64,

    /// The total size of the exFAT volume in sectors.
    /// - Must be at least `2^20 / (2^BytesPerSectorShift)`, ensuring a minimum volume size of 1MB.
    /// - Cannot exceed `2^64 - 1`.
    volume_length: u64,

    /// The sector offset from the start of the volume to the First FAT.
    /// - Minimum value: `24` (accounts for boot sectors).
    /// - Maximum value: `ClusterHeapOffset - (FatLength * NumberOfFats)`.
    fat_offset: u32,

    /// The number of sectors occupied by each FAT.
    /// - Ensures there is enough space for all clusters in the Cluster Heap.
    fat_length: u32,

    /// The sector offset from the start of the volume to the Cluster Heap.
    /// - Defines where the data region (cluster storage) begins.
    cluster_heap_offset: u32,

    /// The number of clusters in the Cluster Heap.
    /// - Determines the minimum size required for a FAT.
    /// - Must be the lesser of `(VolumeLength - ClusterHeapOffset) / 2^SectorsPerClusterShift`
    ///   or `2^32 - 11`.
    cluster_count: u32,

    /// The cluster index of the first cluster in the root directory.
    /// - Must be between `2` (first valid cluster) and `ClusterCount + 1`.
    first_cluster_of_root_directory: u32,

    /// A unique serial number for identifying the volume.
    /// - Typically derived from the date/time of formatting.
    volume_serial_number: [u8; 4],

    /// The revision number of the exFAT structures on the volume.
    /// - The high byte represents the major version, and the low byte represents the minor version.
    /// - Example: `0x01 0x00` represents version 1.0.
    file_system_revision: FileSystemRevision,

    /// A set of flags that indicate file system status.
    /// - **Bit 0**: `ActiveFat` (0 = First FAT, 1 = Second FAT used in TexFAT).
    /// - **Bit 1**: `VolumeDirty` (0 = clean, 1 = dirty).
    /// - **Bit 2**: `MediaFailure` (0 = no failures, 1 = known media failures).
    /// - **Bit 3**: `ClearToZero` (should be cleared before modifying file system structures).
    volume_flags: u16,

    /// The sector size in a power-of-two exponent.
    /// - Example: `9` → `2^9 = 512` bytes per sector.
    /// - Valid range: `9` (512 bytes) to `12` (4096 bytes).
    bytes_per_sector_shift: u8,

    /// The number of sectors per cluster in a power-of-two exponent.
    /// - Example: `4` → `2^4 = 16` sectors per cluster.
    /// - Valid range: `0` (1 sector per cluster) to `25 - BytesPerSectorShift`.
    sectors_per_cluster_shift: u8,

    /// The number of File Allocation Tables (FATs) in the volume.
    /// - `1`: Only the First FAT is present.
    /// - `2`: Used in **TexFAT**, which has a Second FAT and a Second Allocation Bitmap.
    number_of_fats: u8,

    /// Extended INT 13h drive number, useful for bootstrapping.
    /// - Typically contains `0x80`.
    drive_select: u8,

    /// The percentage of allocated clusters in the Cluster Heap.
    /// - Values range from `0` to `100` (rounded down).
    /// - `0xFF` means the percentage is unknown.
    percent_in_use: u8,

    /// Reserved for future use. Must be set to zero.
    reserved: [u8; 7],

    /// The bootstrapping code that is executed if the volume is bootable.
    /// - If not used for booting, should be filled with `0xF4` (Halt instruction).
    boot_code: [u8; 390],

    /// Identifies this sector as a boot sector.
    /// - Must be `0xAA55` to be considered valid.
    boot_signature: u16,
}

/// Structure representing the file system revision.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct FileSystemRevision {
    /// Minor version of the exFAT file system (low-order byte).
    vermin: u8,
    /// Major version of the exFAT file system (high-order byte).
    vermaj: u8,
}
