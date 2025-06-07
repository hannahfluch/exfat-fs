use core::ops::{Div, Sub};

use crate::{
    DEFAULT_BOUNDARY_ALIGNEMENT, FIRST_USABLE_CLUSTER_INDEX, GB, KB, Label, MB,
    boot_sector::{FileSystemRevision, UnixEpochDuration, VolumeFlags, VolumeSerialNumber},
    dir::{RawRoot, entry::DirEntry},
    disk::{SeekFrom, WriteSeek},
    error::ExfatError,
    upcase_table::{DEFAULT_UPCASE_TABLE, UPCASE_TABLE_SIZE_BYTES},
};
use boot::{BACKUP_BOOT_OFFSET, MAIN_BOOT_OFFSET, MAX_CLUSTER_COUNT, MAX_CLUSTER_SIZE};
use bytemuck::cast_slice;
use checked_num::CheckedU64;
use derive_builder::Builder;

use crate::{disk, error::ExfatFormatError};
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
/// ExFat boot sector creation.
mod boot;
mod fat;

/// A struct of exfat formatting options. It implements the [`derive_builder::Builder`] pattern.
#[derive(Builder, Copy, Clone, Debug)]
#[builder(no_std, build_fn(validate = "Self::validate"))]
pub struct FormatVolumeOptions {
    /// Whether or not to pack the bitmap right after the FAT for better performance and space
    /// usage. Defaults to `true`.
    #[builder(default = true)]
    pack_bitmap: bool,
    /// Whether to fully format the volume, which takes longer. Defaults to `false`.
    #[builder(default)]
    full_format: bool,
    /// Size of the target device (in bytes)
    dev_size: u64,
    /// Label of the format
    #[builder(default)]
    label: Label,
    /// Optional GUID. Defaults to `None`.
    #[builder(default)]
    guid: Option<u128>,
    /// Media-relative sector offset of the partition which hosts the given exFAT volume. Defaults
    /// to `0`.
    #[builder(default)]
    partition_offset: u64,
    /// Amount of bytes per sector. Must be a power of `2` and between `512` and `4096`.
    bytes_per_sector: u16,
    /// Byte alignment for filesystem structures like the FAT and Up-case table. Defaults to
    /// [`DEFAULT_BOUNDARY_ALIGNEMENT`].
    #[builder(default = DEFAULT_BOUNDARY_ALIGNEMENT)]
    boundary_align: u32,
}

