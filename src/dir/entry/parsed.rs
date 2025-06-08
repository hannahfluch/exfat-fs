use crate::{
    dir::entry::ClusterAllocation,
    disk::ReadOffset,
    error::RootError,
    timestamp::{Timestamp, Timestamps},
};

use super::{DirEntry, DirEntryReader, FileAttributes, FileEntry, StreamExtensionEntry};

#[derive(Clone, Debug)]
pub(crate) struct ParsedFileEntry {
    pub(crate) name: String,
    pub(crate) attributes: FileAttributes,
    pub(crate) stream_extension_entry: StreamExtensionEntry,
    pub(crate) timestamps: Timestamps,
}

impl ParsedFileEntry {
    pub(crate) fn try_new<R: ReadOffset + core::fmt::Debug>(
        file_entry: &FileEntry,
        reader: &mut DirEntryReader<R>,
    ) -> Result<ParsedFileEntry, RootError<R>> {
        let secondary_count = file_entry.secondary_count;
        if secondary_count < 1 {
            return Err(RootError::NoStreamExtension);
        } else if secondary_count < 2 {
            return Err(RootError::NoFileName);
        }

        // parse stream extension entry afterward
        let stream_extension = reader.read()?;

        let stream_extension_entry = if let DirEntry::StreamExtension(stream_extension_entry) =
            stream_extension
        {
            if !stream_extension_entry.valid()
                || file_entry.file_attributes.is_directory()
                    && stream_extension_entry.valid_data_length != stream_extension_entry.data_len
            {
                return Err(RootError::InvalidStreamExtension);
            }
            stream_extension_entry
        } else {
            return Err(RootError::NoStreamExtension);
        };

        // read file names
        let name_count = secondary_count - 1;
        let mut names = Vec::with_capacity(name_count as usize);

        for _ in 0..name_count {
            // parse file name entry
            let file_name = reader.read()?;
            if let DirEntry::FileName(file_name_entry) = file_name {
                names.push(file_name_entry);
            } else {
                return Err(RootError::NoFileName);
            }
        }
        if names.len() != stream_extension_entry.name_length.div_ceil(15) as usize {
            return Err(RootError::WrongFileNameEntries);
        }
        // construct a filename
        let mut byte_len = 2 * stream_extension_entry.name_length as usize;
        let mut name = String::with_capacity(15 * names.len());

        for entry in names {
            if entry.general_secondary_flags.allocation_possible() {
                return Err(RootError::InvalidFileName);
            }

            // load name
            let raw_name = &entry.file_name[..30.min(byte_len)];
            if raw_name.len() % 2 != 0 {
                return Err(RootError::InvalidFileName);
            }

            byte_len -= raw_name.len();

            // convert to native endian
            let mut file_name = [0u16; 15];
            let file_name = &mut file_name[..(raw_name.len() / 2)];

            for (i, chunk) in raw_name.chunks_exact(2).enumerate() {
                file_name[i] = u16::from_le_bytes([chunk[0], chunk[1]]);
            }
            match String::from_utf16(file_name) {
                Ok(part) => name.push_str(&part),
                Err(_) => return Err(RootError::InvalidFileName),
            }
        }

        // read timestamps
        let create_utc_offset = if ((file_entry.create_utc_offset >> 7) & 1) == 1 {
            (file_entry.create_utc_offset & 0x7F) as i8
        } else {
            0
        };
        let last_modified_utc_offset = if ((file_entry.last_modified_utc_offset >> 7) & 1) == 1 {
            (file_entry.last_modified_utc_offset & 0x7F) as i8
        } else {
            0
        };
        let last_accessed_utc_offset = if ((file_entry.last_accessed_utc_offset >> 7) & 1) == 1 {
            (file_entry.last_accessed_utc_offset & 0x7F) as i8
        } else {
            0
        };

        Ok(ParsedFileEntry {
            name,
            stream_extension_entry,
            attributes: file_entry.file_attributes,
            timestamps: Timestamps::new(
                Timestamp::new(
                    file_entry.create_timestamp,
                    file_entry.create_10ms_increment,
                    create_utc_offset,
                ),
                Timestamp::new(
                    file_entry.last_modified_timestamp,
                    file_entry.last_modified_10ms_increment,
                    last_modified_utc_offset,
                ),
                Timestamp::new(
                    file_entry.last_accessed_timestamp,
                    0,
                    last_accessed_utc_offset,
                ),
            ),
        })
    }
}
