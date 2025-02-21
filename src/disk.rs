use std::io::{self, Seek, Write};

pub fn write_zero<T>(_f: &mut T, _size: u64) -> io::Result<()>
where
    T: Write + Seek,
{
    // 4KB
    let _buffer = [0u64; 512];

    todo!();
}
