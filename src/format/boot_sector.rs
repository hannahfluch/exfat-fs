use std::io::{self, Seek, SeekFrom, Write};

use bytemuck::{Pod, Zeroable, bytes_of, cast_slice};

use crate::disk;

use super::{
    Exfat,
    util::{
        BOOT_SIGNATURE, DRIVE_SELECT, EXTENDED_BOOT, EXTENDED_BOOT_SIGNATURE, FileSystemRevision,
        VolumeSerialNumber,
    },
};
/// The Main/Backup Boot Sector structure for an exFAT volume.
/// This structure defines the essential parameters required for the file system.
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub(super) struct BootSector {
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
    pub(super) fn new(meta: &Exfat) -> BootSector {
        Self {
            jump_boot: [0xeb, 0x76, 0x90],
            filesystem_name: *b"EXFAT   ",
            _reserved: [0; 53],
            partition_offset: meta.format_options.partition_offset.to_le(),
            volume_length: meta.volume_length.to_le(),
            bytes_per_sector_shift: meta.bytes_per_sector_shift,
            fat_offset: meta.fat_offset.to_le(),
            number_of_fats: meta.number_of_fats,
            fat_length: meta.fat_length.to_le(),
            cluster_heap_offset: meta.cluster_heap_offset.to_le(),
            cluster_count: meta.cluster_count.to_le(),
            sectors_per_cluster_shift: meta.sectors_per_cluster_shift,
            first_cluster_of_root_directory: meta.first_cluster_of_root_directory.to_le(),
            volume_serial_number: meta.volume_serial_number,
            volume_flags: meta.volume_flags.to_le(),
            file_system_revision: meta.file_system_revision,
            drive_select: DRIVE_SELECT,
            percent_in_use: 0xFF, // not currently supported
            _reserved2: [0; 7],
            boot_code: [0xF4; 390],
            boot_signature: BOOT_SIGNATURE,
        }
    }
}
#[derive(Copy, Clone, Debug)]
pub(super) struct Checksum {
    inner: u32,
    sector_size_in_bytes: u16,
}

impl Checksum {
    pub(super) fn new(sector_size_in_bytes: u16) -> Checksum {
        Self {
            inner: 0,
            sector_size_in_bytes,
        }
    }
}

impl Checksum {
    /// Updates the checksum according to one entirely empty sector.
    pub(super) fn zero_sector(&mut self) {
        for _ in 0..self.sector_size_in_bytes {
            self.inner = (self.inner & 1) * 0x80000000 + (self.inner >> 1);
        }
    }

    /// Updates the checksum according to a boot sector.
    pub(super) fn boot_sector(&mut self, sector: &[u8]) {
        assert_eq!(sector.len(), self.sector_size_in_bytes as usize);
        for i in 0..self.sector_size_in_bytes {
            if i == 106 || i == 107 || i == 112 {
                continue;
            }

            self.inner =
                (self.inner & 1) * 0x80000000 + (self.inner >> 1) + sector[i as usize] as u32;
        }
    }

    /// Updates the checksum according to a set of extended boot sectors.
    pub(super) fn extended_boot_sector(&mut self, sector: &[u8], amount: u64) {
        assert_eq!(sector.len(), self.sector_size_in_bytes as usize);
        for _ in 0..amount {
            for i in 0..self.sector_size_in_bytes {
                self.inner =
                    (self.inner & 1) * 0x80000000 + (self.inner >> 1) + sector[i as usize] as u32;
            }
        }
    }

    /// Returns a copy of the current state of the checksum in little-endian format.
    pub(super) fn get(&self) -> u32 {
        self.inner.to_le()
    }
}

impl Exfat {
    /// Attempts to write a boot region to a disk at the specified sector offet.
    pub(super) fn write_boot_region<T: Write + Seek>(
        &self,
        f: &mut T,
        mut offset_sectors: u64,
    ) -> io::Result<()> {
        let mut checksum = Checksum::new(self.format_options.bytes_per_sector);

        let boot_sector = BootSector::new(self);

        // write boot sector
        let bytes = bytes_of(&boot_sector);
        self.write_sector(f, bytes, offset_sectors)?;
        checksum.boot_sector(bytes);
        offset_sectors += 1;

        // write extended boot sectors
        let bytes = self.write_extended(f, offset_sectors, EXTENDED_BOOT)?;
        checksum.extended_boot_sector(cast_slice(&bytes), EXTENDED_BOOT);
        offset_sectors += EXTENDED_BOOT;

        // write oem sector (unused so entirely empty)
        // todo: add flash/custom parameter support
        disk::write_zeroes(
            f,
            self.format_options.bytes_per_sector as u64,
            self.offset_sector_bytes(offset_sectors),
        )?;
        checksum.zero_sector();
        offset_sectors += 1;

        // write reserved sector
        disk::write_zeroes(
            f,
            self.format_options.bytes_per_sector as u64,
            self.offset_sector_bytes(offset_sectors),
        )?;
        checksum.zero_sector();
        offset_sectors += 1;

        // checksum sector
        self.write_checksum(f, checksum, offset_sectors)?;

        Ok(())
    }

    /// Attempts to write a single sector at the specified offset (given in sectors).
    fn write_sector<T: Write + Seek>(
        &self,
        f: &mut T,
        bytes: &[u8],
        offset_sectors: u64,
    ) -> io::Result<()> {
        f.seek(SeekFrom::Start(self.offset_sector_bytes(offset_sectors)))?;
        f.write_all(bytes)
    }

