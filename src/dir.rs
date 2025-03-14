#![allow(dead_code)] // todo: add file reading & writing
// http://ntfs.com/exfat-directory-structure.htm

use std::mem::transmute;

use crate::format::{
    Label,
    upcase_table::{DEFAULT_UPCASE_TABLE, DEFAULT_UPCASE_TABLE_CHECKSUM},
    util::FIRST_USABLE_CLUSTER_INDEX,
};

pub(crate) const VOLUME_GUID_ENTRY_TYPE: u8 = 0xA0;

#[derive(Copy, Clone, Debug)]
#[repr(C, u8)]
pub(crate) enum DirEntry {
    Invalid = 0x80,
    // critical primary:
    Bitmap(BitmapEntry),
    UpcaseTable(UpcaseTableEntry),
    VolumeLabel(VolumeLabelEntry),
    File(FileEntry) = 0x85,
    // benign primary:
    VolumeGuid(VolumeGuidEntry) = VOLUME_GUID_ENTRY_TYPE,
    // critical secondary:
    StreamExtension(StreamExtensionEntry) = 0xC0,
    FileName(FileNameEntry),
    // benign secondary:
    VendorExtension(VendorExtensionEntry) = 0xE0,
    VendorAllocation(VendorAllocationEntry),
}

impl DirEntry {
    pub(crate) fn bytes(&self) -> [u8; 32] {
        assert_eq!(size_of::<DirEntry>(), 32);
        unsafe { transmute::<DirEntry, [u8; 32]>(*self) }
    }

    pub(crate) fn entry_type(&self) -> u8 {
        // SAFETY: Because `Self` is marked `repr(u8)`, its layout is a `repr(C)` `union`
        // between `repr(C)` structs, each of which has the `u8` discriminant as its first
        // field, so we can read the discriminant without offsetting the pointer.
        unsafe { *<*const _>::from(self).cast::<u8>() }
    }

    pub(crate) fn unused(r#type: u8) -> DirEntry {
        assert_eq!(size_of::<DirEntry>(), 32);
        let mut bytes = [0u8; size_of::<DirEntry>()];
        bytes[0] = r#type & !(DirEntry::Invalid.entry_type());

        unsafe { transmute::<[u8; 32], DirEntry>(bytes) }
    }

    pub(crate) fn checksum(&self, input: u16) -> u16 {
        let bytes = self.bytes();

        let mut sum = input.rotate_right(1);
        sum = sum.wrapping_add(bytes[0] as u16);
        sum = sum.rotate_right(1);
        sum = sum.wrapping_add(bytes[1] as u16);

        let start = if (self.entry_type() & 0b00000100) == 0 {
            4 // primary
        } else {
            2 // secondary
        };

        for b in bytes[start..].iter() {
            sum = sum.rotate_right(1);
            sum = sum.wrapping_add(*b as u16);
        }

        sum
    }
}

// critical primary directory entry types:
#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct BitmapEntry {
    pub(crate) flags: u8,
    pub(crate) _reserved: [u8; 18],
    pub(crate) first_cluster: u32,
    pub(crate) data_len: u64,
}

impl BitmapEntry {
    pub(crate) fn new(data_len: u64) -> Self {
        Self {
            flags: 0, // currently, only one FAT and allocation bitmap are supported
            _reserved: [0; 18],
            first_cluster: FIRST_USABLE_CLUSTER_INDEX.to_le(),
            data_len: data_len.to_le(),
        }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct UpcaseTableEntry {
    pub(crate) _reserved1: [u8; 3],
    pub(crate) table_checksum: u32,
    pub(crate) _reserved2: [u8; 12],
    pub(crate) first_cluster: u32,
    pub(crate) data_len: u64,
}

impl UpcaseTableEntry {
    pub(crate) fn new(first_cluster: u32) -> Self {
        Self {
            _reserved1: [0; 3],
            table_checksum: DEFAULT_UPCASE_TABLE_CHECKSUM.to_le(),
            _reserved2: [0; 12],
            first_cluster: first_cluster.to_le(),
            data_len: DEFAULT_UPCASE_TABLE.len() as u64,
        }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct VolumeLabelEntry {
    pub(crate) character_count: u8,
    pub(crate) volume_label: [u8; 22],
    pub(crate) _reserved: u64,
}

impl VolumeLabelEntry {
    pub(crate) fn new(label: Label) -> Self {
        VolumeLabelEntry {
            character_count: label.1,
            volume_label: label.0,
            _reserved: 0,
        }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct FileEntry {
    pub(crate) secondary_count: u8,
    pub(crate) set_checksum: u16,
    pub(crate) file_attributes: u16,
    pub(crate) _reserved1: u16,
    pub(crate) create_timestamp: u32,
    pub(crate) last_modified_timestamp: u32,
    pub(crate) last_accessed_timestamp: u32,
    pub(crate) create_10ms_increment: u8,
    pub(crate) last_modified_10ms_increment: u8,
    pub(crate) create_utc_offset: u8,
    pub(crate) last_modified_utc_offset: u8,
    pub(crate) last_accessed_utc_offset: u8,
    pub(crate) _reserved2: [u8; 7],
}

impl FileEntry {
    pub(crate) fn new() -> Self {
        unimplemented!("file entry creation");
    }
}

// benign primary directory entry types:
#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct VolumeGuidEntry {
    pub(crate) secondary_count: u8,
    pub(crate) set_checksum: u16,
    pub(crate) general_primary_flag: u16,
    pub(crate) volume_guid: u128,
    pub(crate) _reserved: [u8; 10],
}

impl VolumeGuidEntry {
    pub(crate) fn new(volume_guid: u128) -> Self {
        let mut instance = VolumeGuidEntry {
            secondary_count: 0,
            set_checksum: 0,
            general_primary_flag: 0,
            volume_guid: volume_guid.to_le(),
            _reserved: [0; 10],
        };
        let entry = DirEntry::VolumeGuid(instance);
        let checksum = entry.checksum(0);
        instance.set_checksum = checksum;

        instance
    }
}

// skipping TexFat

// critcal secondary directory entry types:
#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct StreamExtensionEntry {
    pub(crate) general_secondary_flag: u8,
    pub(crate) _reserved1: u8,
    pub(crate) name_length: u8,
    pub(crate) name_hash: u16,
    pub(crate) _reserved2: u16,
    pub(crate) valid_data_length: u64,
    pub(crate) _reserved3: u32,
    pub(crate) first_cluster: u32,
    pub(crate) data_len: u64,
}

impl StreamExtensionEntry {
    pub(crate) fn new() -> Self {
        unimplemented!("stream extension entry creation");
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct FileNameEntry {
    pub(crate) general_secondary_flag: u8,
    pub(crate) file_name: [u8; 30],
}

impl FileNameEntry {
    pub(crate) fn new() -> Self {
        unimplemented!("file name entry creation");
    }
}

// benign secondary directory entry types:
#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct VendorExtensionEntry {
    pub(crate) general_secondary_flag: u8,
    pub(crate) vendor_guid: u128,
    pub(crate) vendor_defined: [u8; 14],
}

impl VendorExtensionEntry {
    pub(crate) fn new() -> Self {
        unimplemented!("vendor extesnion entry creation");
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct VendorAllocationEntry {
    pub(crate) general_secondary_flag: u8,
    pub(crate) vendor_guid: u128,
    pub(crate) vendor_defined: u16,
    pub(crate) first_cluster: u32,
    pub(crate) data_len: u64,
}

impl VendorAllocationEntry {
    pub(crate) fn new() -> Self {
        unimplemented!("vendor allocaton entry creation");
    }
}
