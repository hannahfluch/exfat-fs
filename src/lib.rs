//! # exFAT-fs
//!
//! exFAT filesystem implementation in Rust.
//!
//! ## Features
//! - exFAT formatting
//! - `no-std` support
//! - reading
//!
//! ## Usage
//!
//! ### Formatting
//! ```rust
//! use exfat_fs::{
//!    MB,
//!    Label,
//!    format::{Exfat, FormatVolumeOptionsBuilder},
//! };
//!
//! use std::{io::Cursor, time::SystemTime};
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
//! let mut formatter = Exfat::try_from::<SystemTime>(format_options).unwrap();
//!
//!
//! let mut file = Cursor::new(vec![0u8; size as usize]);
//!
//!
//! formatter.write::<SystemTime, Cursor<Vec<u8>>>(&mut file).unwrap();
//! ```
//!
//! ### Reading
//! ```no_run
//! use exfat_fs::{root::Root, fs::FsElement};
//! use std::{fs::OpenOptions, io::Read};
//!
//! # let file = OpenOptions::new().read(true).open("exfat_vol").unwrap();
//!
//! // Load root directory
//! let mut root = Root::open(file).unwrap();
//!
//! // Get contents of first element (file)
//! if let FsElement::F(ref mut file) = root.items()[0] {
//!     let mut buffer = String::default();
//!     file.read_to_string(&mut buffer).unwrap();
//!     println!("Contents of file: {buffer}");
//! }
//! ```
//!
//! ## Limitations
//! Writing is currently not supported (WIP).
#![cfg_attr(not(any(feature = "std", test)), no_std)]

#[cfg(any(feature = "std", test))]
extern crate std;

extern crate alloc;

use alloc::{string::String, vec::Vec};
pub(crate) mod boot_sector;
/// Cluster I/O
pub(crate) mod cluster;
/// Disk utility functions
pub mod disk;
/// Internal directory abstractions
pub(crate) mod entry;
pub mod error;
pub(crate) mod fat;
/// Filesystem formatting capabilities
pub mod format;
/// Filesystem abstractions
pub mod fs;
pub mod root;
pub mod timestamp;

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
impl core::fmt::Display for Label {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut converted = [0u16; 11];

        for (i, chunk) in self.0[..self.1 as usize * 2].chunks_exact(2).enumerate() {
            converted[i] = u16::from_ne_bytes([chunk[0], chunk[1]]);
        }

        match String::from_utf16(&converted) {
            Ok(s) => write!(f, "{}", s),
            Err(_) => write!(f, "<invalid utf16>"),
        }
    }
}
