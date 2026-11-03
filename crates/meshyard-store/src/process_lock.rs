use crate::StoreError;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

/// Canonical paths are used for every later operation. Validate from the root down:
/// each checked parent prevents a different OS user from replacing the next entry.
/// Root-owned sticky directories protect entries owned by root/the current UID.
/// Malicious same-UID processes and privileged administrators are outside this policy.
fn trusted_ancestors(path: &Path) -> Result<(), StoreError> {
    let mut chain: Vec<_> = path.ancestors().collect();
    chain.reverse();
    for ancestor in chain {
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| StoreError::Io)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(StoreError::Permissions);
        }
        #[cfg(unix)]
        if !trusted_owner_mode(metadata.uid(), metadata.mode(), current_uid()) {
            return Err(StoreError::Permissions);
        }
        no_extended_access(ancestor)?;
    }
    Ok(())
}
#[cfg(unix)]
fn current_uid() -> u32 {
    rustix::process::geteuid().as_raw()
}
#[cfg(unix)]
fn trusted_owner_mode(uid: u32, mode: u32, current: u32) -> bool {
    (uid == 0 || uid == current) && (mode & 0o022 == 0 || (uid == 0 && mode & 0o1000 != 0))
}
/// macOS ACLs can grant access independently of mode bits (including delete on
/// children of sticky directories). Conservatively reject every nonempty allow
/// entry, including inherited grants; deny-only ACLs do not widen access.
/// Linux POSIX access ACL grants are constrained by the group-class mode mask.
#[cfg(target_os = "macos")]
fn no_extended_access(path: &Path) -> Result<(), StoreError> {
    let acl = exacl::getfacl(path, None).map_err(|_| StoreError::Permissions)?;
    if acl
        .iter()
        .any(|entry| entry.allow && !entry.perms.is_empty())
    {
        return Err(StoreError::Permissions);
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn no_extended_access(_: &Path) -> Result<(), StoreError> {
    Ok(())
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn no_extended_access(_: &Path) -> Result<(), StoreError> {
    Err(StoreError::Permissions)
}

pub fn private_directory(path: &Path) -> Result<PathBuf, StoreError> {
    // Resolve aliases once, then inspect/use only this spelling. For a missing
    // leaf, validate its existing parent before creating anything.
    let canonical = match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(StoreError::Permissions);
            }
            fs::canonicalize(path).map_err(|_| StoreError::Io)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or(StoreError::Permissions)?;
            let parent = fs::canonicalize(parent).map_err(|_| StoreError::Io)?;
            trusted_ancestors(&parent)?;
            let canonical = parent.join(path.file_name().ok_or(StoreError::Permissions)?);
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            builder.mode(0o700);
            match builder.create(&canonical) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(StoreError::Io),
            }
            canonical
        }
        Err(_) => return Err(StoreError::Io),
    };
    trusted_ancestors(&canonical)?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|_| StoreError::Io)?;
    #[cfg(unix)]
    if metadata.uid() != current_uid() || metadata.mode() & 0o077 != 0 {
        return Err(StoreError::Permissions);
    }
    Ok(canonical)
}
pub(crate) fn nofollow(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        options.custom_flags(libc::O_NOFOLLOW).mode(0o600);
    }
}
pub(crate) fn private_file(file: &File, path: &Path) -> Result<(), StoreError> {
    let metadata = file.metadata().map_err(|_| StoreError::Io)?;
    if !metadata.is_file() {
        return Err(StoreError::Permissions);
    }
    #[cfg(unix)]
    if metadata.mode() & 0o777 != 0o600 || metadata.uid() != current_uid() || metadata.nlink() != 1
    {
        return Err(StoreError::Permissions);
    }
    no_extended_access(path)?;
    Ok(())
}
/// SQLite may have created older files with 0644 inside the private directory.
/// Tighten only a current-UID, regular, singly linked inode, never an alias.
pub(crate) fn sqlite_file(path: &Path, create: bool) -> Result<(), StoreError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(create)
        .truncate(false);
    nofollow(&mut options);
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if !create && error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(StoreError::Permissions),
    };
    let metadata = file.metadata().map_err(|_| StoreError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if !metadata.is_file() || metadata.uid() != current_uid() || metadata.nlink() != 1 {
            return Err(StoreError::Permissions);
        }
        no_extended_access(path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| StoreError::Io)?;
    }
    private_file(&file, path)
}
pub struct DataDirLock {
    _file: File,
    directory: PathBuf,
}
impl DataDirLock {
    pub fn acquire(data_dir: &Path) -> Result<Self, StoreError> {
        Self::acquire_in(private_directory(data_dir)?)
    }
    fn acquire_in(directory: PathBuf) -> Result<Self, StoreError> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        nofollow(&mut options);
        let path = directory.join("process.lock");
        let file = options.open(&path).map_err(|_| StoreError::Permissions)?;
        private_file(&file, &path)?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => StoreError::Busy,
            std::fs::TryLockError::Error(_) => StoreError::Io,
        })?;
        Ok(Self {
            _file: file,
            directory,
        })
    }
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    #[allow(clippy::unwrap_used)]
    fn darwin_flock_duplicate_lifetime_diagnostic() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let lock = DataDirLock::acquire(dir.path()).unwrap();
        // dup and fork share the same flock reference on Darwin. Keep that
        // reference alive deterministically, without a scheduling-dependent fork.
        let inherited = lock._file.try_clone().unwrap();
        assert!(matches!(
            DataDirLock::acquire(dir.path()),
            Err(StoreError::Busy)
        ));
        drop(lock);
        assert!(matches!(
            DataDirLock::acquire(dir.path()),
            Err(StoreError::Busy)
        ));
        println!("Darwin flock: guard dropped, duplicate alive, separate open => Busy");
        inherited.unlock().unwrap();
        let reopened = DataDirLock::acquire(dir.path()).unwrap();
        println!("Darwin flock: explicit unlock of duplicate => separate open succeeds");
        assert!(matches!(
            DataDirLock::acquire(dir.path()),
            Err(StoreError::Busy)
        ));
        drop(inherited);
        assert!(matches!(
            DataDirLock::acquire(dir.path()),
            Err(StoreError::Busy)
        ));
        drop(reopened);
        assert!(DataDirLock::acquire(dir.path()).is_ok());
    }

    #[test]
    fn owner_policy_uses_the_actual_effective_uid_and_rejects_other_owners() {
        let current = current_uid();
        assert_eq!(current, rustix::process::geteuid().as_raw());
        let other = if current == 12345 { 12346 } else { 12345 };
        assert!(!trusted_owner_mode(other, 0o700, current));
        assert!(!trusted_owner_mode(other, 0o1777, current));
        assert!(!trusted_owner_mode(current, 0o777, current));
        assert!(trusted_owner_mode(current, 0o700, current));
        assert!(trusted_owner_mode(0, 0o1777, current));
        assert!(!trusted_owner_mode(0, 0o777, current));
    }
}

#[cfg(all(test, unix))]
mod canonical_path_tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    #[allow(clippy::unwrap_used)]
    fn alias_substitution_between_lock_and_sqlite_open_cannot_redirect_database() {
        let root = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let real = root.path().join("real");
        let attacker = root.path().join("attacker");
        for base in [&real, &attacker] {
            fs::create_dir(base).unwrap();
            fs::create_dir(base.join("private")).unwrap();
            fs::set_permissions(base.join("private"), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let alias = root.path().join("alias");
        symlink(&real, &alias).unwrap();
        let canonical = private_directory(&alias.join("private")).unwrap();
        fs::remove_file(&alias).unwrap();
        symlink(&attacker, &alias).unwrap();
        let lock = DataDirLock::acquire_in(canonical).unwrap();
        let db =
            crate::database::Database::open(&lock.directory().join("meshyard.sqlite")).unwrap();
        assert!(matches!(
            DataDirLock::acquire(&real.join("private")),
            Err(StoreError::Busy)
        ));
        assert_eq!(fs::read_dir(attacker.join("private")).unwrap().count(), 0);
        assert!(real.join("private/meshyard.sqlite").is_file());
        drop(db);
        drop(lock);
    }
}
