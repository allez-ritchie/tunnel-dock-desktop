use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

const FILE_NAME: &str = "known_hosts.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyEntry {
    pub fingerprint: String,
    pub key_type: String,
    pub added_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct KnownHostsDocument {
    version: u32,
    hosts: HashMap<String, HostKeyEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyDecision {
    /// Fingerprint already pinned and identical.
    Match,
    /// First contact, fingerprint was pinned now.
    Pinned,
    /// Pinned fingerprint does not match: refuse the connection.
    Changed,
}

/// Trust-on-first-use host key store. A mismatching pinned key is never
/// replaced automatically.
pub struct KnownHosts {
    path: PathBuf,
    entries: RwLock<HashMap<String, HostKeyEntry>>,
}

impl KnownHosts {
    pub async fn load(dir: &Path) -> std::io::Result<Self> {
        tokio::fs::create_dir_all(dir).await?;
        let path = dir.join(FILE_NAME);
        let entries = match tokio::fs::read(&path).await {
            Ok(raw) => match serde_json::from_slice::<KnownHostsDocument>(&raw) {
                Ok(doc) => doc.hosts,
                Err(error) => {
                    tracing::warn!(%error, "已知主机文件损坏，将重新建立信任");
                    HashMap::new()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => return Err(error),
        };
        Ok(Self {
            path,
            entries: RwLock::new(entries),
        })
    }

    pub async fn count(&self) -> usize {
        self.entries.read().await.len()
    }

    pub async fn clear(&self) {
        self.entries.write().await.clear();
        let _ = tokio::fs::remove_file(&self.path).await;
    }

    pub async fn check_and_pin(
        &self,
        host: &str,
        fingerprint: &str,
        key_type: &str,
    ) -> HostKeyDecision {
        if let Some(entry) = self.entries.read().await.get(host) {
            return if entry.fingerprint == fingerprint {
                HostKeyDecision::Match
            } else {
                HostKeyDecision::Changed
            };
        }

        let mut guard = self.entries.write().await;
        if let Some(entry) = guard.get(host) {
            return if entry.fingerprint == fingerprint {
                HostKeyDecision::Match
            } else {
                HostKeyDecision::Changed
            };
        }
        guard.insert(
            host.to_string(),
            HostKeyEntry {
                fingerprint: fingerprint.to_string(),
                key_type: key_type.to_string(),
                added_at: crate::model::now_millis(),
            },
        );
        if let Err(error) = self.persist(&guard).await {
            tracing::error!(%error, "无法保存主机密钥");
        }
        HostKeyDecision::Pinned
    }

    async fn persist(&self, entries: &HashMap<String, HostKeyEntry>) -> std::io::Result<()> {
        let doc = KnownHostsDocument {
            version: 1,
            hosts: entries.clone(),
        };
        let data = serde_json::to_vec_pretty(&doc)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        let temp = self.path.with_extension("json.tmp");
        tokio::fs::write(&temp, data).await?;
        if let Err(error) = tokio::fs::rename(&temp, &self.path).await {
            let _ = tokio::fs::remove_file(&self.path).await;
            tokio::fs::rename(&temp, &self.path)
                .await
                .map_err(|_| error)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pins_then_rejects_changed_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = KnownHosts::load(dir.path()).await.unwrap();
        assert_eq!(
            store
                .check_and_pin("host:22", "SHA256:aaa", "ssh-ed25519")
                .await,
            HostKeyDecision::Pinned
        );
        assert_eq!(
            store
                .check_and_pin("host:22", "SHA256:aaa", "ssh-ed25519")
                .await,
            HostKeyDecision::Match
        );
        assert_eq!(
            store
                .check_and_pin("host:22", "SHA256:bbb", "ssh-ed25519")
                .await,
            HostKeyDecision::Changed
        );
        assert_eq!(store.count().await, 1);
    }

    #[tokio::test]
    async fn survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = KnownHosts::load(dir.path()).await.unwrap();
            store
                .check_and_pin("host:22", "SHA256:aaa", "ssh-ed25519")
                .await;
        }
        let store = KnownHosts::load(dir.path()).await.unwrap();
        assert_eq!(store.count().await, 1);
        assert_eq!(
            store
                .check_and_pin("host:22", "SHA256:aaa", "ssh-ed25519")
                .await,
            HostKeyDecision::Match
        );
    }
}
