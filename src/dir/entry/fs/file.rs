use alloc::sync::Arc;

use crate::{
    dir::{BootSector, ClusterChainOptions, ClusterChainReader, Fat, entry::StreamExtensionEntry},
    disk::{self, ReadOffset},
    error::RootError,
    timestamp::Timestamps,
};

#[derive(Clone)]
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

#[cfg(feature = "std")]
impl<O: disk::ReadOffset> std::io::Seek for File<O> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        use std::io::{Error, ErrorKind, SeekFrom};

        let Some(r) = &mut self.reader else {
            return std::io::empty().seek(pos);
        };

        // get absolute offset.
        let o = match pos {
            SeekFrom::Start(v) => v.min(r.data_length()),
            SeekFrom::End(v) => {
                if v >= 0 {
                    r.data_length()
                } else if let Some(v) = r.data_length().checked_sub(v.unsigned_abs()) {
                    v
                } else {
                    return Err(Error::from(ErrorKind::InvalidInput));
                }
            }
            SeekFrom::Current(v) => v.try_into().map_or_else(
                |_| {
                    r.stream_position()
                        .checked_sub(v.unsigned_abs())
                        .ok_or_else(|| Error::from(ErrorKind::InvalidInput))
                },
                |v| Ok(r.stream_position().saturating_add(v).min(r.data_length())),
            )?,
        };

        assert!(r.seek(o));

        Ok(o)
    }

    fn rewind(&mut self) -> std::io::Result<()> {
        let r = match &mut self.reader {
            Some(v) => v,
            None => return Ok(()),
        };

        r.rewind();

        Ok(())
    }

    fn stream_position(&mut self) -> std::io::Result<u64> {
        let r = match &mut self.reader {
            Some(v) => v,
            None => return Ok(0),
        };

        Ok(r.stream_position())
    }
}

#[cfg(feature = "std")]
impl<D: ReadOffset> std::io::Read for File<D>
where
    D::Err: Into<std::io::Error>,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        println!("rading!");
        match &mut self.reader {
            Some(v) => v.read(buf).map_err(Into::into),
            None => Ok(0),
        }
    }
}
