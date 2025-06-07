use core::ops::Deref;
use std::{
    io::{self, ErrorKind, Seek, Write},
    sync::Arc,
};

/// Writes zeroes to a file from the given absolute offset (in bytes), up to the given size.
pub fn write_zeroes<T>(f: &mut T, size: u64, offset: u64) -> io::Result<()>
where
    T: Write + Seek,
{
    let buffer = [0u8; 4 * crate::KB as usize];

    // seek to offset
    f.seek(io::SeekFrom::Start(offset))?;

    let mut remaining = size;
    while remaining > 0 {
        let iter_size = remaining.min(buffer.len() as u64);
        // `iter_size` is max 4KB so this cast is fine
        if f.write(&buffer[..iter_size as usize])? != iter_size as usize {
            return Err(io::Error::new(ErrorKind::WriteZero, "Failed to write 0s"));
        }
        remaining -= iter_size;
    }
    Ok(())
}

pub trait PartitionError {
    fn unexpected_eop() -> Self;

    fn cluster_not_found(cluster: u32) -> Self;
}

pub trait ReadOffset {
    type Err: PartitionError + 'static;

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<usize, Self::Err>;

    fn read_exact(&self, mut offset: u64, mut buffer: &mut [u8]) -> Result<(), Self::Err> {
        while !buffer.is_empty() {
            match self.read_at(offset, buffer) {
                Ok(0) => break,
                Ok(n) => {
                    buffer = &mut buffer[n..];
                    offset = offset
                        .checked_add(n as u64)
                        .ok_or(PartitionError::unexpected_eop())?;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

impl PartitionError for io::Error {
    fn unexpected_eop() -> Self {
        io::Error::from(io::ErrorKind::UnexpectedEof)
    }

    fn cluster_not_found(cluster: u32) -> Self {
        io::Error::new(
            ErrorKind::Other,
            format!("cluster #{cluster} is not available"),
        )
    }
}

impl<T: ReadOffset> ReadOffset for &T {
    type Err = T::Err;

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, Self::Err> {
        (*self).read_at(offset, buf)
    }
}
impl<T: ReadOffset> ReadOffset for Arc<T> {
    type Err = T::Err;

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, Self::Err> {
        self.deref().read_at(offset, buf)
    }
}
impl ReadOffset for std::fs::File {
    type Err = std::io::Error;

    #[cfg(unix)]
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, Self::Err> {
        std::os::unix::fs::FileExt::read_at(self, buf, offset)
    }

    #[cfg(windows)]
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, Self::Err> {
        std::os::windows::fs::FileExt::seek_read(self, buf, offset)
    }
}
