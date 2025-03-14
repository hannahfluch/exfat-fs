use std::io::{self, Seek, SeekFrom, Write};

use crate::FIRST_USABLE_CLUSTER_INDEX;

use super::Exfat;

#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
struct FatEntry(u32);

impl FatEntry {
    /// The media type FAT entry. `F8h` as the first byte and `FFh` for the remeaining three bytes.
    fn media_type() -> FatEntry {
        Self(0xfffffff8u32)
    }

    /// Marks the end of a cluster chain.
    fn eof() -> FatEntry {
        Self(0xffffffff)
    }
}

impl Exfat {
    pub(super) fn write_fat<T: Write + Seek>(&mut self, device: &mut T) -> io::Result<()> {
        // write entry 0 (media type)
        self.write_fat_entry(device, FatEntry::media_type(), 0)?;

        // write entry 1 (reserved)
        self.write_fat_entry(device, FatEntry::eof(), 1)?;

        // write bitmap entries
        let mut index =
            self.write_fat_entries(device, FIRST_USABLE_CLUSTER_INDEX, self.bitmap_length_bytes)?;

        // write upcase table entries
        index = self.write_fat_entries(device, index, self.uptable_length_bytes)?;

        // write root directory entries
        index = self.write_fat_entries(device, index, self.root_length_bytes)?;

        self.cluster_count_used = index - FIRST_USABLE_CLUSTER_INDEX;

        Ok(())
    }

    fn write_fat_entry<T: Write + Seek>(
        &self,
        device: &mut T,
        entry: FatEntry,
        index: u64,
    ) -> io::Result<()> {
        let offset_bytes = self.fat_offset as u64 * self.format_options.bytes_per_sector as u64
            + index * size_of::<FatEntry>() as u64;
        device.seek(SeekFrom::Start(offset_bytes))?;
        device.write_all(&entry.0.to_le_bytes())
    }

    /// Writes a cluster chain onto the device and returns the next free FAT entry.
    fn write_fat_entries<T: Write + Seek>(
        &self,
        device: &mut T,
        cluster: u32,
        length: u32,
    ) -> io::Result<u32> {
        let count =
            cluster + length.next_multiple_of(self.bytes_per_cluster) / self.bytes_per_cluster;

        // write fat entry for each cluster in chain
        for current_cluster in cluster..count - 1 {
            self.write_fat_entry(
                device,
                FatEntry(current_cluster + 1),
                current_cluster as u64,
            )?;
        }

        // write cluster chain EOF
        self.write_fat_entry(device, FatEntry::eof(), count as u64 - 1)?;

        Ok(count)
    }
}

#[test]
fn small_fat_creation() {
    use super::Exfat;
    use super::FormatVolumeOptionsBuilder;

    let size: u64 = 32 * crate::MB as u64;
    let mut f = std::io::Cursor::new(vec![0u8; size as usize]);

    let format_options = FormatVolumeOptionsBuilder::default()
        .pack_bitmap(false)
        .full_format(false)
        .partition_offset(0)
        .boundary_align(crate::DEFAULT_BOUNDARY_ALIGNEMENT)
        .dev_size(size)
        .bytes_per_sector(512)
        .build()
        .unwrap();

    let mut formatter = Exfat::try_from(format_options).unwrap();

    formatter.write(&mut f).unwrap();

    assert_eq!(formatter.cluster_count_used, 4);
}

#[test]
fn medium_fat_creation() {
    use super::Exfat;
    use super::FormatVolumeOptionsBuilder;

    let size: u64 = 512 * crate::MB as u64;
    let mut f = std::io::Cursor::new(vec![0u8; size as usize]);

    let format_options = FormatVolumeOptionsBuilder::default()
        .pack_bitmap(false)
        .full_format(false)
        .partition_offset(0)
        .boundary_align(crate::DEFAULT_BOUNDARY_ALIGNEMENT)
        .dev_size(size)
        .bytes_per_sector(512)
        .build()
        .unwrap();

    let mut formatter = Exfat::try_from(format_options).unwrap();

    formatter.write(&mut f).unwrap();

    assert_eq!(formatter.cluster_count_used, 3);
}
