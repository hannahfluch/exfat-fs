use alloc::sync::Arc;

use crate::{
    dir::{BootSector, ClusterChainOptions, ClusterChainReader, Fat, entry::StreamExtensionEntry},
    disk,
    error::RootError,
    timestamp::Timestamps,
};

pub struct File<O: disk::ReadOffset> {
    name: String,
    len: u64,
    reader: Option<ClusterChainReader<O>>,
    timestamps: Timestamps,
}
impl<O: disk::ReadOffset> File<O> {
    pub(crate) fn try_new(
        disk: Arc<O>,
        boot: Arc<BootSector>,
        fat: &Fat,
        name: String,
        stream: StreamExtensionEntry,
        timestamps: Timestamps,
    ) -> Result<Self, RootError<O>> {
        // create a cluster reader
        let first_cluster = stream.first_cluster;
        let len = stream.valid_data_length;
        let reader = if first_cluster == 0 {
            None
        } else {
            let options = if stream.general_secondary_flags.no_fat_chain() {
                ClusterChainOptions::Contiguous { data_length: len }
            } else {
                ClusterChainOptions::Fat {
                    data_length: Some(len),
                }
            };
            Some(ClusterChainReader::try_new(
                boot,
                fat,
                first_cluster,
                options,
                disk,
            )?)
        };

        Ok(Self {
            name,
            len,
            reader,
            timestamps,
        })
    }

    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn timestamps(&self) -> &Timestamps {
        &self.timestamps
    }
}
