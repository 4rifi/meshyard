#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;
use meshyard_core::{SecretKey, Signer};
use meshyard_store::{identity::*, process_lock::DataDirLock};
#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt, symlink};
#[test]
fn file_key_roundtrip_private_atomic_and_debug_redacted() {
    let dir = support::private_tempdir();
    let path = dir.path().join("identity");
    let store = FileKeyStore::new(&path);
    assert!(matches!(store.load(), Err(IdentityError::NotFound)));
    let key = load_or_create(&store).unwrap();
    assert_eq!(key.device_id(), load_or_create(&store).unwrap().device_id());
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(format!("{key:?}"), "SecretKey([REDACTED])");
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        store.create(&SecretKey::from_bytes(&[44; 32])),
        Err(IdentityError::AlreadyExists)
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn unknown_file_is_never_overwritten_even_if_invalid() {
    let dir = support::private_tempdir();
    let path = dir.path().join("identity");
    std::fs::write(&path, b"unknown file").unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let store = FileKeyStore::new(&path);
    assert_eq!(
        store.create(&SecretKey::from_bytes(&[1; 32])),
        Err(IdentityError::AlreadyExists)
    );
    assert!(matches!(
        load_or_create(&store),
        Err(IdentityError::Invalid)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"unknown file");
}
#[cfg(unix)]
#[test]
fn dangerous_permissions_symlinks_and_hardlinks_are_rejected() {
    let dir = support::private_tempdir();
    let path = dir.path().join("identity");
    let store = FileKeyStore::new(&path);
    load_or_create(&store).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(store.load(), Err(IdentityError::Permissions)));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let link = dir.path().join("link");
    symlink(&path, &link).unwrap();
    let linked = FileKeyStore::new(&link);
    assert!(matches!(linked.load(), Err(IdentityError::Permissions)));
    assert_eq!(
        linked.create(&SecretKey::from_bytes(&[2; 32])),
        Err(IdentityError::AlreadyExists)
    );
    std::fs::hard_link(&path, dir.path().join("hardlink")).unwrap();
    assert!(matches!(store.load(), Err(IdentityError::Permissions)));
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        DataDirLock::acquire(dir.path()),
        Err(meshyard_store::StoreError::Permissions)
    ));
    assert!(matches!(store.load(), Err(IdentityError::Permissions)));
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
}
#[test]
fn unavailable_locked_invalid_and_denied_never_generate_replacements() {
    struct Unavailable(IdentityError);
    impl DeviceKeyStore for Unavailable {
        fn load(&self) -> Result<SecretKey, IdentityError> {
            Err(self.0)
        }
        fn create(&self, _: &SecretKey) -> Result<(), IdentityError> {
            panic!("must never replace identity")
        }
    }
    for error in [
        IdentityError::Unavailable,
        IdentityError::Locked,
        IdentityError::Invalid,
        IdentityError::Permissions,
    ] {
        assert!(matches!(load_or_create(&Unavailable(error)),Err(e) if e==error));
    }
}
#[test]
fn concurrent_file_creation_has_one_winner_without_overwrite() {
    let dir = support::private_tempdir();
    let path = dir.path().join("identity");
    let mut jobs = vec![];
    for byte in 1..=8 {
        let path = path.clone();
        jobs.push(std::thread::spawn(move || {
            FileKeyStore::new(path).create(&SecretKey::from_bytes(&[byte; 32]))
        }));
    }
    let results: Vec<_> = jobs.into_iter().map(|job| job.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(FileKeyStore::new(path).load().is_ok());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[cfg(unix)]
#[test]
fn symlink_data_dir_and_lock_are_rejected() {
    let dir = support::private_tempdir();
    let link = dir.path().join("link");
    symlink(dir.path(), &link).unwrap();
    assert!(DataDirLock::acquire(&link).is_err());
    let target = dir.path().join("unrelated");
    std::fs::write(&target, b"do not touch").unwrap();
    symlink(&target, dir.path().join("process.lock")).unwrap();
    assert!(DataDirLock::acquire(dir.path()).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"do not touch");
}
