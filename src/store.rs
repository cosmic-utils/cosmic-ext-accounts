//! Encrypted, app-private credential storage.
//!
//! The encryption key is derived from a master secret held by the operating
//! system. Opening the store requires secure storage and an authentic payload.

use std::{fs, io::Write, path::PathBuf};

use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::{
    config::Config,
    error::{Error, Result},
    model::{STORE_VERSION, StoredData},
};

const NONCE_LEN: usize = 24;
const STORE_FILE: &str = "accounts.enc";
// The v1 file format uses this HKDF context. Changing it would invalidate
// previously encrypted stores.
const V1_KEY_CONTEXT: &str = "cosmic-utils/accounts/store/v1";

pub struct SecretStore {
    path: PathBuf,
    cipher: XChaCha20Poly1305,
}

impl SecretStore {
    /// Opens the store using the platform's secure storage.
    #[cfg(any(unix, windows))]
    pub async fn open(config: &Config) -> Result<Self> {
        #[cfg(all(unix, not(target_os = "macos")))]
        let master = Zeroizing::new(
            ashpd::desktop::secret::retrieve()
                .await
                .map_err(|error| Error::StorageUnavailable(error.to_string()))?,
        );

        #[cfg(any(target_os = "macos", windows))]
        let master = {
            let config = config.clone();
            tokio::task::spawn_blocking(move || native_master(&config))
                .await
                .map_err(|error| Error::StorageUnavailable(error.to_string()))??
        };

        Self::open_with_key(config, &master)
    }

    #[cfg(not(any(unix, windows)))]
    pub async fn open(_config: &Config) -> Result<Self> {
        Err(Error::StorageUnavailable(
            "secure storage is unsupported on this platform".into(),
        ))
    }

    /// Opens the store with an explicitly supplied master secret.
    pub fn open_with_key(config: &Config, master: &[u8]) -> Result<Self> {
        if master.is_empty() {
            return Err(Error::StorageUnavailable(
                "the master secret is empty".into(),
            ));
        }

        let hkdf = Hkdf::<Sha256>::new(Some(config.app_id.as_bytes()), master);
        let mut key = Zeroizing::new([0u8; 32]);
        hkdf.expand(V1_KEY_CONTEXT.as_bytes(), &mut *key)
            .map_err(|error| Error::StorageUnavailable(error.to_string()))?;

        let dir = config.data_dir()?;
        fs::create_dir_all(&dir).map_err(|error| Error::StorageUnavailable(error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        }

        Ok(Self {
            path: dir.join(STORE_FILE),
            cipher: XChaCha20Poly1305::new_from_slice(&key[..])
                .map_err(|error| Error::StorageUnavailable(error.to_string()))?,
        })
    }

    pub fn load(&self) -> Result<StoredData> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(StoredData {
                    version: STORE_VERSION,
                    ..StoredData::default()
                });
            }
            Err(error) => {
                return Err(Error::StorageUnavailable(error.to_string()));
            }
        };
        if bytes.len() < NONCE_LEN {
            return Err(Error::StorageCorrupt("stored payload is truncated".into()));
        }
        let (nonce, ciphertext) = bytes.split_at(NONCE_LEN);
        let plaintext = self.cipher.decrypt(nonce.into(), ciphertext).map_err(|_| {
            Error::StorageCorrupt(
                "the credential store could not be decrypted; the master secret may have changed"
                    .into(),
            )
        })?;
        let data: StoredData = serde_json::from_slice(&plaintext)
            .map_err(|error| Error::StorageCorrupt(error.to_string()))?;
        if data.version != STORE_VERSION {
            return Err(Error::StorageCorrupt(format!(
                "unsupported store version {}",
                data.version
            )));
        }
        Ok(data)
    }

    pub fn save(&self, data: &StoredData) -> Result<()> {
        let plaintext = serde_json::to_vec(data)
            .map_err(|error| Error::StorageUnavailable(error.to_string()))?;
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_slice())
            .map_err(|error| Error::StorageUnavailable(error.to_string()))?;

        let mut payload = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&ciphertext);

        let dir = self
            .path
            .parent()
            .ok_or_else(|| Error::StorageUnavailable("store path has no parent".into()))?;
        let mut temporary = tempfile::NamedTempFile::new_in(dir)
            .map_err(|error| Error::StorageUnavailable(error.to_string()))?;
        temporary
            .write_all(&payload)
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|error| Error::StorageUnavailable(error.to_string()))?;
        temporary
            .persist(&self.path)
            .map_err(|error| Error::StorageUnavailable(error.to_string()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

#[cfg(any(target_os = "macos", windows))]
fn native_master(config: &Config) -> Result<Zeroizing<Vec<u8>>> {
    use chacha20poly1305::aead::rand_core::RngCore;

    // Serialize key creation for the app, including callers that override the
    // encrypted store's location. Both native backends can overwrite an entry.
    let lock_dir = config.default_data_dir()?;
    fs::create_dir_all(&lock_dir).map_err(|error| Error::StorageUnavailable(error.to_string()))?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_dir.join("accounts-key.lock"))
        .map_err(|error| Error::StorageUnavailable(error.to_string()))?;
    lock.lock()
        .map_err(|error| Error::StorageUnavailable(error.to_string()))?;

    let entry = keyring::Entry::new(&config.app_id, "cosmic-ext-orbit/accounts/store/v1")
        .map_err(|error| Error::StorageUnavailable(error.to_string()))?;
    match entry.get_secret() {
        Ok(master) => Ok(Zeroizing::new(master)),
        Err(keyring::Error::NoEntry) => {
            let legacy_entry = keyring::Entry::new(&config.app_id, V1_KEY_CONTEXT)
                .map_err(|error| Error::StorageUnavailable(error.to_string()))?;
            match legacy_entry.get_secret() {
                Ok(master) => {
                    let master = Zeroizing::new(master);
                    entry
                        .set_secret(&master)
                        .map_err(|error| Error::StorageUnavailable(error.to_string()))?;
                    return Ok(master);
                }
                Err(keyring::Error::NoEntry) => {}
                Err(error) => return Err(Error::StorageUnavailable(error.to_string())),
            }

            if config
                .data_dir()?
                .join(STORE_FILE)
                .try_exists()
                .map_err(|error| Error::StorageUnavailable(error.to_string()))?
            {
                return Err(Error::StorageUnavailable(
                    "the master secret for the existing credential store is missing".into(),
                ));
            }

            let mut master = Zeroizing::new(vec![0; 32]);
            OsRng
                .try_fill_bytes(&mut master)
                .map_err(|error| Error::StorageUnavailable(error.to_string()))?;
            entry
                .set_secret(&master)
                .map_err(|error| Error::StorageUnavailable(error.to_string()))?;
            Ok(master)
        }
        Err(error) => Err(Error::StorageUnavailable(error.to_string())),
    }
}
