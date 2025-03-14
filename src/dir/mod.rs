use crate::Label;
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
