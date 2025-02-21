use std::{
    io::{Seek, Write},
    ops::{Div, Sub},
};

use crate::error::ExFatError;

use super::{
    sector::BootSector, FileSystemRevision, VolumeFlags, VolumeSerialNumber, FIRST_CLUSTER_INDEX,
    MAX_CLUSTER_COUNT, MAX_CLUSTER_SIZE, UPCASE_TABLE_SIZE_BYTES,
};

#[derive(Copy, Clone, Debug)]
pub struct BootSectorMeta {
    pub(in crate::boot) partition_offset: u64,
    pub(in crate::boot) volume_length: u64,
    pub(in crate::boot) fat_offset: u32,
    pub(in crate::boot) fat_length: u32,
    pub(in crate::boot) cluster_heap_offset: u32,
    pub(in crate::boot) cluster_count: u32,
    pub(in crate::boot) first_cluster_of_root_directory: u32,
    pub(in crate::boot) file_system_revision: FileSystemRevision,
    pub(in crate::boot) volume_flags: u16,
    pub(in crate::boot) bytes_per_sector_shift: u8,
    pub(in crate::boot) sectors_per_cluster_shift: u8,
    pub(in crate::boot) number_of_fats: u8,
    pub(in crate::boot) uptable_offset_bytes: u32,
    pub(in crate::boot) bitmap_offset_bytes: u32,
    pub(in crate::boot) bytes_per_sector: u16,
    pub(in crate::boot) bytes_per_cluster: u32,
    pub(in crate::boot) size: u64,
    pub(in crate::boot) boundary_align: u32,
    pub(in crate::boot) pack_bitmap: bool,
    pub(in crate::boot) volume_serial_number: VolumeSerialNumber,
}

impl BootSectorMeta {
    pub fn try_new(
        partition_offset: u64,
        bytes_per_sector: u16,
        bytes_per_cluster: u32,
        size: u64,
        boundary_align: u32,
        pack_bitmap: bool,
    ) -> Result<BootSectorMeta, ExFatError> {
        if !bytes_per_sector.is_power_of_two() || !(512..=4096).contains(&bytes_per_sector) {
            return Err(ExFatError::InvalidBytesPerSector(bytes_per_sector));
        }

        // format volume with a single FAT
        let number_of_fats = 1u8;
        let volume_flags = VolumeFlags::empty().bits();

        // transform partition_offset to be measured by sectors
        let partition_offset = partition_offset / bytes_per_sector as u64;

        if !bytes_per_cluster.is_power_of_two()
            || !(bytes_per_sector as u32..=MAX_CLUSTER_SIZE).contains(&bytes_per_cluster)
        {
            return Err(ExFatError::InvlaidClusterSize(bytes_per_cluster));
        }
        let bytes_per_sector_shift = bytes_per_sector.ilog2() as u8;
        let sectors_per_cluster_shift = (bytes_per_cluster / bytes_per_sector as u32).ilog2() as u8;

        let volume_length = size
            .checked_div(bytes_per_sector.into())
            .and_then(|o| {
                if o < (1 << (20 - bytes_per_sector_shift)) {
                    None
                } else {
                    Some(o)
                }
            })
            .ok_or(ExFatError::InvalidSize(size))?;

        let fat_offset_bytes: u32 = (bytes_per_sector as u64)
            .checked_mul(24)
            .and_then(|prd| prd.checked_add(partition_offset))
            .ok_or(ExFatError::InvalidPartitionOffset(partition_offset))?
            .next_multiple_of(boundary_align as u64)
            .sub(partition_offset)
            .try_into()
            .map_err(|_| ExFatError::BoundaryAlignemntTooBig(boundary_align))?;

        let fat_offset = fat_offset_bytes / bytes_per_sector as u32;

        let max_clusters: u64 = size
            .checked_sub(fat_offset_bytes as u64)
            .and_then(|d| d.checked_sub(number_of_fats as u64 * 8))
            .and_then(|d| d.checked_sub(1))
            .and_then(|d| d.checked_div(bytes_per_cluster as u64 + 4 * number_of_fats as u64))
            .and_then(|q| q.checked_add(1))
            .ok_or(ExFatError::InvlaidClusterSize(bytes_per_cluster))?;

        let fat_length_bytes = max_clusters
            .checked_add(2)
            .and_then(|x| x.checked_mul(4))
            .map(|x| x.next_multiple_of(bytes_per_sector as u64))
            .ok_or(ExFatError::InvlaidClusterSize(bytes_per_cluster))?;

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

        if pack_bitmap {
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
        let uptable_start_cluster = FIRST_CLUSTER_INDEX as u32 + cluster_length / bytes_per_cluster;
        let uptable_length_bytes = UPCASE_TABLE_SIZE_BYTES;

        let cluster_length = (uptable_length_bytes as u32).next_multiple_of(bytes_per_cluster);

        let first_cluster_of_root_directory =
            uptable_start_cluster + cluster_length / bytes_per_cluster;

        let file_system_revision = FileSystemRevision::default();
        let volume_serial_number = VolumeSerialNumber::try_new()?;
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
            bitmap_offset_bytes,
            uptable_offset_bytes,
            size,
            bytes_per_cluster,
            bytes_per_sector,
            pack_bitmap,
            boundary_align,
        })
    }

    /// Attempts to write the boot sector onto the device. Returning a struct of all the data
    /// written.
    pub fn write<T>(&self, f: &mut T) -> Result<BootSector, ExFatError>
    where
        T: Write + Seek,
    {
        let len = f
            .seek(std::io::SeekFrom::End(0))
            .map_err(ExFatError::from)?;

        println!("length: {}, size: {}", len, self.size);
        if len < self.size {
            return Err(ExFatError::InvalidFileSize);
        }

        let _boot_sector = BootSector::new(self);
        todo!();
    }
}
