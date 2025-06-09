use crate::{
    boot_sector::BootSector,
    cluster::{ClusterChainOptions, reader::ClusterChainReader},
    disk::ReadOffset,
    entry::{DirEntry, StreamExtensionEntry, parsed::ParsedFileEntry, reader::DirEntryReader},
    error::DirectoryError,
    fat::Fat,
    timestamp::Timestamps,
};
use alloc::string::String;
use alloc::sync::Arc;

use super::{FsElement, file::File};

/// Represents a directory in an exFAT filesystem.
pub struct Directory<O> {
    disk: Arc<O>,
    boot: Arc<BootSector>,
    fat: Arc<Fat>,
    name: String,
    stream: StreamExtensionEntry,
    timestamps: Timestamps,
}

type Type = BootSector;

impl<O> Directory<O> {
    pub(crate) fn new(
        disk: Arc<O>,
        boot: Arc<Type>,
        fat: Arc<Fat>,
        name: String,
        stream: StreamExtensionEntry,
        timestamps: Timestamps,
    ) -> Self {
        Self {
            disk,
            boot,
            fat,
            name,
            stream,
            timestamps,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    pub fn timestamps(&self) -> &Timestamps {
        &self.timestamps
    }
}

impl<O: ReadOffset> Directory<O> {
    pub fn open(&self) -> Result<Vec<FsElement<O>>, DirectoryError<O>>
    where
        O::Err: core::fmt::Debug,
    {
        let options = if self.stream.general_secondary_flags.no_fat_chain() {
            ClusterChainOptions::Contiguous {
                data_length: self.stream.data_len,
            }
        } else {
            ClusterChainOptions::Fat {
                data_length: Some(self.stream.data_len),
            }
        };

        let mut reader = DirEntryReader::from(ClusterChainReader::try_new(
            Arc::clone(&self.boot),
            &self.fat,
            self.stream.first_cluster,
            options,
            Arc::clone(&self.disk),
        )?);

        // Read file entries.
        let mut items: Vec<FsElement<O>> = Vec::new();

        loop {
            // read primary entry
            let entry = reader.read()?;

            // unused entries are ignored
            if entry.unused() {
                continue;
            }

            // check for validity of dir entry
            if !entry.regular() {
                break;
            } else if !entry.primary() {
                return Err(DirectoryError::NotPrimaryEntry(entry.entry_type()));
            }

            let DirEntry::File(entry) = entry else {
                return Err(DirectoryError::NotFileEntry(entry.entry_type()));
            };

            // parse file entry
            let parsed = ParsedFileEntry::try_new(&entry, &mut reader)?;
            let item = if entry.file_attributes.is_directory() {
                FsElement::D(Directory::new(
                    Arc::clone(&self.disk),
                    Arc::clone(&self.boot),
                    Arc::clone(&self.fat),
                    parsed.name,
                    parsed.stream_extension_entry,
                    parsed.timestamps,
                ))
            } else {
                FsElement::F(File::try_new(
                    &self.disk,
                    &self.boot,
                    &self.fat,
                    parsed.name,
                    parsed.stream_extension_entry,
                    parsed.timestamps,
                )?)
            };
            items.push(item);
        }

        Ok(items)
    }
}
