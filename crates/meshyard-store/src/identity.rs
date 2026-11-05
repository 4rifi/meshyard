//! Secret bytes are never bound into SQL. Errors deliberately omit OS text and paths.
use crate::process_lock::{nofollow, private_directory, private_file};
use meshyard_core::SecretKey;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

pub const KEYCHAIN_SERVICE: &str = "computer.meshyard.device";
pub const KEYCHAIN_ACCOUNT: &str = "device";
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    #[error("device identity not found")]
    NotFound,
    #[error("device identity store unavailable")]
    Unavailable,
    #[error("device identity store locked or access denied")]
    Locked,
    #[error("device identity already exists; refusing replacement")]
    AlreadyExists,
    #[error("unsafe identity permissions or symlink")]
    Permissions,
    #[error("invalid device identity format")]
    Invalid,
    #[error("device identity I/O failed")]
    Io,
    #[error("secure random source unavailable")]
    Random,
}
pub trait DeviceKeyStore {
    fn load(&self) -> Result<SecretKey, IdentityError>;
    fn create(&self, key: &SecretKey) -> Result<(), IdentityError>;
}
/// Only true NotFound permits generating a key. No backend fallback is performed.
pub fn load_or_create(store: &dyn DeviceKeyStore) -> Result<SecretKey, IdentityError> {
    match store.load() {
        Ok(key) => Ok(key),
        Err(IdentityError::NotFound) => {
            let mut bytes = Zeroizing::new([0; 32]);
            getrandom::fill(bytes.as_mut()).map_err(|_| IdentityError::Random)?;
            let key = SecretKey::from_bytes(&bytes);
            store.create(&key)?;
            Ok(key)
        }
        Err(error) => Err(error),
    }
}
/// Constructing this type is the explicit file-backend selection. No default/fallback path.
pub struct FileKeyStore {
    path: PathBuf,
}
impl FileKeyStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
        }
    }
    fn parent(&self) -> Result<&Path, IdentityError> {
        self.path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or(IdentityError::Permissions)
    }
}
impl DeviceKeyStore for FileKeyStore {
    fn load(&self) -> Result<SecretKey, IdentityError> {
        let parent = private_directory(self.parent()?).map_err(|_| IdentityError::Permissions)?;
        let path = parent.join(self.path.file_name().ok_or(IdentityError::Permissions)?);
        // Check leaf absence only after validating the canonical destination.
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(IdentityError::NotFound);
            }
            Err(_) => return Err(IdentityError::Io),
            Ok(meta) if meta.file_type().is_symlink() => return Err(IdentityError::Permissions),
            _ => {}
        }
        let mut options = OpenOptions::new();
        options.read(true);
        nofollow(&mut options);
        let file = options
            .open(&path)
            .map_err(|_| IdentityError::Permissions)?;
        private_file(&file, &path).map_err(|_| IdentityError::Permissions)?;
        let mut bytes = Zeroizing::new([0; 33]);
        let mut reader = file.take(33);
        let mut len = 0;
        while len < bytes.len() {
            let count = reader
                .read(&mut bytes[len..])
                .map_err(|_| IdentityError::Io)?;
            if count == 0 {
                break;
            }
            len += count;
        }
        if len != 32 {
            return Err(IdentityError::Invalid);
        }
        let key_bytes: &[u8; 32] = bytes[..32].try_into().map_err(|_| IdentityError::Invalid)?;
        Ok(SecretKey::from_bytes(key_bytes))
    }
    fn create(&self, key: &SecretKey) -> Result<(), IdentityError> {
        let parent = private_directory(self.parent()?).map_err(|_| IdentityError::Permissions)?;
        let path = parent.join(self.path.file_name().ok_or(IdentityError::Permissions)?);
        self.create_in(key, &parent, &path)
    }
}
impl FileKeyStore {
    fn create_in(&self, key: &SecretKey, parent: &Path, path: &Path) -> Result<(), IdentityError> {
        // A complete, fsynced temporary inode is linked into place atomically with no overwrite.
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(|_| IdentityError::Random)?;
        let suffix: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
        let temp = parent.join(format!(".identity-{suffix}.tmp"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        nofollow(&mut options);
        let mut file = options.open(&temp).map_err(|_| IdentityError::Io)?;
        let result = (|| {
            private_file(&file, &temp).map_err(|_| IdentityError::Permissions)?;
            key.with_secret_bytes(|bytes| file.write_all(bytes))
                .map_err(|_| IdentityError::Io)?;
            file.sync_all().map_err(|_| IdentityError::Io)?;
            fs::hard_link(&temp, path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    IdentityError::AlreadyExists
                } else {
                    IdentityError::Io
                }
            })?;
            Ok(())
        })();
        // Only our create_new staging inode is ever removed.
        let cleanup = fs::remove_file(&temp).map_err(|_| IdentityError::Io);
        result?;
        cleanup?;
        fs::File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| IdentityError::Io)?;
        Ok(())
    }
}
#[cfg(target_os = "macos")]
pub struct KeychainKeyStore {
    service: String,
    account: String,
}
#[cfg(target_os = "macos")]
impl Default for KeychainKeyStore {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(target_os = "macos")]
impl KeychainKeyStore {
    pub fn new() -> Self {
        Self {
            service: KEYCHAIN_SERVICE.into(),
            account: KEYCHAIN_ACCOUNT.into(),
        }
    }
    fn entry(&self) -> Result<keyring::Entry, IdentityError> {
        keyring::Entry::new(&self.service, &self.account).map_err(map_keychain)
    }
}
#[cfg(target_os = "macos")]
fn map_keychain(error: keyring::Error) -> IdentityError {
    match error {
        keyring::Error::NoEntry => IdentityError::NotFound,
        keyring::Error::PlatformFailure(ref inner) | keyring::Error::NoStorageAccess(ref inner) => {
            inner
                .downcast_ref::<security_framework::base::Error>()
                .map(|e| map_native_code(e.code()))
                .unwrap_or(IdentityError::Unavailable)
        }
        _ => IdentityError::Unavailable,
    }
}
#[cfg(target_os = "macos")]
fn map_native_code(code: i32) -> IdentityError {
    match code {
        -25300 => IdentityError::NotFound,
        -25299 => IdentityError::AlreadyExists,
        -25293 | -25308 => IdentityError::Locked,
        _ => IdentityError::Unavailable,
    }
}
#[cfg(target_os = "macos")]
impl DeviceKeyStore for KeychainKeyStore {
    fn load(&self) -> Result<SecretKey, IdentityError> {
        let secret = Zeroizing::new(self.entry()?.get_secret().map_err(map_keychain)?);
        let bytes: &[u8; 32] = secret
            .as_slice()
            .try_into()
            .map_err(|_| IdentityError::Invalid)?;
        Ok(SecretKey::from_bytes(bytes))
    }
    fn create(&self, key: &SecretKey) -> Result<(), IdentityError> {
        match self.load() {
            Err(IdentityError::NotFound) => {}
            Ok(_) => return Err(IdentityError::AlreadyExists),
            Err(error) => return Err(error),
        }
        // keyring set_secret uses an upsert (including on some read errors). Use the
        // same backend's safe create-only API so races cannot overwrite an unknown key.
        use security_framework::os::macos::keychain::{SecKeychain, SecPreferencesDomain};
        let chain = SecKeychain::default_for_domain(SecPreferencesDomain::User)
            .map_err(|e| map_native_code(e.code()))?;
        key.with_secret_bytes(|bytes| {
            chain
                .add_generic_password(&self.service, &self.account, bytes)
                .map_err(|e| map_native_code(e.code()))
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod keychain_probe {
    use super::*;
    use meshyard_core::Signer;
    #[test]
    #[ignore = "manual isolated macOS Keychain probe; may require OS approval"]
    fn isolated_keychain_roundtrip() -> Result<(), IdentityError> {
        let _no_prompts =
            security_framework::os::macos::keychain::SecKeychain::disable_user_interaction()
                .map_err(|e| map_native_code(e.code()))?;
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(|_| IdentityError::Random)?;
        let suffix: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
        let store = KeychainKeyStore {
            service: format!("computer.meshyard.test-probe.{suffix}"),
            account: "isolated-t002-probe".into(),
        };
        // No production lookup. If unavailable/denied, stop without fallback or deletion.
        assert_eq!(store.load().err(), Some(IdentityError::NotFound));
        let key = load_or_create(&store)?;
        let result = store
            .load()
            .map(|loaded| assert_eq!(key.device_id(), loaded.device_id()));
        // We created this unique record successfully, so only it may be deleted.
        let cleanup = store.entry()?.delete_credential().map_err(map_keychain);
        result?;
        cleanup?;
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod canonical_path_tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    #[allow(clippy::unwrap_used)]
    fn alias_substitution_after_validation_does_not_redirect_secret_publication_or_cleanup() {
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
        let store = FileKeyStore::new(alias.join("private/identity"));
        let parent = private_directory(store.parent().unwrap()).unwrap();
        // Deterministic substitution exactly between validation and publication.
        // The test process has the same UID; this tests canonical-path pinning,
        // not an actual second-user exploit or 0600 file-read bypass.
        fs::remove_file(&alias).unwrap();
        symlink(&attacker, &alias).unwrap();
        store
            .create_in(
                &SecretKey::from_bytes(&[31; 32]),
                &parent,
                &parent.join("identity"),
            )
            .unwrap();
        assert_eq!(fs::read_dir(attacker.join("private")).unwrap().count(), 0);
        assert_eq!(fs::read_dir(real.join("private")).unwrap().count(), 1);
        assert_eq!(
            fs::read(real.join("private/identity")).unwrap(),
            vec![31; 32]
        );
    }
}
