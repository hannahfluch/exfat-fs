use crate::{
    dir::{BootSector, Fat, entry::StreamExtensionEntry},
    disk::{self},
    timestamp::Timestamps,
};
use alloc::string::String;
use alloc::sync::Arc;

/// Represents a directory in an exFAT filesystem.
pub struct Directory<O: disk::ReadOffset> {
    disk: Arc<O>,
    boot: Arc<BootSector>,
    fat: Arc<Fat>,
    name: String,
    stream: StreamExtensionEntry,
    timestamps: Timestamps,
}

impl<O: disk::ReadOffset> Directory<O> {
    pub(crate) fn new(
        disk: Arc<O>,
        boot: Arc<BootSector>,
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
