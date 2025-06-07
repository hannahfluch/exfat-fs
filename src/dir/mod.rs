use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::{
    Label,
    boot_sector::{BootSector, VolumeFlags},
    disk::ReadOffset,
    error::RootError,
    fat::Fat,
};
use bytemuck::from_bytes_mut;
use endify::Endify;
use entry::{
    BitmapEntry, ClusterAllocation, DirEntry, UpcaseTableEntry, VOLUME_GUID_ENTRY_TYPE,
    VolumeGuidEntry, VolumeLabelEntry,
    parsed::{Directory, File, FsElement, ParsedFileEntry},
};
use reader::{
    DirEntryReader,
    cluster::{ClusterChainOptions, ClusterChainReader},
};

pub(crate) mod entry;
pub(crate) mod reader;

/// Buffer used to read the boot sector.
#[repr(align(8))]
struct AlignedBootSector([u8; 512]);

/// Root directory entry.
pub struct RawRoot {
    vol_label: DirEntry,
    vol_guid: DirEntry,
    bitmap: DirEntry,
    uptable: DirEntry,
    items: Vec<DirEntry>,
}

impl RawRoot {
    pub(crate) fn new(
        volume_label: Label,
        volume_guid: Option<u128>,
        bitmap_length_bytes: u64,
        uptable_start_cluster: u32,
    ) -> RawRoot {
        // create volume label entry
        let vol_label = DirEntry::VolumeLabel(VolumeLabelEntry::new(volume_label));

        // create volume GUID entry
        let vol_guid = if let Some(guid) = volume_guid {
            DirEntry::VolumeGuid(VolumeGuidEntry::new(guid))
        } else {
            DirEntry::new_unused(VOLUME_GUID_ENTRY_TYPE)
        };

        // create bitmap entry
        let bitmap = DirEntry::Bitmap(BitmapEntry::new(bitmap_length_bytes));

        // create upcase table entry
        let uptable = DirEntry::UpcaseTable(UpcaseTableEntry::new(uptable_start_cluster));

        RawRoot {
            vol_label,
            vol_guid,
            bitmap,
            uptable,
            items: Vec::default(),
        }
    }

    pub(crate) fn bytes(self) -> Vec<u8> {
        let mut all_items = vec![self.vol_label, self.vol_guid, self.bitmap, self.uptable];
        all_items.extend(self.items);
        all_items
            .into_iter()
            .flat_map(|b| b.bytes())
            .collect::<Vec<u8>>()
    }
}

pub struct Root<O: ReadOffset> {
    volume_label: Option<Label>,
    items: Vec<FsElement<O>>,
}

impl<O: ReadOffset> Root<O> {
    pub fn label(&self) -> Option<&Label> {
        self.volume_label.as_ref()
    }
    pub fn items(&self) -> &[FsElement<O>] {
        &self.items
    }
}

