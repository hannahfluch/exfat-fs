use std::{
    ops::{Div, Sub},
    time::{SystemTime, SystemTimeError, UNIX_EPOCH},
};

use bitflags::bitflags;

use crate::error::ExFatError;

use super::{
    BOOT_SIGNATURE, DRIVE_SELECT, FIRST_CLUSTER_INDEX, MAX_CLUSTER_COUNT, MAX_CLUSTER_SIZE,
    UPCASE_TABLE_SIZE_BYTES,
};
/// The Main/Backup Boot Sector structure for an exFAT volume.
/// This structure defines the essential parameters required for the file system.
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
    volume_serial_number: VolumeSerialNumber,

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
    _reserved2: [u8; 7],

    /// The bootstrapping code that is executed if the volume is bootable.
    /// - If not used for booting, should be filled with `0xF4` (Halt instruction).
    boot_code: [u8; 390],

    /// Identifies this sector as a boot sector.
    /// - Must be `0xAA55` to be considered valid.
    boot_signature: u16,
}

impl BootSector {
    /// Creates a new boot sector with a single FAT. All input parameters are given in bytes. (NOT SECTORS!). The offset to the bitmap is also returned.
    pub fn try_new(
        partition_offset: u64,
        bytes_per_sector: u16,
        bytes_per_cluster: u32,
        size: u32,
        boundary_align: u32,
        pack_bitmap: bool,
    ) -> Result<(BootSector, u32), ExFatError> {
        if !bytes_per_sector.is_power_of_two() || !(512..=4096).contains(&bytes_per_sector) {
            return Err(ExFatError::InvalidBytesPerSector(bytes_per_sector));
        }

        // format volume with a single FAT
        let number_of_fats = 1u8;
        let volume_flags = VolumeFlags::empty().bits();

        // transform partition_offset to be measured by sectors
        let partition_offset = partition_offset.div(bytes_per_sector as u64).to_le();

        if !bytes_per_cluster.is_power_of_two()
            || !(bytes_per_sector as u32..=MAX_CLUSTER_SIZE).contains(&bytes_per_cluster)
        {
            return Err(ExFatError::InvlaidClusterSize(bytes_per_cluster));
        }
        let bytes_per_sector_shift = bytes_per_sector.ilog2() as u8;
        let sectors_per_cluster_shift = (bytes_per_cluster / bytes_per_sector as u32).ilog2() as u8;

        let volume_length = (size
            .checked_div(bytes_per_sector.into())
            .and_then(|o| {
                if o < (1 << (20 - bytes_per_sector_shift)) {
                    None
                } else {
                    Some(o)
                }
            })
            .ok_or(ExFatError::InvalidSize(size))? as u64)
            .to_le();

        let fat_offset_bytes: u32 = (bytes_per_sector as u64)
            .checked_mul(24)
            .and_then(|prd| prd.checked_add(partition_offset))
            .ok_or(ExFatError::InvalidPartitionOffset(partition_offset))?
            .next_multiple_of(boundary_align as u64)
            .sub(partition_offset)
            .try_into()
            .map_err(|_| ExFatError::BoundaryAlignemntTooBig(boundary_align))?;

        let fat_offset = fat_offset_bytes.div(bytes_per_sector as u32).to_le();

        let max_clusters: u32 = size
            .checked_sub(fat_offset_bytes)
            .and_then(|d| d.checked_sub(number_of_fats as u32 * 8))
            .and_then(|d| d.checked_sub(1))
            .and_then(|d| d.checked_div(bytes_per_cluster + 4 * number_of_fats as u32))
            .and_then(|q| q.checked_add(1))
            .ok_or(ExFatError::InvlaidClusterSize(bytes_per_cluster))?;

        let fat_length_bytes = max_clusters
            .checked_add(2)
            .and_then(|x| x.checked_mul(4))
            .map(|x| x.next_multiple_of(bytes_per_sector as u32))
            .ok_or(ExFatError::InvlaidClusterSize(bytes_per_cluster))?
            .to_le();

        let fat_length = fat_length_bytes / bytes_per_sector as u32;

        let mut cluster_heap_offset_bytes = ((partition_offset
            + fat_offset_bytes as u64
            + fat_length_bytes as u64 * number_of_fats as u64)
            .next_multiple_of(boundary_align as u64)
            - partition_offset) as u32;

        let mut cluster_heap_offset = cluster_heap_offset_bytes
            .div(bytes_per_sector as u32)
            .to_le();

        if cluster_heap_offset_bytes >= size {
            return Err(ExFatError::BoundaryAlignemntTooBig(boundary_align));
        }
        let mut cluster_count = (size - cluster_heap_offset_bytes)
            .div(bytes_per_cluster)
            .to_le();
        if cluster_count
            > MAX_CLUSTER_COUNT.min(
                ((volume_length - cluster_heap_offset as u64)
                    / 2u64.pow(sectors_per_cluster_shift as u32)) as u32,
            )
        {
            return Err(ExFatError::InvlaidClusterSize(bytes_per_cluster));
        }

        // bitmap is first cluster of cluster heap
        let mut bitmap_offset_bytes = cluster_heap_offset_bytes;
        let mut bitmap_length_bytes = cluster_count.next_multiple_of(8) / 8;

        if pack_bitmap {
            let fat_end_bytes = fat_offset_bytes + fat_length_bytes;
            let mut bitmap_length_bytes_packed;
            let mut bitmap_length_clusters_packed =
                bitmap_length_bytes.next_multiple_of(bytes_per_cluster);

            loop {
                let bitmap_cluster_count_packed = bitmap_length_clusters_packed / bytes_per_cluster;
                // check if there is enough space to put bitmap before alignment boundary
                if cluster_heap_offset_bytes - bitmap_length_clusters_packed < fat_end_bytes
                    || cluster_count > MAX_CLUSTER_COUNT - bitmap_cluster_count_packed
                {
                    return Err(ExFatError::CannotPackBitmap);
                }

                let total_cluster_count = cluster_count + bitmap_cluster_count_packed;
                bitmap_length_bytes_packed = total_cluster_count.next_multiple_of(8).div(8);
                let new_bitmap_length_clusters =
                    bitmap_length_bytes_packed.next_multiple_of(bytes_per_cluster);

                if new_bitmap_length_clusters == bitmap_length_clusters_packed {
                    cluster_heap_offset_bytes -= bitmap_length_clusters_packed;
                    cluster_count = total_cluster_count.to_le();
                    bitmap_offset_bytes -= bitmap_length_clusters_packed;
                    bitmap_length_bytes = bitmap_length_bytes_packed;
                    break;
                }
                bitmap_length_clusters_packed = new_bitmap_length_clusters;
            }

            // reassing changed variable
            cluster_heap_offset = cluster_heap_offset_bytes
                .div(bytes_per_sector as u32)
                .to_le();
        }
        let cluster_length = bitmap_length_bytes.next_multiple_of(bytes_per_cluster);

        let uptable_start_cluster = FIRST_CLUSTER_INDEX as u32 + cluster_length / bytes_per_cluster;
        let uptable_length_bytes = UPCASE_TABLE_SIZE_BYTES;

        let cluster_length = (uptable_length_bytes as u32).next_multiple_of(bytes_per_cluster);

        let first_cluster_of_root_directory =
            (uptable_start_cluster + cluster_length / bytes_per_cluster).to_le();
        let volume_serial_number = VolumeSerialNumber::try_new()?;

        let file_system_revision = FileSystemRevision::default();
        let drive_select = DRIVE_SELECT;
        // empty at beginning
        let percent_in_use = 0;
        let boot_code = [0xF4; 390];
        let boot_signature = BOOT_SIGNATURE.to_le();

        Ok((
            Self {
                jump_boot: [0xeb, 0x76, 0x90],
                filesystem_name: *b"EXFAT   ",
                _reserved: [0; 53],
                partition_offset,
                volume_length,
                bytes_per_sector_shift,
                fat_offset,
                number_of_fats,
                fat_length,
                cluster_heap_offset,
                cluster_count,
                sectors_per_cluster_shift,
                first_cluster_of_root_directory,
                volume_serial_number,
                volume_flags,
                file_system_revision,
                drive_select,
                percent_in_use,
                _reserved2: [0; 7],
                boot_code,
                boot_signature,
            },
            bitmap_offset_bytes,
        ))
    }
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

#[test]
fn test_boot_sector_simple() {
    let size: u32 = 256 * crate::MB as u32;
    let bytes_per_sector = 512;
    let bytes_per_cluster = 4 * crate::KB as u32;

    let (boot_sector, _) = BootSector::try_new(
        0,
        bytes_per_sector,
        bytes_per_cluster,
        size,
        crate::DEFAULT_BOUNDARY_ALIGNEMENT,
        false,
    )
    .unwrap();

    assert_eq!(boot_sector.jump_boot, [0xEB, 0x76, 0x90]);
    assert_eq!(boot_sector.filesystem_name, *b"EXFAT   ");
    assert_eq!(boot_sector.boot_signature, BOOT_SIGNATURE);
    assert_eq!(boot_sector.volume_length, 524288);
    assert_eq!(boot_sector.fat_offset, 2048);
    assert_eq!(boot_sector.fat_length, 510);
    assert_eq!(boot_sector.cluster_heap_offset, 4096);
    assert_eq!(boot_sector.cluster_count, 65024);
    assert_eq!(boot_sector.first_cluster_of_root_directory, 6);
    assert_eq!(boot_sector.bytes_per_sector_shift, 9);
    assert_eq!(boot_sector.sectors_per_cluster_shift, 3);
}

#[test]
fn test_boot_sector_pack_bitmap() {
    let size: u32 = 256 * crate::MB as u32;
    let bytes_per_sector = 512;
    let bytes_per_cluster = 4 * crate::KB as u32;

    let (boot_sector, _) = BootSector::try_new(
        0,
        bytes_per_sector,
        bytes_per_cluster,
        size,
        crate::DEFAULT_BOUNDARY_ALIGNEMENT,
        true,
    )
    .unwrap();

    assert_eq!(boot_sector.jump_boot, [0xEB, 0x76, 0x90]);
    assert_eq!(boot_sector.filesystem_name, *b"EXFAT   ");
    assert_eq!(boot_sector.boot_signature, BOOT_SIGNATURE);
    assert_eq!(boot_sector.volume_length, 524288);
    assert_eq!(boot_sector.fat_offset, 2048);
    assert_eq!(boot_sector.fat_length, 510);
    assert_eq!(boot_sector.cluster_heap_offset, 4080);
    assert_eq!(boot_sector.cluster_count, 65026);
    assert_eq!(boot_sector.first_cluster_of_root_directory, 6);
    assert_eq!(boot_sector.bytes_per_sector_shift, 9);
    assert_eq!(boot_sector.sectors_per_cluster_shift, 3);
}
