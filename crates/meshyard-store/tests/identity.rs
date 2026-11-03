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

#[cfg(unix)]
#[test]
fn writable_ancestor_rejects_identity_lock_and_database_before_writing() {
    let root = support::private_tempdir();
    let ancestor = root.path().join("shared");
    std::fs::create_dir(&ancestor).unwrap();
    std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o777)).unwrap();
    let private = ancestor.join("private");
    std::fs::create_dir(&private).unwrap();
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
    let store = FileKeyStore::new(private.join("identity"));
    assert!(matches!(store.load(), Err(IdentityError::Permissions)));
    assert_eq!(
        store.create(&SecretKey::from_bytes(&[9; 32])),
        Err(IdentityError::Permissions)
    );
    assert!(matches!(
        DataDirLock::acquire(&private),
        Err(meshyard_store::StoreError::Permissions)
    ));
    assert!(matches!(
        meshyard_store::Store::open(&private),
        Err(meshyard_store::StoreError::Permissions)
    ));
    assert_eq!(std::fs::read_dir(&private).unwrap().count(), 0);
    let missing = ancestor.join("missing");
    assert!(meshyard_store::Store::open(&missing).is_err());
    assert!(!missing.exists());
    // A symlink at an actor-writable ancestor cannot hide the unsafe chain.
    let alias = root.path().join("alias");
    symlink(&ancestor, &alias).unwrap();
    assert!(
        FileKeyStore::new(alias.join("private/identity"))
            .create(&SecretKey::from_bytes(&[9; 32]))
            .is_err()
    );
    assert!(meshyard_store::Store::open(alias.join("private")).is_err());
    assert_eq!(std::fs::read_dir(&private).unwrap().count(), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn canonical_tmp_aliases_work_under_root_owned_sticky_parent() {
    let dir = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in("/tmp")
        .unwrap();
    let key = load_or_create(&FileKeyStore::new(dir.path().join("identity"))).unwrap();
    assert_eq!(
        key.device_id(),
        FileKeyStore::new(dir.path().join("identity"))
            .load()
            .unwrap()
            .device_id()
    );
    meshyard_store::Store::open(dir.path())
        .unwrap()
        .close()
        .await
        .unwrap();
    assert!(dir.path().join("meshyard.sqlite").is_file());
}

#[cfg(unix)]
#[test]
fn sqlite_leaf_and_sidecar_aliases_never_modify_unrelated_files() {
    for name in [
        "meshyard.sqlite",
        "meshyard.sqlite-wal",
        "meshyard.sqlite-shm",
        "meshyard.sqlite-journal",
    ] {
        for hardlink in [false, true] {
            let dir = support::private_tempdir();
            let outside = support::private_tempdir();
            let target = outside.path().join("unrelated");
            std::fs::write(&target, b"do not touch").unwrap();
            let link = dir.path().join(name);
            if hardlink {
                std::fs::hard_link(&target, &link).unwrap();
            } else {
                symlink(&target, &link).unwrap();
            }
            assert!(matches!(
                meshyard_store::Store::open(dir.path()),
                Err(meshyard_store::StoreError::Permissions)
            ));
            assert_eq!(std::fs::read(&target).unwrap(), b"do not touch");
            assert_eq!(
                std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                0o644
            );
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn macos_extended_and_inherited_acl_grants_are_rejected_even_with_private_mode() {
    use exacl::{AclEntry, Flag, Perm};
    for flags in [Flag::empty(), Flag::FILE_INHERIT | Flag::DIRECTORY_INHERIT] {
        let dir = support::private_tempdir();
        // A grant to our own numeric UID is sufficient to exercise the deliberately
        // conservative policy without creating users or needing administrator rights.
        let uid = rustix::process::geteuid().as_raw().to_string();
        let grant =
            AclEntry::allow_user(&uid, Perm::READ | Perm::WRITE | Perm::DELETE_CHILD, flags);
        exacl::setfacl(&[dir.path()], &[grant], None).unwrap();
        assert_eq!(
            std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let child = dir.path().join("private");
        std::fs::create_dir(&child).unwrap();
        std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o700)).unwrap();
        if !flags.is_empty() {
            assert!(
                exacl::getfacl(&child, None)
                    .unwrap()
                    .iter()
                    .any(|e| e.flags.contains(Flag::INHERITED))
            );
        }
        assert_eq!(
            FileKeyStore::new(child.join("identity")).create(&SecretKey::from_bytes(&[9; 32])),
            Err(IdentityError::Permissions)
        );
        assert!(meshyard_store::Store::open(&child).is_err());
        assert_eq!(std::fs::read_dir(&child).unwrap().count(), 0);
        exacl::setfacl(&[dir.path(), child.as_path()], &[], None).unwrap();
    }
}

#[cfg(target_os = "macos")]
#[test]
fn acl_dependency_cannot_emit_debug_paths_even_if_runtime_logging_is_enabled() {
    assert!(log::STATIC_MAX_LEVEL < log::LevelFilter::Debug);
}
