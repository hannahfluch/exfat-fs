pub mod dir;
pub mod disk;
pub mod error;
pub mod format;

pub const GB: u32 = 1024 * 1024 * 1024;
pub const MB: u32 = 1024 * 1024;
pub const KB: u16 = 1024;

pub const DEFAULT_BOUNDARY_ALIGNEMENT: u32 = 1024 * 1024;
