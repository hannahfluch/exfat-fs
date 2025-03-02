use std::{
    io::{self, Seek, SeekFrom, Write},
    ops::{Div, Sub},
};

use crate::dir::{
    BitmapEntry, DirEntry, UpcaseTableEntry, VolumeGuidEntry, VolumeLabelEntry,
    VOLUME_GUID_ENTRY_TYPE,
};
use bytemuck::cast_slice;
use checked_num::CheckedU64;
use util::{
    FileSystemRevision, VolumeSerialNumber, BACKUP_BOOT_OFFSET, FIRST_USABLE_CLUSTER_INDEX,
    MAIN_BOOT_OFFSET, MAX_CLUSTER_COUNT, MAX_CLUSTER_SIZE, UPCASE_TABLE_SIZE_BYTES,
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
    pub label: Label,
    pub guid: Option<u128>,
}

/// A UTF16 encoded volume label. The length must not exceed 11 characters.
#[derive(Copy, Clone, Debug, Default)]
pub struct Label(pub(crate) [u8; 22], pub(crate) u8);

impl Label {
    pub fn new(label: String) -> Option<Label> {
        let len = label.len();
        if len > 11 {
            None
        } else {
            let mut utf16_bytes = [0u8; 22];

            let encoded: Vec<u8> = label.encode_utf16().flat_map(|x| x.to_le_bytes()).collect();

            let copy_len = encoded.len();
            assert!(copy_len <= 22);
            utf16_bytes[..copy_len].copy_from_slice(&encoded[..copy_len]);

            Some(Label(utf16_bytes, len as u8))
        }
    }
}

impl FormatOptions {
    pub fn new(pack_bitmap: bool, full_format: bool, dev_size: u64, label: Label) -> FormatOptions {
        Self {
            pack_bitmap,
            full_format,
            dev_size,
            label,
            guid: None,
        }
    }

    pub fn set_guid(&mut self, guid: u128) {
        self.guid = Some(guid);
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
    pub(super) uptable_start_cluster: u32,
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
            uptable_start_cluster,
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

        // write root directory
        self.write_root_dir(f)?;
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

    fn write_root_dir<T: Write + Seek>(&self, device: &mut T) -> io::Result<()> {
        // create volume label entry
        let vol_label = DirEntry::VolumeLabel(VolumeLabelEntry::new(self.format_options.label));

        // create volume GUID entry
        let vol_guid = if let Some(guid) = self.format_options.guid {
            DirEntry::VolumeGuid(VolumeGuidEntry::new(guid))
        } else {
            DirEntry::unused(VOLUME_GUID_ENTRY_TYPE)
        };

        // create bitmap entry
        let bitmap = DirEntry::Bitmap(BitmapEntry::new(self.bitmap_length_bytes as u64));

        // create upcase table entry
        let uptable = DirEntry::UpcaseTable(UpcaseTableEntry::new(self.uptable_start_cluster));

        let bytes = [vol_label, vol_guid, bitmap, uptable]
            .into_iter()
            .flat_map(|b| b.bytes())
            .collect::<Vec<u8>>();

        device.seek(SeekFrom::Start(self.root_offset_bytes as u64))?;
        device.write_all(&bytes)?;
        Ok(())
    }
}

#[test]
fn small_format() {
    use crate::format::{FormatOptions, Label};
    use std::io::Read;

    let size: u64 = 32 * crate::MB as u64;
    let mut f = std::io::Cursor::new(vec![0u8; size as usize]);
    let bytes_per_sector = 512;
    let bytes_per_cluster = 4 * crate::KB as u32;

    let label = Label::new("Hello".to_string()).expect("label creation failed");

    let mut formatter = Formatter::try_new(
        0,
        bytes_per_sector,
        bytes_per_cluster,
        size,
        crate::DEFAULT_BOUNDARY_ALIGNEMENT,
        FormatOptions::new(false, false, size, label),
    )
    .expect("formatting failed");
    formatter.write(&mut f).expect("writing failed");

    let offset_volume_label_entry_bytes = 0x203000;
    let mut read_buffer = vec![0u8; 32];
    f.seek(std::io::SeekFrom::Start(offset_volume_label_entry_bytes))
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

    f.seek(std::io::SeekFrom::Start(offset_upcase_table_entry_bytes))
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

    f.seek(std::io::SeekFrom::Start(offset_bitmap_entry_bytes))
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
