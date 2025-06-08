use core::ops::Deref;

use alloc::sync::Arc;
/// Writes zeroes to a file from the given absolute offset (in bytes), up to the given size.
pub fn write_zeroes<T>(f: &mut T, size: u64, offset: u64) -> Result<(), T::Err>
where
    T: WriteSeek,
{
    let buffer = [0u8; 4 * crate::KB as usize];

    // seek to offset
    f.seek(SeekFrom::Start(offset))?;

    let mut remaining = size;
    while remaining > 0 {
        let iter_size = remaining.min(buffer.len() as u64);
        // `iter_size` is max 4KB so this cast is fine
        if f.write(&buffer[..iter_size as usize])? != iter_size as usize {
            return Err(f.failed_to_write());
        }
        remaining -= iter_size;
    }
    Ok(())
}

pub trait WriteSeek {
    type Err;
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Err>;
    fn failed_to_write(&self) -> Self::Err;
    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Err>;
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Err>;
    fn stream_position(&mut self) -> Result<u64, Self::Err>;
}
#[cfg(feature = "std")]
impl<T> WriteSeek for T
where
    T: std::io::Write + std::io::Seek,
{
    type Err = std::io::Error;

    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Err> {
        std::io::Write::write(self, buf)
    }
    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Err> {
        std::io::Write::write_all(self, buf)
    }
    fn failed_to_write(&self) -> Self::Err {
        Self::Err::new(std::io::ErrorKind::WriteZero, "Failed to write 0s")
    }
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Err> {
        std::io::Seek::seek(self, pos.into())
    }
    fn stream_position(&mut self) -> Result<u64, Self::Err> {
        std::io::Seek::stream_position(self)
    }
}

pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

#[cfg(feature = "std")]
impl From<SeekFrom> for std::io::SeekFrom {
    fn from(value: SeekFrom) -> Self {
        match value {
            SeekFrom::Start(x) => std::io::SeekFrom::Start(x),
            SeekFrom::End(x) => std::io::SeekFrom::End(x),
            SeekFrom::Current(x) => std::io::SeekFrom::Current(x),
        }
    }
}

pub trait PartitionError: core::fmt::Debug {
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

#[cfg(feature = "std")]
impl PartitionError for std::io::Error {
    fn unexpected_eop() -> Self {
        std::io::Error::from(std::io::ErrorKind::UnexpectedEof)
    }

    fn cluster_not_found(cluster: u32) -> Self {
        std::io::Error::new(
            std::io::ErrorKind::Other,
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
#[cfg(feature = "std")]
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
