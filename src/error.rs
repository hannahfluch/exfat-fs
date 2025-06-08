use crate::{
    boot_sector::UnixEpochDuration,
    disk::{ReadOffset, WriteSeek},
};
use alloc::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum ExfatFormatError<T: UnixEpochDuration> {
    #[error("Invalid bytes per sector. Must be a power of `2` and between `512` and `4096`: {0}.")]
    InvalidBytesPerSector(u16),
    #[error("Invalid volume size: {0}.")]
    InvalidSize(u64),
    #[error("Invalid partition offset: {0}.")]
    InvalidPartitionOffset(u64),
    #[error("Invalid number of FATs (must be 1 or 2): {0}.")]
    InvalidNumberOfFats(u8),
    #[error("Invalid cluster size: {0}. Must be a power of `2` and at most 32MB: {0}")]
    InvlaidClusterSize(u32),
    #[error("Boundary alignment is too big: {0}")]
    BoundaryAlignemntTooBig(u32),
    #[error("Unable to generate unique serial number. Error: {0}")]
    NoSerial(#[source] T::Err),
    #[error("Unable to pack bitmap.")]
    CannotPackBitmap,
    #[error("File size does not match exFAT size.")]
    InvalidFileSize,
}

#[derive(Debug, thiserror::Error)]
pub enum ExfatError<T: UnixEpochDuration, O: WriteSeek>
where
    T::Err: core::fmt::Debug,
{
    #[error("{0}")]
    Format(#[from] ExfatFormatError<T>),
    #[error("I/O error: {0}.")]
    Io(#[source] O::Err),
}

#[derive(Debug, thiserror::Error)]
pub enum RootError<O: ReadOffset> {
    #[error("I/O error: {0}.")]
    Io(O::Err),
    #[error("The provided volume is not an exFAT filesystem.")]
    WrongFs,
    #[error("Invalid bytes per sector shift detected: {0}. Must be between `9` and `12`")]
    InvalidBytesPerSectorShift(u8),
    #[error("Invalid sectors per cluster shift detected: {0}.")]
    InvalidSectorsPerClusterShift(u8),
    #[error("Invalid number of FATs detected: {0}. Must be either `1` or `2`.")]
    InvalidNumberOfFats(u8),
    #[error("Fat could not be parsed: {0}.")]
    Fat(#[from] FatLoadError<Arc<O>>),
    #[error(
        "Invalid index of root directory cluster detected: {0}. Must be bigger than `2` and at most `cluster_count + 1`"
    )]
    InvalidRootDirectoryClusterIndex(u32),
    #[error("Cluster chain could not be parsed: {0}.")]
    ClusterChain(#[from] ClusterChainError),
    #[error("Entry Reader Error: {0}.")]
    DirEntry(#[from] EntryReaderError<Arc<O>>),
    #[error(
        "All directory entries of the root directory must be of type `PRIMARY`. Detected entry type: {0}"
    )]
    RootEntryNotPrimary(u8),
    #[error("More than 2 allocation bitmap root entry fields detected.")]
    InvalidNumberOfAllocationBitmaps,
    #[error("Corrupt allocation bitmap entry.")]
    InvalidAllocationBitmap,
    #[error("More than 1 upcase table root entry field detected.")]
    InvalidNumberOfUpcaseTables,
    #[error("Corrupt upcase table entry.")]
    InvalidUpcaseTable,
    #[error("More than 1 volume label root entry field detected.")]
    InvalidNumberOfVolumeLabels,
    #[error("Corrupt volume label entry.")]
    InvalidVolumeLabel,
    #[error("Unable to parse file entry: {0}")]
    InvalidFileEntry(#[from] FileParserError<Arc<O>>),
    #[error("Unexpected directory entry in root directory. Detected entry type: {0}")]
    UnexpectedRootEntry(u8),
}

#[derive(Debug, thiserror::Error)]
pub enum FatLoadError<O: ReadOffset> {
    #[error("FAT starts at invalid offset.")]
    InvalidOffset,
    #[error("Read failed at: {0:#x}.")]
    ReadFailed(u64, #[source] O::Err),
}

#[derive(Debug, thiserror::Error)]
pub enum ClusterChainError {
    #[error("Invalid starting cluster.")]
    InvalidFirstCluster,
    #[error("Invalid data length for cluster chain.")]
    InvalidDataLength,
}

#[derive(Debug, thiserror::Error)]
pub enum EntryReaderError<O: ReadOffset> {
    #[error("Cannot read entry #{0} on cluster #{1}.")]
    ReadFailed(usize, u32, #[source] O::Err),
    #[error("{0}")]
    Entry(#[from] DirEntryError),
}

#[derive(Debug, thiserror::Error)]
pub enum DirEntryError {
    #[error("Invalid directory entry detected: {0}.")]
    InvalidEntry(u8),
}

#[derive(Debug, thiserror::Error)]
pub enum FileParserError<O: ReadOffset>
where
    O::Err: core::fmt::Debug,
{
    #[error("File entry is missing stream extension.")]
    NoStreamExtension,
    #[error("File entry is missing file name.")]
    NoFileName,
    #[error("{0}")]
    ReadFailed(#[from] EntryReaderError<O>),
    #[error("Invalid stream extension entry detected.")]
    InvalidStreamExtension,
    #[error("Wrong number of file name entries detected.")]
    WrongFileNameEntries,
    #[error("Invalid file name entry detected.")]
    InvalidFileName,
}

#[derive(Debug, thiserror::Error)]
pub enum DirectoryError<O: ReadOffset>
where
    O::Err: core::fmt::Debug,
{
    #[error("Cannot create a clusters reader for allocation: {0}")]
    CreateClustersReaderFailed(#[from] ClusterChainError),
    #[error("Cannot read an entry: {0}")]
    ReadEntryFailed(#[from] EntryReaderError<Arc<O>>),
    #[error("Detected directory entry that is not `PRIMARY`. Detected entry type: {0}")]
    NotPrimaryEntry(u8),
    #[error("Detected directory entry that is not a file entry. Detected entry type: {0}")]
    NotFileEntry(u8),
    #[error("Unable to parse file entry: {0}")]
    InvalidFileEntry(#[from] FileParserError<Arc<O>>),
}
