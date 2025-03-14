//! # exFAT
//!
//! exFAT filesystem formatting in Rust.
//!
//! ## Usage
//!
//! ```rust
//! use exfat::{
//!    MB,
//!    format::{Exfat, FormatVolumeOptionsBuilder, Label},
//! };
//!
//! let size: u64 = 32 * MB as u64;
//! let hello_label = Label::new("Hello".to_string()).unwrap();
//!
//! let format_options = FormatVolumeOptionsBuilder::default()
//!     .pack_bitmap(false)
//!     .full_format(false)
//!     .dev_size(size)
//!     .label(hello_label)
//!     .bytes_per_sector(512)
//!     .build()
//!     .unwrap();
//!
//! let mut formatter = Exfat::try_from(format_options).unwrap();
//!
//!
//! # let mut file = std::io::Cursor::new(vec![0u8; size as usize]);
//!
//!
//! formatter.write(&mut file).unwrap();
//! ```
//!
//! ## Limitations
//! Currently, the crate can only be used to format, but not read/write to the fs. no-std support
//! is also a work-in-progress.

/// Directory abstractions
pub(crate) mod dir;
/// Disk utility functions
pub mod disk;
pub mod error;
/// Filesystem formatting capabilities
pub mod format;

pub const GB: u32 = 1024 * 1024 * 1024;
pub const MB: u32 = 1024 * 1024;
pub const KB: u16 = 1024;

pub const DEFAULT_BOUNDARY_ALIGNEMENT: u32 = 1024 * 1024;
