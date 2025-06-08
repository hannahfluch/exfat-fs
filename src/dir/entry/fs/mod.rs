use directory::Directory;
use file::File;

use crate::disk::{self};

pub(crate) mod directory;
pub(crate) mod file;

pub enum FsElement<O: disk::ReadOffset> {
    F(File<O>),
    D(Directory<O>),
}
