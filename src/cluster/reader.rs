use alloc::vec::Vec;

use crate::{
    boot_sector::BootSector,
    disk::{PartitionError, ReadOffset},
    error::ClusterChainError,
    fat::{ClusterChain, Fat},
};

use super::ClusterChainOptions;

#[derive(Debug, Clone)]
pub(crate) struct ClusterChainReader<O, B> {
    boot: B,
    chain: Vec<u32>,
    data_length: u64,
    offset: u64,
    disk: O,
}

impl<O, B: AsRef<BootSector>> ClusterChainReader<O, B> {
    pub(crate) fn data_length(&self) -> u64 {
        self.data_length
    }
    pub(crate) fn seek(&mut self, off: u64) -> bool {
        if off > self.data_length {
            return false;
        }

        self.offset = off;
        true
    }

    pub(crate) fn rewind(&mut self) {
        self.offset = 0;
    }

    pub(crate) fn stream_position(&self) -> u64 {
        self.offset
    }
}

impl<O, B: AsRef<BootSector>> ClusterChainReader<O, B> {
    pub(crate) fn try_new(
        boot: B,
        fat: &Fat,
        first_cluster: u32,
        options: ClusterChainOptions,
        disk: O,
    ) -> Result<Self, ClusterChainError> {
        assert!(
            boot.as_ref().first_cluster_of_root_directory >= 2
                && boot.as_ref().first_cluster_of_root_directory <= boot.as_ref().cluster_count + 1,
            "Invalid Root Cluster Index"
        );

        let cluster_size_bytes = boot.as_ref().bytes_per_cluster() as u64;

        let (chain, data_length) = match options {
            ClusterChainOptions::Contiguous { data_length } => {
                let count = data_length.div_ceil(cluster_size_bytes);

                if let Ok(count) = u32::try_from(count) {
                    let chain: Vec<u32> = (first_cluster..(first_cluster + count)).collect();

                    (chain, data_length)
                } else {
                    todo!(
                        "ExFat directory size is limited, if the `NoFATBit` is set. TODO: Check if this is the right behavior."
                    );
                }
            }
            ClusterChainOptions::Fat { data_length } => {
                let chain: Vec<u32> = ClusterChain::new(fat, first_cluster).collect();
                if chain.is_empty() {
                    return Err(ClusterChainError::InvalidFirstCluster);
                }

                let data_length = data_length.unwrap_or(
                    boot.as_ref().bytes_per_sector() as u64
                        * boot.as_ref().sectors_per_cluster() as u64
                        * chain.len() as u64,
                );

                if data_length > cluster_size_bytes * chain.len() as u64 {
                    return Err(ClusterChainError::InvalidDataLength);
                }

                (chain, data_length)
            }
        };

        Ok(Self {
            boot,
            chain,
            data_length,
            offset: 0,
            disk,
        })
    }
    pub fn current(&self) -> u32 {
        self.chain[(self.offset / self.boot.as_ref().bytes_per_cluster() as u64) as usize]
    }
}

impl<O: ReadOffset, B: AsRef<BootSector>> ClusterChainReader<O, B> {
    pub(crate) fn read(&mut self, buf: &mut [u8]) -> Result<usize, O::Err> {
        // Check if the actual read is required.
        if buf.is_empty() || self.offset == self.data_length {
            return Ok(0);
        }

        // Get remaining data in the current cluster.
        let boot = &self.boot;
        let cluster_size = boot.as_ref().bytes_per_cluster() as u64;
        let cluster_remaining = cluster_size - self.offset % cluster_size;
        let remaining = cluster_remaining.min(self.data_length - self.offset);

        // Get the offset in the partition.
        let cluster = self.chain[(self.offset / cluster_size) as usize];
        let offset = boot
            .as_ref()
            .cluster_offset(cluster)
            .ok_or(PartitionError::cluster_not_found(cluster))?
            + self.offset % cluster_size;

        // Read the image
        let amount = buf.len().min(remaining as usize);

        self.disk.read_exact(offset, &mut buf[..amount])?;

        self.offset += amount as u64;
        Ok(amount)
    }
    pub fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<(), O::Err> {
        while !buf.is_empty() {
            let n = self.read(buf)?;

            if n == 0 {
                return Err(O::Err::unexpected_eop());
            }

            buf = &mut buf[n..];
        }

        Ok(())
    }
}
