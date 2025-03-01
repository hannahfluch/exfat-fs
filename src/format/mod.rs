use std::{
    io::{self, Seek, SeekFrom, Write},
    ops::{Div, Sub},
};

use bytemuck::cast_slice;
use checked_num::CheckedU64;
use util::{
    DirEntry, FileSystemRevision, VolumeSerialNumber, BACKUP_BOOT_OFFSET,
    FIRST_USABLE_CLUSTER_INDEX, MAIN_BOOT_OFFSET, MAX_CLUSTER_COUNT, MAX_CLUSTER_SIZE,
    UPCASE_TABLE_SIZE_BYTES,
};

use crate::{disk, error::ExFatError};

pub mod boot_sector;
pub mod fat;
pub mod upcase_table;
pub mod util;

#[derive(Copy, Clone, Debug)]
pub struct FormatOptions {
    pub pack_bitmap: bool,
    pub full_format: bool,
    /// Size of the target device (in bytes)
    pub dev_size: u64,
}

impl FormatOptions {
    pub fn new(pack_bitmap: bool, full_format: bool, dev_size: u64) -> FormatOptions {
        Self {
            pack_bitmap,
            full_format,
            dev_size,
        }
    }
}
#[derive(Copy, Clone, Debug)]
pub struct Formatter {
    pub(super) partition_offset: u64,
    pub(super) volume_length: u64,
    pub(super) fat_offset: u32,
    pub(super) fat_length: u32,
    pub(super) cluster_heap_offset: u32,
    pub(super) cluster_count: u32,
    pub(super) cluster_count_used: u32,
    pub(super) first_cluster_of_root_directory: u32,
    pub(super) file_system_revision: FileSystemRevision,
    pub(super) volume_flags: u16,
    pub(super) bytes_per_sector_shift: u8,
    pub(super) sectors_per_cluster_shift: u8,
    pub(super) number_of_fats: u8,
    pub(super) uptable_length_bytes: u32,
    pub(super) bitmap_length_bytes: u32,
    pub(super) bitmap_offset_bytes: u32,
    pub(super) bytes_per_sector: u16,
    pub(super) bytes_per_cluster: u32,
    pub(super) size: u64,
    pub(super) volume_serial_number: VolumeSerialNumber,
    pub(super) root_offset_bytes: u32,
    pub(super) format_options: FormatOptions,
    pub(super) root_length_bytes: u32,
    pub(super) uptable_offset_bytes: u32,
}

impl Formatter {
    pub fn try_new(
        partition_offset: u64,
        bytes_per_sector: u16,
        bytes_per_cluster: u32,
        size: u64,
        boundary_align: u32,
        format_options: FormatOptions,
    ) -> Result<Formatter, ExFatError> {
        if format_options.dev_size < size {
            return Err(ExFatError::InvalidFileSize);
        }

        if !bytes_per_sector.is_power_of_two() || !(512..=4096).contains(&bytes_per_sector) {
            return Err(ExFatError::InvalidBytesPerSector(bytes_per_sector));
        }

        // format volume with a single FAT
        let number_of_fats = 1u8;
        let volume_flags = 0;

        // transform partition_offset to be measured by sectors
        let partition_offset = partition_offset / bytes_per_sector as u64;

        if !bytes_per_cluster.is_power_of_two()
            || !(bytes_per_sector as u32..=MAX_CLUSTER_SIZE).contains(&bytes_per_cluster)
        {
            return Err(ExFatError::InvlaidClusterSize(bytes_per_cluster));
        }
        let bytes_per_sector_shift = bytes_per_sector.ilog2() as u8;
        let sectors_per_cluster_shift = (bytes_per_cluster / bytes_per_sector as u32).ilog2() as u8;

        let volume_length = size / bytes_per_sector as u64;

        if volume_length < (1 << (20 - bytes_per_sector_shift)) {
            return Err(ExFatError::InvalidSize(size));
        }

        let fat_offset_bytes: u32 = (CheckedU64::new(bytes_per_sector as u64) * 24
            + partition_offset)
            .ok_or(ExFatError::InvalidPartitionOffset(partition_offset))?
            .next_multiple_of(boundary_align as u64)
            .sub(partition_offset)
            .try_into()
            .map_err(|_| ExFatError::BoundaryAlignemntTooBig(boundary_align))?;

        let fat_offset = fat_offset_bytes / bytes_per_sector as u32;

        let max_clusters: CheckedU64 =
            ((CheckedU64::new(size) - fat_offset_bytes as u64 - number_of_fats as u64 * 8 - 1)
                / (bytes_per_cluster as u64 + 4 * number_of_fats as u64)
                + 1)
            .ok_or(ExFatError::InvlaidClusterSize(bytes_per_cluster))?
            .into();

        let fat_length_bytes = ((max_clusters + 2) * 4)
            .ok_or(ExFatError::InvlaidClusterSize(bytes_per_cluster))?
            .next_multiple_of(bytes_per_sector as u64);

        let fat_length: u32 = (fat_length_bytes / bytes_per_sector as u64)
            .try_into()
            .map_err(|_| ExFatError::InvlaidClusterSize(bytes_per_cluster))?;

        let mut cluster_heap_offset_bytes = ((partition_offset
            + fat_offset_bytes as u64
            + fat_length_bytes * number_of_fats as u64)
            .next_multiple_of(boundary_align as u64)
            - partition_offset) as u32;

        let mut cluster_heap_offset = cluster_heap_offset_bytes / bytes_per_sector as u32;

        if cluster_heap_offset_bytes as u64 >= size {
            return Err(ExFatError::BoundaryAlignemntTooBig(boundary_align));
        }

        let mut cluster_count: u32 = ((size - cluster_heap_offset_bytes as u64)
            / bytes_per_cluster as u64)
            .try_into()
            .map_err(|_| ExFatError::InvlaidClusterSize(bytes_per_cluster))?;

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

        if format_options.pack_bitmap {
            let fat_end_bytes = fat_offset_bytes as u64 + fat_length_bytes;
            let mut bitmap_length_bytes_packed;
            let mut bitmap_length_clusters_packed =
                bitmap_length_bytes.next_multiple_of(bytes_per_cluster);

            loop {
                let bitmap_cluster_count_packed = bitmap_length_clusters_packed / bytes_per_cluster;
                // check if there is enough space to put bitmap before alignment boundary
                if ((cluster_heap_offset_bytes - bitmap_length_clusters_packed) as u64)
                    < fat_end_bytes
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
                    cluster_count = total_cluster_count;
                    bitmap_offset_bytes -= bitmap_length_clusters_packed;
                    bitmap_length_bytes = bitmap_length_bytes_packed;
                    break;
                }
                bitmap_length_clusters_packed = new_bitmap_length_clusters;
            }

            // reassing changed variable
            cluster_heap_offset = cluster_heap_offset_bytes / bytes_per_sector as u32;
        }
        let cluster_length = bitmap_length_bytes.next_multiple_of(bytes_per_cluster);