    /// Attempts to write a given amount of extended boot sectors at the specified offset (given in
    /// sectors). Returns the buffer of the extended boot sector.
    fn write_extended<T: Write + Seek>(
        &self,
        f: &mut T,
        offset_sectors: u64,
        amount: u64,
    ) -> io::Result<Vec<u32>> {
        f.seek(SeekFrom::Start(self.offset_sector_bytes(offset_sectors)))?;

        let buffer_len = self.format_options.bytes_per_sector as usize / 4;
        let mut buffer = vec![0; buffer_len];

        buffer[buffer_len - 1] = EXTENDED_BOOT_SIGNATURE.to_le();

        for i in 0..amount {
            let sector_offset = offset_sectors + i;
            self.write_sector(f, cast_slice(&buffer), sector_offset)?;
        }

        Ok(buffer)
    }

    /// Attempts to write the checksum sector
    fn write_checksum<T: Write + Seek>(
        &self,
        f: &mut T,
        checksum: Checksum,
        offset_sectors: u64,
    ) -> io::Result<()> {
        f.seek(SeekFrom::Start(self.offset_sector_bytes(offset_sectors)))?;

        let checksum = checksum.get();

        let buffer_len = self.format_options.bytes_per_sector as usize / 4;
        let mut buffer = vec![0u32; buffer_len];

        for i in buffer.iter_mut() {
            *i = checksum;
        }

        self.write_sector(f, cast_slice(&buffer), offset_sectors)?;

        Ok(())
    }

    /// Offset in bytes until the given sector index.
    fn offset_sector_bytes(&self, sector_index: u64) -> u64 {
        self.format_options.bytes_per_sector as u64 * sector_index
    }
}

#[test]
fn small_simple() {
    use crate::format::FormatVolumeOptionsBuilder;
    let size: u64 = 256 * crate::MB as u64;

    let format_options = FormatVolumeOptionsBuilder::default()
        .pack_bitmap(false)
        .full_format(false)
        .partition_offset(0)
        .boundary_align(crate::DEFAULT_BOUNDARY_ALIGNEMENT)
        .dev_size(size)
        .bytes_per_sector(512)
        .build()
        .unwrap();

    let exfat = Exfat::try_from(format_options).unwrap();

    let boot_sector = BootSector::new(&exfat);

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
fn small_pack_bitmap() {
    use crate::format::Exfat;
    use crate::format::FormatVolumeOptionsBuilder;
    let size: u64 = 256 * crate::MB as u64;

    let format_options = FormatVolumeOptionsBuilder::default()
        .pack_bitmap(true)
        .full_format(false)
        .partition_offset(0)
        .boundary_align(crate::DEFAULT_BOUNDARY_ALIGNEMENT)
        .dev_size(size)
        .bytes_per_sector(512)
        .build()
        .unwrap();

    let meta = Exfat::try_from(format_options).unwrap();

    let boot_sector = BootSector::new(&meta);

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

#[test]
fn big_simple() {
    use crate::format::FormatVolumeOptionsBuilder;
    let size: u64 = 5 * crate::GB as u64;

    let format_options = FormatVolumeOptionsBuilder::default()
        .pack_bitmap(false)
        .full_format(false)
        .partition_offset(0)
        .boundary_align(crate::DEFAULT_BOUNDARY_ALIGNEMENT)
        .dev_size(size)
        .bytes_per_sector(512)
        .build()
        .unwrap();

    let meta = Exfat::try_from(format_options).unwrap();

    let boot_sector = BootSector::new(&meta);
    assert_eq!(boot_sector.jump_boot, [0xEB, 0x76, 0x90]);
    assert_eq!(boot_sector.filesystem_name, *b"EXFAT   ");
    assert_eq!(boot_sector.boot_signature, BOOT_SIGNATURE);
    assert_eq!(boot_sector.volume_length, 10485760);
    assert_eq!(boot_sector.fat_offset, 2048);
    assert_eq!(boot_sector.fat_length, 1280);
    assert_eq!(boot_sector.cluster_heap_offset, 4096);
    assert_eq!(boot_sector.cluster_count, 163776);
    assert_eq!(boot_sector.first_cluster_of_root_directory, 4);
    assert_eq!(boot_sector.bytes_per_sector_shift, 9);
    assert_eq!(boot_sector.sectors_per_cluster_shift, 6);
}

#[test]
fn boot_region() {
    use super::FormatVolumeOptionsBuilder;
    use std::io::Read;

    let size: u64 = 32 * crate::MB as u64;
    let bytes_per_sector = 512;

    let format_options = FormatVolumeOptionsBuilder::default()
        .pack_bitmap(false)
        .full_format(false)
        .partition_offset(0)
        .boundary_align(crate::DEFAULT_BOUNDARY_ALIGNEMENT)
        .dev_size(size)
        .bytes_per_sector(bytes_per_sector)
        .build()
        .unwrap();

    let mut formatter = Exfat::try_from(format_options).unwrap();

    let mut f = std::io::Cursor::new(vec![0u8; size as usize]);

    formatter.write(&mut f).unwrap();

    let offset_main_checksum_bytes = 11 * bytes_per_sector as u64;
    let offset_backup_checksum_bytes = 23 * bytes_per_sector as u64;

    // assert checksum is the same for main boot region and backup boot region
    let mut read_main = vec![0u8; 8];
    f.seek(std::io::SeekFrom::Start(offset_main_checksum_bytes))
        .unwrap();
    f.read_exact(&mut read_main).unwrap();

    let mut read_backup = vec![0u8; 8];

    f.seek(std::io::SeekFrom::Start(offset_backup_checksum_bytes))
        .unwrap();
    f.read_exact(&mut read_backup).unwrap();

    assert_eq!(
        read_backup, read_main,
        "checksum of main and backup boot region must be equal"
    );
}
