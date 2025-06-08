use directory::Directory;
use file::File;

use crate::disk::{self};

pub mod directory;
pub mod file;

pub enum FsElement<O: disk::ReadOffset> {
    F(File<O>),
    D(Directory<O>),
}
