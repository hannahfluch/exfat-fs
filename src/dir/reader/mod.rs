use cluster::ClusterChainReader;

use crate::{disk::ReadOffset, error::EntryReaderError};

use super::DirEntry;

pub(crate) mod cluster;

/// Directory Entry Reader
pub(crate) struct DirEntryReader<O: ReadOffset> {
    cluster_reader: ClusterChainReader<O>,
    index: usize,
}

impl<O: ReadOffset> From<ClusterChainReader<O>> for DirEntryReader<O> {
    fn from(value: ClusterChainReader<O>) -> Self {
        DirEntryReader {
            cluster_reader: value,
            index: 0,
        }
    }
}

impl<O: ReadOffset> DirEntryReader<O> {
    pub(crate) fn read(&mut self) -> Result<DirEntry, EntryReaderError<O>> {
        // Get current cluster and entry index.
        let cluster = self.cluster_reader.current();
        let index = self.index;

        // Read directory entry.
        let mut entry = [0u8; 32];

        if let Err(e) = self.cluster_reader.read_exact(&mut entry) {
            return Err(EntryReaderError::ReadFailed(index, cluster, e));
        }

        // Update entry index
        if self.cluster_reader.current() != cluster {
            self.index = 0;
        } else {
            self.index += 1;
        }

        DirEntry::try_from(entry).map_err(|err| err.into())
    }
}
