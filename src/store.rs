//! Encrypted, app-private credential storage.
//!
//! The encryption key is derived from the Secret portal's per-app master
//! secret. If the portal is unavailable, or the stored payload cannot be
//! authenticated, opening the store fails instead of falling back to plaintext
//! or starting with an empty account list.

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

pub struct SecretStore {
    path: PathBuf,
    cipher: XChaCha20Poly1305,
}

impl SecretStore {
    /// Opens the store, retrieving the app secret from the XDG Secret portal.
    pub async fn open(config: &Config) -> Result<Self> {
        let master = ashpd::desktop::secret::retrieve()
            .await
            .map_err(|error| Error::StorageUnavailable(error.to_string()))?;
        Self::open_with_key(config, &master)
    }

    /// Opens the store with an explicitly supplied master secret.
    pub fn open_with_key(config: &Config, master: &[u8]) -> Result<Self> {
        if master.is_empty() {
            return Err(Error::StorageUnavailable(
                "the Secret portal returned an empty secret".into(),
            ));
        }

        let hkdf = Hkdf::<Sha256>::new(Some(config.app_id.as_bytes()), master);
        let mut key = Zeroizing::new([0u8; 32]);
        hkdf.expand(b"cosmic-utils/accounts/store/v1", &mut *key)
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
        let plaintext = self
            .cipher
            .decrypt(nonce.into(), ciphertext)
            .map_err(|_| {
                Error::StorageCorrupt(
                    "the credential store could not be decrypted; the Secret portal key may have changed"
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