impl<O: ReadOffset> Root<O> {
    pub fn open(device: O) -> Result<Self, RootError<O>>
    where
        O::Err: core::fmt::Debug,
        O: core::fmt::Debug,
    {
        let device = Arc::new(device);
        let mut aligned = Box::new(AlignedBootSector([0u8; 512]));
        device
            .read_exact(0, &mut aligned.0[..])
            .map_err(RootError::Io)?;

        let boot_sector = from_bytes_mut::<BootSector>(&mut aligned.0);

        // convert to native endianess
        let boot_sector = Arc::new(Endify::from_le(*boot_sector));

        // check for fs name
        if boot_sector.filesystem_name != *b"EXFAT   " {
            return Err(RootError::WrongFs);
        }

        // check for bytes per sector shift
        if !(9..=12).contains(&boot_sector.bytes_per_sector_shift) {
            return Err(RootError::InvalidBytesPerSectorShift(
                boot_sector.bytes_per_sector_shift,
            ));
        }

        // check for sectors per cluster shift
        if boot_sector.sectors_per_cluster_shift > 25 - boot_sector.bytes_per_sector_shift {
            return Err(RootError::InvalidSectorsPerClusterShift(
                boot_sector.sectors_per_cluster_shift,
            ));
        }

        // check for number of fats
        let fat_num = if [1, 2].contains(&boot_sector.number_of_fats) {
            Ok(boot_sector.number_of_fats)
        } else {
            Err(RootError::InvalidNumberOfFats(boot_sector.number_of_fats))
        }?;
        let volume_flags = VolumeFlags::from_bits_truncate(boot_sector.volume_flags);

        // check for correct active fat
        if volume_flags.contains(VolumeFlags::ACTIVE_FAT) && fat_num == 1
            || !volume_flags.contains(VolumeFlags::ACTIVE_FAT) && fat_num == 2
        {
            return Err(RootError::InvalidNumberOfFats(fat_num));
        }

        // parse FAT
        let fat = Arc::new(Fat::load(&device, &boot_sector)?);

        let first_cluster = boot_sector.first_cluster_of_root_directory;
        // check for correct index of root cluster
        if first_cluster < 2 || first_cluster > boot_sector.cluster_count + 1 {
            return Err(RootError::InvalidRootDirectoryClusterIndex(first_cluster));
        }

        let mut reader = DirEntryReader::from(ClusterChainReader::try_new(
            Arc::clone(&boot_sector),
            &fat,
            first_cluster,
            ClusterChainOptions::default(),
            Arc::clone(&device),
        )?);

        // Load root directory
        let mut allocation_bitmaps: [Option<BitmapEntry>; 2] = [None, None];
        let mut upcase_table: Option<UpcaseTableEntry> = None;
        let mut volume_label: Option<Label> = None;
        let mut items: Vec<FsElement<O>> = Vec::new();

        loop {
            let entry = reader.read()?;

            // unused entries are ignored
            if entry.unused() {
                continue;
            }

            if !entry.regular() {
                break;
            } else if !entry.primary() {
                return Err(RootError::RootEntryNotPrimary(entry.entry_type()));
            }

            match entry {
                DirEntry::EndOfDirectory(_) => todo!(),
                DirEntry::Invalid => todo!(),
                DirEntry::Bitmap(bitmap_entry) => {
                    let index = if allocation_bitmaps[1].is_some() {
                        return Err(RootError::InvalidNumberOfAllocationBitmaps);
                    } else if allocation_bitmaps[0].is_some() {
                        1
                    } else {
                        0
                    };
                    if index != bitmap_entry.index() || !bitmap_entry.valid() {
                        return Err(RootError::InvalidAllocationBitmap);
                    }

                    allocation_bitmaps[index as usize] = Some(bitmap_entry);
                }
                DirEntry::UpcaseTable(upcase_table_entry) => {
                    if upcase_table.is_some() {
                        return Err(RootError::InvalidNumberOfUpcaseTables);
                    }
                    if !upcase_table_entry.valid() {
                        return Err(RootError::InvalidUpcaseTable);
                    }
                    upcase_table = Some(upcase_table_entry);
                }
                DirEntry::VolumeLabel(volume_label_entry) => {
                    if volume_label.is_some() {
                        return Err(RootError::InvalidNumberOfVolumeLabels);
                    }
                    if volume_label_entry.character_count > 11 {
                        return Err(RootError::InvalidVolumeLabel);
                    }

                    volume_label = Some(Label(
                        volume_label_entry.volume_label,
                        volume_label_entry.character_count,
                    ));
                }
                DirEntry::File(file_entry) => {
                    let parsed = ParsedFileEntry::try_new(&file_entry, &mut reader)?;
                    let item = if file_entry.file_attributes.is_directory() {
                        FsElement::D(Directory::new(
                            Arc::clone(&device),
                            Arc::clone(&boot_sector),
                            Arc::clone(&fat),
                            parsed.name,
                            parsed.stream_extension_entry,
                            parsed.timestamps,
                        ))
                    } else {
                        FsElement::F(File::try_new(
                            Arc::clone(&device),
                            Arc::clone(&boot_sector),
                            &fat,
                            parsed.name,
                            parsed.stream_extension_entry,
                            parsed.timestamps,
                        )?)
                    };

                    items.push(item);
                }
                _ => return Err(RootError::UnexpectedRootEntry(entry.entry_type())),
            }
        }

        // check allocation bitmap count
        if boot_sector.number_of_fats == 2 && allocation_bitmaps[1].is_none()
            || allocation_bitmaps[0].is_none()
        {
            return Err(RootError::InvalidNumberOfAllocationBitmaps);
        }

        // check upcase table
        if upcase_table.is_none() {
            return Err(RootError::InvalidNumberOfUpcaseTables);
        }
        Ok(Root {
            volume_label,
            items,
        })
    }
}
