use crate::{
    Label,
    boot_sector::{BootSector, VolumeFlags},
    disk::Read,
    error::RootError,
};
use bytemuck::from_bytes_mut;
use entry::{
    BitmapEntry, DirEntry, UpcaseTableEntry, VOLUME_GUID_ENTRY_TYPE, VolumeGuidEntry,
    VolumeLabelEntry,
};

pub(crate) mod entry;

/// Root directory entry.
pub struct Root {
    vol_label: DirEntry,
    vol_guid: DirEntry,
    bitmap: DirEntry,
    uptable: DirEntry,
    items: Vec<DirEntry>,
}

impl Root {
    pub(crate) fn new(
        volume_label: Label,
        volume_guid: Option<u128>,
        bitmap_length_bytes: u64,
        uptable_start_cluster: u32,
    ) -> Root {
        // create volume label entry
        let vol_label = DirEntry::VolumeLabel(VolumeLabelEntry::new(volume_label));

        // create volume GUID entry
        let vol_guid = if let Some(guid) = volume_guid {
            DirEntry::VolumeGuid(VolumeGuidEntry::new(guid))
        } else {
            DirEntry::unused(VOLUME_GUID_ENTRY_TYPE)
        };

        // create bitmap entry
        let bitmap = DirEntry::Bitmap(BitmapEntry::new(bitmap_length_bytes));

        // create upcase table entry
        let uptable = DirEntry::UpcaseTable(UpcaseTableEntry::new(uptable_start_cluster));

        Root {
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

impl Root {
    pub fn open<R: Read>(device: R) -> Result<(), RootError<R::ReadError>> {
        let mut bytes = vec![0; 512];
        device.read_exact(0, &mut bytes)?;
        let boot_sector = from_bytes_mut::<BootSector>(&mut bytes);

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

        // todo: load FAT

        unimplemented!()
    }
}
