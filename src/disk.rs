use std::io::{self, ErrorKind, Seek, Write};

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
