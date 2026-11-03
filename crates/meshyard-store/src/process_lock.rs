use crate::StoreError;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
};

pub(crate) fn private_directory(path: &Path) -> Result<(), StoreError> {
    if !path.exists() {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(path).map_err(|_| StoreError::Io)?;
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| StoreError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StoreError::Permissions);
    }
    #[cfg(unix)]
    if metadata.mode() & 0o077 != 0 {
        return Err(StoreError::Permissions);
    }
    Ok(())
}
pub(crate) fn nofollow(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        options.custom_flags(libc::O_NOFOLLOW).mode(0o600);
    }
}
pub(crate) fn private_file(file: &File, parent: &Path) -> Result<(), StoreError> {
    let metadata = file.metadata().map_err(|_| StoreError::Io)?;
    if !metadata.is_file() {
        return Err(StoreError::Permissions);
    }
    #[cfg(unix)]
    {
        let dir = fs::metadata(parent).map_err(|_| StoreError::Io)?;
        if metadata.mode() & 0o777 != 0o600 || metadata.uid() != dir.uid() || metadata.nlink() != 1
        {
            return Err(StoreError::Permissions);
        }
    }
    Ok(())
}
pub struct DataDirLock {
    _file: File,
}
impl DataDirLock {
    pub fn acquire(data_dir: &Path) -> Result<Self, StoreError> {
        private_directory(data_dir)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        nofollow(&mut options);
        let file = options
            .open(data_dir.join("process.lock"))
            .map_err(|_| StoreError::Permissions)?;
        private_file(&file, data_dir)?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => StoreError::Busy,
            std::fs::TryLockError::Error(_) => StoreError::Io,
        })?;
        Ok(Self { _file: file })
    }
}
