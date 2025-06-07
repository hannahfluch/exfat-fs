use bytemuck::{checked::cast_slice, AnyBitPattern, NoUninit};
use checked_num::CheckedU64;
use endify::Endify;

use crate::{
    boot_sector::{BootSector, VolumeFlags},
    disk::ReadOffset,
    error::FatLoadError,
};

#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, AnyBitPattern, NoUninit, Endify)]
pub(crate) struct FatEntry(pub(crate) u32);

impl FatEntry {
    /// The media type FAT entry. `F8h` as the first byte and `FFh` for the remeaining three bytes.
    pub(crate) fn media_type() -> FatEntry {
        Self(0xfffffff8u32)
    }

    /// Marks the end of a cluster chain.
    pub(crate) fn eof() -> FatEntry {
        Self(0xffffffff)
    }

    /// Marks the cluster as `bad`
    pub(crate) fn bad() -> FatEntry {
        Self(0xfffffff7)
    }
}

#[repr(C)]
#[derive(Clone, Debug)]
pub(crate) struct Fat {
    entries: Vec<FatEntry>,
}

impl Fat {
    pub(crate) fn load<R: ReadOffset>(
        device: &R,
        boot: &BootSector,
    ) -> Result<Fat, FatLoadError<R>> {
        assert!([1, 2].contains(&boot.number_of_fats));
        let volume_flags = VolumeFlags::from_bits_truncate(boot.volume_flags);
        let index = if volume_flags.contains(VolumeFlags::ACTIVE_FAT) {
            1
        } else {
            0
        };
        assert_eq!(index + 1, boot.number_of_fats);

        let sector_offset =
            CheckedU64::new(boot.fat_length as u64) * index as u64 + boot.fat_offset as u64;
        let byte_offset =
            (sector_offset * boot.bytes_per_sector() as u64).ok_or(FatLoadError::InvalidOffset)?;

        // load FAT entries from disk
        let mut entries = vec![0u8; boot.cluster_count as usize * 4];

        device
            .read_exact(byte_offset, &mut entries)
            .map_err(|e| FatLoadError::ReadFailed(byte_offset, e))?;

        let entries = entries
            .chunks_exact_mut(4)
            .map(|c| FatEntry(u32::from_le_bytes(c.try_into().unwrap())))
            .collect::<Vec<FatEntry>>();

        Ok(Self { entries })
    }
}

pub(crate) struct ClusterChain<'fat> {
    entries: &'fat [u32],
    next: u32,
}

impl<'fat> ClusterChain<'fat> {
    pub(crate) fn new(table: &'fat Fat, first: u32) -> ClusterChain<'fat> {
        Self {
            entries: cast_slice(&table.entries),
            next: first,
        }
    }
}

impl Iterator for ClusterChain<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        // Check next entry.
        let entries = self.entries;
        let next = self.next as usize;

        if next < 2 || next >= entries.len() || entries[next] == FatEntry::bad().0 {
            return None;
        }

        // Move to next entry.
        self.next = entries[next];

        Some(next as u32)
    }
}