        let uptable_offset_bytes = bitmap_offset_bytes + cluster_length;
        let uptable_start_cluster = FIRST_USABLE_CLUSTER_INDEX + cluster_length / bytes_per_cluster;
        let uptable_length_bytes = UPCASE_TABLE_SIZE_BYTES;

        let cluster_length = uptable_length_bytes.next_multiple_of(bytes_per_cluster);

        let root_offset_bytes = uptable_offset_bytes + cluster_length;
        let first_cluster_of_root_directory =
            uptable_start_cluster + cluster_length / bytes_per_cluster;

        let file_system_revision = FileSystemRevision::default();
        let volume_serial_number = VolumeSerialNumber::try_new()?;

        let root_length_bytes = size_of::<DirEntry>() as u32 * 3;
        let cluster_count_used = 0; // in the beginning no cluster is used

        Ok(Self {
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
            volume_flags,
            volume_serial_number,
            file_system_revision,
            size,
            bytes_per_cluster,
            bytes_per_sector,
            root_offset_bytes,
            format_options,
            bitmap_length_bytes,
            uptable_length_bytes,
            root_length_bytes,
            cluster_count_used,
            bitmap_offset_bytes,
            uptable_offset_bytes,
        })
    }

    /// Attempts to write the boot region & FAT onto the device. The file length must be the same as the
    /// provided `dev_size` in the [`Formatter`].
    pub fn write<T: Write + Seek>(&mut self, f: &mut T) -> Result<(), ExFatError> {
        let old_pos = f.stream_position()?;
        let len = f.seek(SeekFrom::End(0))?;

        if old_pos != len {
            f.seek(SeekFrom::Start(old_pos))?;
        }

        assert_eq!(len, self.format_options.dev_size);

        if len != self.format_options.dev_size {
            return Err(ExFatError::InvalidFileSize);
        }

        let size = if self.format_options.full_format {
            self.size
        } else {
            self.root_offset_bytes as u64 + self.bytes_per_cluster as u64
        };

        // clear disk size as needed
        disk::write_zeroes(f, size, 0)?;

        // write main boot region
        self.write_boot_region(f, MAIN_BOOT_OFFSET)?;

        // write backup boot region
        self.write_boot_region(f, BACKUP_BOOT_OFFSET)?;

        // write fat
        self.write_fat(f)?;

        // write bitmap
        self.write_bitmap(f)?;

        // write uptable
        self.write_upcase_table(f)?;
        Ok(())
    }
}

impl Formatter {
    fn write_bitmap<T: Write + Seek>(&self, device: &mut T) -> io::Result<()> {
        let mut bitmap = vec![0u8; self.bitmap_length_bytes as usize];

        // number of currently completely used bytes (set to 0xff)
        let full_bytes = self.cluster_count_used / 8;
        // remaining clusters that don't fully complete a byte
        let remaining_bits = self.cluster_count_used % 8;

        // offset to the first byte that can be fully used (set to 0x00)
        let mut zero_offset = full_bytes;

        bitmap[..full_bytes as usize].fill(0xff);

        // set the remaining bits
        if remaining_bits != 0 {
            bitmap[full_bytes as usize] = (1 << remaining_bits) - 1;
            zero_offset += 1;
        }

        if zero_offset < self.bitmap_length_bytes {
            bitmap[(zero_offset as usize)..].fill(0);
        }

        device.seek(SeekFrom::Start(self.bitmap_offset_bytes as u64))?;
        device.write_all(cast_slice(&bitmap))
    }
}
