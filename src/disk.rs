use std::io::{self, ErrorKind, Read, Seek, Write};

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
}

pub trait ReadOffset {
    type ReadOffsetError: PartitionError;

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize, Self::ReadOffsetError>;

    fn read_exact(
        &mut self,
        mut offset: u64,
        mut buffer: &mut [u8],
    ) -> Result<(), Self::ReadOffsetError> {
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
}

impl ReadOffset for std::fs::File {
    type ReadOffsetError = io::Error;
    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize, Self::ReadOffsetError> {
        self.seek(io::SeekFrom::Start(offset))?;
        self.read(buffer)
    }
}
