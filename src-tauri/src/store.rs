use crate::model::{now_millis, TunnelConfig};
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

const FILE_NAME: &str = "tunnels.json";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("无法读写隧道配置: {0}")]
    Io(#[from] std::io::Error),
    #[error("无法序列化隧道配置: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Single source of truth for tunnel definitions. Small JSON document with
/// atomic replace; the in-memory snapshot is guarded by an async RwLock so
/// reads never touch the disk.
pub struct Store {
    path: PathBuf,
    tunnels: RwLock<Vec<TunnelConfig>>,
}

impl Store {
    pub async fn load(dir: &Path) -> Result<Self, StoreError> {
        tokio::fs::create_dir_all(dir).await?;
        let path = dir.join(FILE_NAME);
        let mut tunnels = match tokio::fs::read(&path).await {
            Ok(raw) => match serde_json::from_slice::<Vec<TunnelConfig>>(&raw) {
                Ok(mut list) => {
                    list.sort_by_key(|t| t.created_at);
                    list
                }
                Err(error) => {
                    tracing::error!(%error, "隧道配置 JSON 损坏，暂以空配置启动");
                    Vec::new()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(StoreError::Io(error)),
        };
        tunnels.sort_by_key(|t| t.created_at);
        Ok(Self {
            path,
            tunnels: RwLock::new(tunnels),
        })
    }

    pub async fn snapshot(&self) -> Vec<TunnelConfig> {
        self.tunnels.read().await.clone()
    }

    pub async fn get(&self, id: &str) -> Option<TunnelConfig> {
        self.tunnels
            .read()
            .await
            .iter()
            .find(|t| t.id == id)
            .cloned()
    }

    pub async fn upsert(&self, mut config: TunnelConfig) -> Result<TunnelConfig, StoreError> {
        config.updated_at = now_millis();
        let mut guard = self.tunnels.write().await;
        match guard.iter().position(|t| t.id == config.id) {
            Some(index) => guard[index] = config.clone(),
            None => guard.push(config.clone()),
        }
        self.persist(&guard).await?;
        Ok(config)
    }

    pub async fn delete(&self, id: &str) -> Result<(), StoreError> {
        let mut guard = self.tunnels.write().await;
        guard.retain(|t| t.id != id);
        self.persist(&guard).await
    }

    pub async fn set_desired(&self, id: &str, on: bool) -> Result<(), StoreError> {
        let mut guard = self.tunnels.write().await;
        if let Some(tunnel) = guard.iter_mut().find(|t| t.id == id) {
            tunnel.desired_on = on;
            tunnel.updated_at = now_millis();
        }
        self.persist(&guard).await
    }

    pub async fn set_all_desired(&self, on: bool) -> Result<(), StoreError> {
        let mut guard = self.tunnels.write().await;
        for tunnel in guard.iter_mut() {
            tunnel.desired_on = on;
            tunnel.updated_at = now_millis();
        }
        self.persist(&guard).await
    }

    pub async fn set_desired_for(&self, ids: &[String], on: bool) -> Result<(), StoreError> {
        let mut guard = self.tunnels.write().await;
        for tunnel in guard.iter_mut() {
            if ids.iter().any(|id| id == &tunnel.id) {
                tunnel.desired_on = on;
                tunnel.updated_at = now_millis();
            }
        }
        self.persist(&guard).await
    }

    pub async fn any_auto_start(&self) -> bool {
        self.tunnels
            .read()
            .await
            .iter()
            .any(|t| t.auto_start_on_boot)
    }

    async fn persist(&self, tunnels: &[TunnelConfig]) -> Result<(), StoreError> {
        let data = serde_json::to_vec_pretty(tunnels)?;
        let temp = self.path.with_extension("json.tmp");
        tokio::fs::write(&temp, &data).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = tokio::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600)).await;
        }
        if let Err(error) = tokio::fs::rename(&temp, &self.path).await {
            // Windows cannot replace an existing file through rename.
            let _ = tokio::fs::remove_file(&self.path).await;
            tokio::fs::rename(&temp, &self.path)
                .await
                .map_err(|_| StoreError::Io(error))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn persists_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = Store::load(dir.path()).await.unwrap();
            let mut config = TunnelConfig {
                name: "NAS".into(),
                ssh_host: "example.com".into(),
                ssh_user: "ops".into(),
                remote_host: "127.0.0.1".into(),
                ..TunnelConfig::default()
            };
            config.name = "NAS".into();
            store.upsert(config.clone()).await.unwrap();
            assert_eq!(store.snapshot().await.len(), 1);
            store.set_desired(&config.id, true).await.unwrap();
        }
        let store = Store::load(dir.path()).await.unwrap();
        let loaded = store.snapshot().await;
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "NAS");
        assert!(loaded[0].desired_on);
    }

    #[tokio::test]
    async fn delete_removes_entry() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::load(dir.path()).await.unwrap();
        let config = store.upsert(TunnelConfig::default()).await.unwrap();
        store.delete(&config.id).await.unwrap();
        assert!(store.snapshot().await.is_empty());
    }
}
