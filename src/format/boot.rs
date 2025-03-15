use std::io::{self, Seek, SeekFrom, Write};

use bytemuck::{bytes_of, cast_slice};

use crate::{MB, boot_sector::BootSector, disk};

use super::Exfat;

/// Offset for main boot region (in sectors)
pub(super) const MAIN_BOOT_OFFSET: u64 = 0;
/// Offset to backup boot region (in sectors)
pub(super) const BACKUP_BOOT_OFFSET: u64 = 12;
/// Maximum amount of clusters
pub(super) const MAX_CLUSTER_COUNT: u32 = 0xFFFFFFF5;
/// Maximux size of clusters
pub(super) const MAX_CLUSTER_SIZE: u32 = 32 * MB;
pub(super) const DRIVE_SELECT: u8 = 0x80;
/// Signature of regular boot sector
pub(super) const BOOT_SIGNATURE: u16 = 0xAA55;
/// Singature of extended boot sector
pub(super) const EXTENDED_BOOT_SIGNATURE: u32 = 0xAA550000;

/// Number of extended boot sectors per boot region
pub(super) const EXTENDED_BOOT: u64 = 8;

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
            volume_flags: meta.volume_flags.bits().to_le(),
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
