//! # exFAT-fs
//!
//! exFAT filesystem implementation in Rust.
//!
//! ## Usage
//!
//! ```rust
//! use exfat_fs::{
//!    MB,
//!    Label,
//!    format::{Exfat, FormatVolumeOptionsBuilder},
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

pub(crate) mod boot_sector;
/// Directory abstractions
pub mod dir;
/// Disk utility functions
pub mod disk;
pub mod error;
/// Filesystem formatting capabilities
pub mod format;
pub(crate) mod upcase_table;

pub const GB: u32 = 1024 * 1024 * 1024;
pub const MB: u32 = 1024 * 1024;
pub const KB: u16 = 1024;

pub const DEFAULT_BOUNDARY_ALIGNEMENT: u32 = 1024 * 1024;
/// First usable cluster index of the cluster heap
pub(crate) const FIRST_USABLE_CLUSTER_INDEX: u32 = 2;

/// A UTF16 encoded volume label. The length must not exceed 11 characters.
#[derive(Copy, Clone, Debug, Default)]
pub struct Label(pub(crate) [u8; 22], pub(crate) u8);

impl Label {
    pub fn new(label: String) -> Option<Label> {
        let len = label.len();
        if len > 11 {
            None
        } else {
            let mut utf16_bytes = [0u8; 22];

            let encoded: Vec<u8> = label.encode_utf16().flat_map(|x| x.to_le_bytes()).collect();

            let copy_len = encoded.len();
            assert!(copy_len <= 22);
            utf16_bytes[..copy_len].copy_from_slice(&encoded[..copy_len]);

            Some(Label(utf16_bytes, len as u8))
        }
    }
}