impl FormatVolumeOptionsBuilder {
    fn validate(&self) -> Result<(), String> {
        if let Some(ref bytes_per_sector) = self.bytes_per_sector {
            if !bytes_per_sector.is_power_of_two() || !(512..=4096).contains(bytes_per_sector) {
                return Err(
                    "Bytes per sector field must be a power of two and between `512` and `4096`."
                        .to_string(),
                );
            }
        }

        if let Some(ref boundary_align) = self.boundary_align {
            if !boundary_align.is_power_of_two() {
                return Err("Boundary alignment field must be a power of two.".to_string());
            }
        }

        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Exfat {
    volume_length: u64,
    fat_offset: u32,
    fat_length: u32,
    cluster_heap_offset: u32,
    cluster_count: u32,
    cluster_count_used: u32,
    first_cluster_of_root_directory: u32,
    file_system_revision: FileSystemRevision,
    volume_flags: VolumeFlags,
    bytes_per_sector_shift: u8,
    sectors_per_cluster_shift: u8,
    number_of_fats: u8,
    uptable_length_bytes: u32,
    bitmap_length_bytes: u32,
    bitmap_offset_bytes: u32,
    bytes_per_cluster: u32,
    volume_serial_number: VolumeSerialNumber,
    root_offset_bytes: u32,
    format_options: FormatVolumeOptions,
    root_length_bytes: u32,
    uptable_offset_bytes: u32,
    uptable_start_cluster: u32,
}

impl Exfat {
    /// Attempts to initialize an exFAT formatter instance based on the [`FormatVolumeOptions`]
    /// provided.
    pub fn try_from<T: UnixEpochDuration>(
        format_options: FormatVolumeOptions,
    ) -> Result<Self, ExfatFormatError<T>> {
        let size = format_options.dev_size;

        let bytes_per_cluster = default_cluster_size(size);

        // format volume with a single FAT
        let number_of_fats = 1u8;
        let volume_flags = VolumeFlags::empty();

        // transform partition_offset to be measured by sectors
        let partition_offset =
            format_options.partition_offset / format_options.bytes_per_sector as u64;

        if !bytes_per_cluster.is_power_of_two()
            || !(format_options.bytes_per_sector as u32..=MAX_CLUSTER_SIZE)
                .contains(&bytes_per_cluster)
        {
            return Err(ExfatFormatError::InvlaidClusterSize(bytes_per_cluster));
        }
        let bytes_per_sector_shift = format_options.bytes_per_sector.ilog2() as u8;
        let sectors_per_cluster_shift =
            (bytes_per_cluster / format_options.bytes_per_sector as u32).ilog2() as u8;

        let volume_length = size / format_options.bytes_per_sector as u64;

        if volume_length < (1 << (20 - bytes_per_sector_shift)) {
            return Err(ExfatFormatError::InvalidSize(size));
        }

        let fat_offset_bytes: u32 = (CheckedU64::new(format_options.bytes_per_sector as u64) * 24
            + partition_offset)
            .ok_or(ExfatFormatError::InvalidPartitionOffset(partition_offset))?
            .next_multiple_of(format_options.boundary_align as u64)
            .sub(partition_offset)
            .try_into()
            .map_err(|_| {
                ExfatFormatError::BoundaryAlignemntTooBig(format_options.boundary_align)
            })?;

        let fat_offset = fat_offset_bytes / format_options.bytes_per_sector as u32;

        let max_clusters: CheckedU64 =
            ((CheckedU64::new(size) - fat_offset_bytes as u64 - number_of_fats as u64 * 8 - 1)
                / (bytes_per_cluster as u64 + 4 * number_of_fats as u64)
                + 1)
            .ok_or(ExfatFormatError::InvlaidClusterSize(bytes_per_cluster))?
            .into();

        let fat_length_bytes = ((max_clusters + 2) * 4)
            .ok_or(ExfatFormatError::InvlaidClusterSize(bytes_per_cluster))?
            .next_multiple_of(format_options.bytes_per_sector as u64);

        let fat_length: u32 = (fat_length_bytes / format_options.bytes_per_sector as u64)
            .try_into()
            .map_err(|_| ExfatFormatError::InvlaidClusterSize(bytes_per_cluster))?;

        let mut cluster_heap_offset_bytes = ((partition_offset
            + fat_offset_bytes as u64
            + fat_length_bytes * number_of_fats as u64)
            .next_multiple_of(format_options.boundary_align as u64)
            - partition_offset) as u32;

        let mut cluster_heap_offset =
            cluster_heap_offset_bytes / format_options.bytes_per_sector as u32;

        if cluster_heap_offset_bytes as u64 >= size {
            return Err(ExfatFormatError::BoundaryAlignemntTooBig(
                format_options.boundary_align,
            ));
        }

        let mut cluster_count: u32 = ((size - cluster_heap_offset_bytes as u64)
            / bytes_per_cluster as u64)
            .try_into()
            .map_err(|_| ExfatFormatError::InvlaidClusterSize(bytes_per_cluster))?;

        if cluster_count
            > MAX_CLUSTER_COUNT.min(
                ((volume_length - cluster_heap_offset as u64)
                    / 2u64.pow(sectors_per_cluster_shift as u32)) as u32,
            )
        {
            return Err(ExfatFormatError::InvlaidClusterSize(bytes_per_cluster));
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
                    return Err(ExfatFormatError::CannotPackBitmap);
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
            cluster_heap_offset =
                cluster_heap_offset_bytes / format_options.bytes_per_sector as u32;
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
        let volume_serial_number =
            VolumeSerialNumber::try_new::<T>().map_err(|err| ExfatFormatError::NoSerial(err))?;

        let root_length_bytes = size_of::<DirEntry>() as u32 * 3;
        let cluster_count_used = 0; // in the beginning no cluster is used

        Ok(Self {
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
            bytes_per_cluster,
            root_offset_bytes,
            format_options,
            bitmap_length_bytes,
            uptable_length_bytes,
            root_length_bytes,
            cluster_count_used,
            bitmap_offset_bytes,
            uptable_offset_bytes,
            uptable_start_cluster,
        })
    }
}

impl Exfat {
    /// Attempts to write the boot region & FAT onto the device. The file length must be the same as the
    /// provided `dev_size` in the [`Exfat`].
    pub fn write<T: UnixEpochDuration, O: WriteSeek>(
        &mut self,
        f: &mut O,
    ) -> Result<(), ExfatError<T, O>>
    where
        T::Err: core::fmt::Debug,
    {
        let old_pos = f.stream_position().map_err(|err| ExfatError::Io(err))?;
        let len = f
            .seek(SeekFrom::End(0))
            .map_err(|err| ExfatError::Io(err))?;

        if old_pos != len {
            f.seek(SeekFrom::Start(old_pos))
                .map_err(|err| ExfatError::Io(err))?;
        }

        assert_eq!(len, self.format_options.dev_size);

        if len != self.format_options.dev_size {
            return Err(ExfatError::Format(ExfatFormatError::InvalidFileSize));
        }

        let size = if self.format_options.full_format {
            self.format_options.dev_size
        } else {
            self.root_offset_bytes as u64 + self.bytes_per_cluster as u64
        };

        // clear disk size as needed
        disk::write_zeroes(f, size, 0).map_err(|err| ExfatError::Io(err))?;

        // write main boot region
        self.write_boot_region(f, MAIN_BOOT_OFFSET)
            .map_err(|err| ExfatError::Io(err))?;

        // write backup boot region
        self.write_boot_region(f, BACKUP_BOOT_OFFSET)
            .map_err(|err| ExfatError::Io(err))?;

        // write fat
        self.write_fat(f).map_err(|err| ExfatError::Io(err))?;

        // write bitmap
        self.write_bitmap(f).map_err(|err| ExfatError::Io(err))?;

        // write uptable
        self.write_upcase_table(f)
            .map_err(|err| ExfatError::Io(err))?;

        // write root directory
        self.write_root_dir(f).map_err(|err| ExfatError::Io(err))?;
        Ok(())
    }
}

/// default cluster size based on sector size
fn default_cluster_size(size: u64) -> u32 {
    const FIRST_BOUND: u64 = 256 * MB as u64;
    const FROM_FIRST_BOUND: u64 = FIRST_BOUND + 1;

    const SECOND_BOUND: u64 = 32 * GB as u64;
    const FROM_SECOND_BOUND: u64 = SECOND_BOUND + 1;

    match size {
        ..=FIRST_BOUND => 4 * KB as u32,
        FROM_FIRST_BOUND..=SECOND_BOUND => 32 * KB as u32,
        FROM_SECOND_BOUND.. => 128 * KB as u32,
    }
}

impl Exfat {
    fn write_upcase_table<T: WriteSeek>(&self, device: &mut T) -> Result<(), T::Err> {
        device.seek(SeekFrom::Start(self.uptable_offset_bytes as u64))?;
        device.write_all(&DEFAULT_UPCASE_TABLE)
    }

    fn write_bitmap<T: WriteSeek>(&self, device: &mut T) -> Result<(), T::Err> {
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

    fn write_root_dir<T: WriteSeek>(&self, device: &mut T) -> Result<(), T::Err> {
        let root = RawRoot::new(
            self.format_options.label,
            self.format_options.guid,
            self.bitmap_length_bytes as u64,
            self.uptable_start_cluster,
        );

        device.seek(SeekFrom::Start(self.root_offset_bytes as u64))?;
        device.write_all(&root.bytes())?;
        Ok(())
    }
}

#[cfg(test)]
#[test]
fn small_format() {
    use crate::Label;
    use crate::format::FormatVolumeOptionsBuilder;
    use std::io::Read;
    use std::vec::Vec;

    let size: u64 = 32 * crate::MB as u64;
    let mut f = std::io::Cursor::new(vec![0u8; size as usize]);

    let label = Label::new("Hello".to_string()).expect("label creation failed");

    let format_options = FormatVolumeOptionsBuilder::default()
        .label(label)
        .pack_bitmap(false)
        .full_format(false)
        .dev_size(size)
        .bytes_per_sector(512)
        .boundary_align(crate::DEFAULT_BOUNDARY_ALIGNEMENT)
        .build()
        .expect("building format volume option failed");

    let mut formatter =
        Exfat::try_from::<std::time::SystemTime>(format_options).expect("formatting failed");
    formatter
        .write::<std::time::SystemTime, std::io::Cursor<Vec<u8>>>(&mut f)
        .expect("writing failed");

    let offset_volume_label_entry_bytes = 0x203000;
    let mut read_buffer = vec![0u8; 32];
    f.seek(crate::disk::SeekFrom::Start(
        offset_volume_label_entry_bytes,
    ))
    .unwrap();
    f.read_exact(&mut read_buffer).unwrap();

    // assert volume label root directory entry is at the expected offset
    let vol_label_entry_type = read_buffer[0];
    assert_eq!(
        vol_label_entry_type, 0x83,
        "Volume Label Root Directory Entry has invalid type"
    );

    // assert volume label length is correct
    let vol_label_length = read_buffer[1];
    assert_eq!(
        vol_label_length, 5,
        "Volume Label Root Directory Entry has invalid label length"
    );

    // assert volume label data is correct
    assert_eq!(
        &read_buffer[2..2 + vol_label_length as usize],
        &label.0[..vol_label_length as usize],
        "Volume Label Root Directory Entry has invalid data"
    );
    let offset_upcase_table_entry_bytes = 0x203060;

    f.seek(crate::disk::SeekFrom::Start(
        offset_upcase_table_entry_bytes,
    ))
    .unwrap();
    f.read_exact(&mut read_buffer).unwrap();

    // assert upcase table root directory entry is at the expected offset
    assert_eq!(
        read_buffer[0], 0x82,
        "Upcase Table Root Directory Entry has invalid type"
    );

    // assert upcase table root directory entry checksum is correct
    assert_eq!(
        u32::from_le_bytes(read_buffer[4..8].try_into().unwrap()),
        0xe619d30d,
        "Upcase Table Root Directory Entry has invalid checksum"
    );

    let offset_bitmap_entry_bytes = 0x203040;

    f.seek(crate::disk::SeekFrom::Start(offset_bitmap_entry_bytes))
        .unwrap();
    f.read_exact(&mut read_buffer).unwrap();

    // assert bitmap root directory entry is at the expected offset
    assert_eq!(
        read_buffer[0], 0x81,
        "Allocation Bitmap Root Directory Entry has invalid type"
    );

    // assert bitmap root directory entry is of the expected size
    assert_eq!(
        &read_buffer[24..],
        960u64.to_le_bytes(),
        "Allocation Bitmap Root Directory Entry has invalid size"
    );
}
