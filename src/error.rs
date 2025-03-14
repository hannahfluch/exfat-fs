use std::{io, time::SystemTimeError};

#[derive(Debug, thiserror::Error)]
pub enum ExfatFormatError {
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
    NoSerial(#[from] SystemTimeError),
    #[error("Unable to pack bitmap.")]
    CannotPackBitmap,
    #[error("File size does not match exFAT size.")]
    InvalidFileSize,
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}
