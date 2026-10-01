//! Application-owned storage. Source log directories are never write targets.

use std::{io, path::Path};

pub fn prepare_data_directory(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}
