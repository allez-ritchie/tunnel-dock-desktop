use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rand::rngs::OsRng;
use rand::RngCore;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const KEY_FILE: &str = "master.key";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const VERSION: &str = "v1";

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("无法读写主密钥: {0}")]
    Io(#[from] std::io::Error),
    #[error("主密钥文件格式不正确")]
    InvalidKey,
    #[error("无法加密 SSH 凭据")]
    Encrypt,
    #[error("无法解密 SSH 凭据，请重新输入")]
    Decrypt,
    #[error("未知的密文格式")]
    Format,
}

/// AES-256-GCM vault backed by a 32-byte random master key stored with mode
/// 0600 in the app data directory. Ciphertext layout: `v1.iv.data` (Base64).
pub struct Secrets {
    key: [u8; KEY_LEN],
}

impl Secrets {
    pub fn load(dir: &Path) -> Result<Arc<Self>, SecretError> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(KEY_FILE);
        let key = if path.is_file() {
            let raw = std::fs::read(&path)?;
            if raw.len() != KEY_LEN {
                return Err(SecretError::InvalidKey);
            }
            let mut key = [0u8; KEY_LEN];
            key.copy_from_slice(&raw);
            key
        } else {
            let mut key = [0u8; KEY_LEN];
            OsRng.fill_bytes(&mut key);
            write_key_file(&path, &key)?;
            key
        };
        Ok(Arc::new(Self { key }))
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String, SecretError> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.key));
        let mut iv = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut iv);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&iv), plaintext.as_bytes())
            .map_err(|_| SecretError::Encrypt)?;
        Ok(format!(
            "{VERSION}.{}.{}",
            BASE64.encode(iv),
            BASE64.encode(ciphertext)
        ))
    }

    pub fn decrypt(&self, payload: &str) -> Result<String, SecretError> {
        let mut parts = payload.split('.');
        let version = parts.next().ok_or(SecretError::Format)?;
        if version != VERSION {
            return Err(SecretError::Format);
        }
        let iv = BASE64
            .decode(parts.next().ok_or(SecretError::Format)?)
            .map_err(|_| SecretError::Format)?;
        let data = BASE64
            .decode(parts.next().ok_or(SecretError::Format)?)
            .map_err(|_| SecretError::Format)?;
        if parts.next().is_some() || iv.len() != NONCE_LEN {
            return Err(SecretError::Format);
        }
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.key));
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&iv), data.as_ref())
            .map_err(|_| SecretError::Decrypt)?;
        String::from_utf8(plaintext).map_err(|_| SecretError::Decrypt)
    }

    pub fn decrypt_opt(&self, payload: Option<&String>) -> Result<Option<String>, SecretError> {
        match payload {
            Some(value) if !value.is_empty() => self.decrypt(value).map(Some),
            _ => Ok(None),
        }
    }
}

fn write_key_file(path: &PathBuf, key: &[u8]) -> Result<(), SecretError> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(key)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_format() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let encrypted = vault.encrypt("s3cret-密码").unwrap();
        assert!(encrypted.starts_with("v1."));
        assert_eq!(encrypted.split('.').count(), 3);
        assert_eq!(vault.decrypt(&encrypted).unwrap(), "s3cret-密码");
    }

    #[test]
    fn ciphertext_is_randomized() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        assert_ne!(
            vault.encrypt("same").unwrap(),
            vault.encrypt("same").unwrap()
        );
    }

    #[test]
    fn rejects_tampered_payload() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let encrypted = vault.encrypt("value").unwrap();
        let tampered = format!("{encrypted}AA");
        assert!(vault.decrypt(&tampered).is_err());
        assert!(vault.decrypt("v2.a.b").is_err());
    }

    #[test]
    fn another_vault_cannot_decrypt() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let a = Secrets::load(dir_a.path()).unwrap();
        let b = Secrets::load(dir_b.path()).unwrap();
        let encrypted = a.encrypt("value").unwrap();
        assert!(b.decrypt(&encrypted).is_err());
    }
}
