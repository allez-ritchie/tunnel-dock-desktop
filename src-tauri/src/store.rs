use crate::model::{
    new_id, now_millis, AppGroup, DockSnapshot, ForwardApp, JumpHost, SshCredentials, TunnelConfig,
    DEFAULT_GROUP_ID, SNAPSHOT_VERSION,
};
use std::collections::HashSet;
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

/// Single source of truth for tunnels, applications and groups. They live in
/// one small JSON document with atomic replace, so deleting a group and its
/// applications is a single write and can never be half applied.
pub struct Store {
    path: PathBuf,
    snapshot: RwLock<DockSnapshot>,
}

impl Store {
    pub async fn load(dir: &Path) -> Result<Self, StoreError> {
        tokio::fs::create_dir_all(dir).await?;
        let path = dir.join(FILE_NAME);
        let snapshot = match tokio::fs::read(&path).await {
            Ok(raw) => decode_document(&raw),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => DockSnapshot::default(),
            Err(error) => return Err(StoreError::Io(error)),
        };
        Ok(Self {
            path,
            snapshot: RwLock::new(snapshot.normalized()),
        })
    }

    pub async fn snapshot(&self) -> DockSnapshot {
        self.snapshot.read().await.clone()
    }

    pub async fn get_tunnel(&self, id: &str) -> Option<TunnelConfig> {
        self.snapshot.read().await.tunnel(id).cloned()
    }

    pub async fn get_app(&self, id: &str) -> Option<ForwardApp> {
        self.snapshot.read().await.app(id).cloned()
    }

    pub async fn upsert_tunnel(
        &self,
        mut tunnel: TunnelConfig,
    ) -> Result<TunnelConfig, StoreError> {
        tunnel.updated_at = now_millis();
        let saved = tunnel.clone();
        self.mutate(
            |doc| match doc.tunnels.iter().position(|item| item.id == saved.id) {
                Some(index) => doc.tunnels[index] = saved.clone(),
                None => doc.tunnels.push(saved.clone()),
            },
        )
        .await?;
        Ok(tunnel)
    }

    /// Removes only the tunnel definition. Callers must stop and mark the
    /// applications that referenced it before calling this.
    pub async fn delete_tunnel(&self, id: &str) -> Result<(), StoreError> {
        self.mutate(|doc| doc.tunnels.retain(|tunnel| tunnel.id != id))
            .await
    }

    pub async fn upsert_app(&self, mut app: ForwardApp) -> Result<ForwardApp, StoreError> {
        app.updated_at = now_millis();
        let saved = app.clone();
        self.mutate(
            |doc| match doc.apps.iter().position(|item| item.id == saved.id) {
                Some(index) => doc.apps[index] = saved.clone(),
                None => doc.apps.push(saved.clone()),
            },
        )
        .await?;
        Ok(app)
    }

    pub async fn delete_app(&self, id: &str) -> Result<(), StoreError> {
        self.mutate(|doc| doc.apps.retain(|app| app.id != id)).await
    }

    pub async fn set_app_desired(&self, id: &str, on: bool) -> Result<(), StoreError> {
        self.mutate(|doc| {
            if let Some(app) = doc.apps.iter_mut().find(|app| app.id == id) {
                app.desired_on = on;
                app.updated_at = now_millis();
            }
        })
        .await
    }

    pub async fn set_apps_desired(&self, ids: &[String], on: bool) -> Result<(), StoreError> {
        let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
        self.mutate(|doc| {
            for app in &mut doc.apps {
                if wanted.contains(app.id.as_str()) {
                    app.desired_on = on;
                    app.updated_at = now_millis();
                }
            }
        })
        .await
    }

    pub async fn upsert_group(&self, mut group: AppGroup) -> Result<AppGroup, StoreError> {
        group.name = group.name.trim().to_string();
        let saved = group.clone();
        self.mutate(
            |doc| match doc.groups.iter().position(|item| item.id == saved.id) {
                Some(index) => doc.groups[index] = saved.clone(),
                None => doc.groups.push(saved.clone()),
            },
        )
        .await?;
        Ok(group)
    }

    /// Deletes a group together with every application inside it. The default
    /// group is never removed.
    pub async fn delete_group(&self, id: &str) -> Result<(), StoreError> {
        if id == DEFAULT_GROUP_ID {
            return Ok(());
        }
        self.mutate(|doc| {
            doc.groups.retain(|group| group.id != id);
            doc.apps.retain(|app| app.group_id != id);
        })
        .await
    }

    pub async fn any_auto_start(&self) -> bool {
        self.snapshot
            .read()
            .await
            .apps
            .iter()
            .any(|app| app.auto_start_on_boot)
    }

    /// 用一份完整的配置文档替换当前内容（导入配置包用）。
    ///
    /// 一次性落盘，因此导入中途失败不会留下“建了组却没建应用”的半份配置；
    /// `tunnels.json` 仍是原子替换写入。
    pub async fn replace_all(&self, snapshot: DockSnapshot) -> Result<DockSnapshot, StoreError> {
        let next = snapshot.normalized();
        let mut guard = self.snapshot.write().await;
        self.persist(&next).await?;
        *guard = next.clone();
        Ok(next)
    }

    async fn mutate<F>(&self, transform: F) -> Result<(), StoreError>
    where
        F: FnOnce(&mut DockSnapshot),
    {
        let mut guard = self.snapshot.write().await;
        let mut next = guard.clone();
        transform(&mut next);
        next = next.normalized();
        self.persist(&next).await?;
        *guard = next;
        Ok(())
    }

    async fn persist(&self, snapshot: &DockSnapshot) -> Result<(), StoreError> {
        let data = serde_json::to_vec_pretty(snapshot)?;
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

/// Reads the current document, or migrates the v1 shape: a bare array where each
/// element mixed SSH access with one forwarding rule.
fn decode_document(raw: &[u8]) -> DockSnapshot {
    match serde_json::from_slice::<DockSnapshot>(raw) {
        Ok(document) => document,
        Err(error) => match serde_json::from_slice::<Vec<LegacyTunnelConfig>>(raw) {
            Ok(legacy) => {
                let migrated = migrate_legacy(legacy);
                tracing::info!(
                    tunnels = migrated.tunnels.len(),
                    apps = migrated.apps.len(),
                    "已把旧版隧道配置迁移为隧道 + 应用"
                );
                migrated
            }
            Err(_) => {
                tracing::error!(%error, "隧道配置 JSON 损坏，暂以空配置启动");
                DockSnapshot::default()
            }
        },
    }
}

fn legacy_endpoint_key(
    ssh_host: &str,
    ssh_port: u16,
    ssh_user: &str,
    credentials: &SshCredentials,
    jumps: &[JumpHost],
) -> String {
    let jump_key = jumps
        .iter()
        .map(|jump| {
            format!(
                "{}:{}:{}:{}",
                jump.host,
                jump.port,
                jump.username,
                format!("{:?}", jump.credentials.auth_type)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        ssh_host,
        ssh_port,
        ssh_user,
        format!("{:?}", credentials.auth_type),
        credentials.password_cipher.as_deref().unwrap_or(""),
        credentials.private_key_cipher.as_deref().unwrap_or(""),
        jump_key
    )
}

fn migrate_legacy(legacy: Vec<LegacyTunnelConfig>) -> DockSnapshot {
    let mut document = DockSnapshot {
        version: SNAPSHOT_VERSION,
        ..DockSnapshot::default()
    };
    let mut app_ids: HashSet<String> = HashSet::new();
    let mut legacy = legacy;
    legacy.sort_by_key(|item| item.created_at);

    for old in legacy {
        let key = legacy_endpoint_key(
            &old.ssh_host,
            old.ssh_port,
            &old.ssh_user,
            &old.credentials,
            &old.jumps,
        );
        let tunnel = document
            .tunnels
            .iter()
            .find(|tunnel| {
                legacy_endpoint_key(
                    &tunnel.ssh_host,
                    tunnel.ssh_port,
                    &tunnel.ssh_user,
                    &tunnel.credentials,
                    &tunnel.jumps,
                ) == key
            })
            .map(|tunnel| tunnel.id.clone())
            .unwrap_or_else(|| {
                let id = if document.tunnels.is_empty() {
                    old.id.clone()
                } else {
                    new_id()
                };
                let name = format!(
                    "{}@{}",
                    if old.ssh_user.trim().is_empty() {
                        "root"
                    } else {
                        old.ssh_user.trim()
                    },
                    if old.ssh_host.trim().is_empty() {
                        "SSH"
                    } else {
                        old.ssh_host.trim()
                    }
                );
                document.tunnels.push(TunnelConfig {
                    id: id.clone(),
                    name,
                    ssh_host: old.ssh_host.clone(),
                    ssh_port: old.ssh_port,
                    ssh_user: old.ssh_user.clone(),
                    credentials: old.credentials.clone(),
                    jumps: old.jumps.clone(),
                    auto_reconnect: old.auto_reconnect,
                    keep_alive_seconds: old.keep_alive_seconds,
                    connect_timeout_seconds: old.connect_timeout_seconds,
                    strict_host_key: old.strict_host_key,
                    created_at: old.created_at,
                    updated_at: old.updated_at,
                });
                id
            });

        let mut app_id = old.id;
        if !app_ids.insert(app_id.clone()) {
            app_id = new_id();
            app_ids.insert(app_id.clone());
        }
        document.apps.push(ForwardApp {
            id: app_id,
            name: old.name,
            group_id: DEFAULT_GROUP_ID.to_string(),
            tunnel_id: tunnel,
            local_port: old.local_port,
            remote_host: old.remote_host,
            remote_port: old.remote_port,
            browser_url: old.browser_url,
            auto_start_on_boot: old.auto_start_on_boot,
            desired_on: old.desired_on,
            created_at: old.created_at,
            updated_at: old.updated_at,
        });
    }
    document.normalized()
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LegacyTunnelConfig {
    id: String,
    name: String,
    local_host: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
    ssh_host: String,
    ssh_port: u16,
    ssh_user: String,
    credentials: SshCredentials,
    jumps: Vec<JumpHost>,
    auto_reconnect: bool,
    auto_start_on_boot: bool,
    keep_alive_seconds: u32,
    connect_timeout_seconds: u32,
    strict_host_key: bool,
    browser_url: String,
    desired_on: bool,
    created_at: u64,
    updated_at: u64,
}

impl Default for LegacyTunnelConfig {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: String::new(),
            local_host: crate::model::LOOPBACK.to_string(),
            local_port: 18080,
            remote_host: String::new(),
            remote_port: 80,
            ssh_host: String::new(),
            ssh_port: crate::model::DEFAULT_SSH_PORT,
            ssh_user: String::new(),
            credentials: SshCredentials::default(),
            jumps: Vec::new(),
            auto_reconnect: true,
            auto_start_on_boot: false,
            keep_alive_seconds: crate::model::DEFAULT_KEEPALIVE_SECONDS,
            connect_timeout_seconds: crate::model::DEFAULT_CONNECT_TIMEOUT_SECONDS,
            strict_host_key: true,
            browser_url: String::new(),
            desired_on: false,
            created_at: 0,
            updated_at: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AppBlockReason, AppStatus};

    fn tunnel(name: &str) -> TunnelConfig {
        TunnelConfig {
            name: name.into(),
            ssh_host: "example.com".into(),
            ssh_user: "ops".into(),
            ..TunnelConfig::default()
        }
    }

    fn app(name: &str, tunnel_id: &str) -> ForwardApp {
        ForwardApp {
            name: name.into(),
            tunnel_id: tunnel_id.into(),
            remote_host: "127.0.0.1".into(),
            ..ForwardApp::default()
        }
    }

    #[tokio::test]
    async fn persists_and_reloads_all_entities() {
        let dir = tempfile::tempdir().unwrap();
        let tunnel_id;
        {
            let store = Store::load(dir.path()).await.unwrap();
            let saved = store.upsert_tunnel(tunnel("NAS")).await.unwrap();
            tunnel_id = saved.id.clone();
            let saved_app = store
                .upsert_app(ForwardApp {
                    desired_on: true,
                    ..app("Web", &tunnel_id)
                })
                .await
                .unwrap();
            store
                .upsert_group(AppGroup {
                    name: "Prod".into(),
                    ..AppGroup::default()
                })
                .await
                .unwrap();
            store.set_app_desired(&saved_app.id, true).await.unwrap();
        }
        let store = Store::load(dir.path()).await.unwrap();
        let document = store.snapshot().await;
        assert_eq!(document.tunnels.len(), 1);
        assert_eq!(document.tunnels[0].name, "NAS");
        assert_eq!(document.apps.len(), 1);
        assert!(document.apps[0].desired_on);
        assert_eq!(document.groups.len(), 2);
        assert!(document.groups[0].is_default());
    }

    #[tokio::test]
    async fn default_group_is_always_present() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::load(dir.path()).await.unwrap();
        let document = store.snapshot().await;
        assert_eq!(document.groups.len(), 1);
        assert_eq!(document.groups[0].id, DEFAULT_GROUP_ID);
        assert_eq!(document.groups[0].name, "default");
    }

    #[tokio::test]
    async fn delete_group_removes_its_apps_but_keeps_other_groups() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::load(dir.path()).await.unwrap();
        let tunnel = store.upsert_tunnel(tunnel("NAS")).await.unwrap();
        let group = store
            .upsert_group(AppGroup {
                name: "Prod".into(),
                ..AppGroup::default()
            })
            .await
            .unwrap();
        let doomed = store.upsert_app(app("Web", &tunnel.id)).await.unwrap();
        let mut grouped = app("DB", &tunnel.id);
        grouped.group_id = group.id.clone();
        store.upsert_app(grouped).await.unwrap();
        store.upsert_app(doomed).await.unwrap();

        store.delete_group(&group.id).await.unwrap();
        let document = store.snapshot().await;
        assert!(document.group(&group.id).is_none());
        assert_eq!(document.apps.len(), 1);
        assert_eq!(document.apps[0].name, "Web");
    }

    #[tokio::test]
    async fn deleting_the_default_group_is_a_noop() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::load(dir.path()).await.unwrap();
        store.delete_group(DEFAULT_GROUP_ID).await.unwrap();
        assert!(store.snapshot().await.group(DEFAULT_GROUP_ID).is_some());
    }

    #[tokio::test]
    async fn migrates_the_legacy_tunnel_array_and_dedupes_endpoints() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = serde_json::json!([
            {
                "id": "old-1",
                "name": "Web",
                "localPort": 18080,
                "remoteHost": "127.0.0.1",
                "remotePort": 8080,
                "sshHost": "example.com",
                "sshPort": 22,
                "sshUser": "ops",
                "credentials": { "authType": "password", "passwordCipher": "v1.a.b" },
                "desiredOn": true,
                "createdAt": 10,
                "updatedAt": 10
            },
            {
                "id": "old-2",
                "name": "API",
                "localPort": 18081,
                "remoteHost": "10.0.0.5",
                "remotePort": 5000,
                "sshHost": "example.com",
                "sshPort": 22,
                "sshUser": "ops",
                "credentials": { "authType": "password", "passwordCipher": "v1.a.b" },
                "autoStartOnBoot": true,
                "createdAt": 20,
                "updatedAt": 20
            }
        ]);
        let path = dir.path().join(FILE_NAME);
        tokio::fs::create_dir_all(dir.path()).await.unwrap();
        tokio::fs::write(&path, serde_json::to_vec(&legacy).unwrap())
            .await
            .unwrap();

        let store = Store::load(dir.path()).await.unwrap();
        let document = store.snapshot().await;
        assert_eq!(document.tunnels.len(), 1, "same SSH endpoint is reused");
        assert_eq!(document.tunnels[0].id, "old-1");
        assert_eq!(document.tunnels[0].name, "ops@example.com");
        assert_eq!(document.apps.len(), 2);
        assert!(document.apps.iter().all(|app| app.tunnel_id == "old-1"));
        assert!(document.apps.iter().all(|app| app.group_id == "default"));
        assert!(document
            .apps
            .iter()
            .any(|app| app.name == "API" && app.auto_start_on_boot));
    }

    #[test]
    fn corrupt_document_falls_back_to_an_empty_dock() {
        let document = decode_document(b"{not json");
        assert!(document.tunnels.is_empty());
        assert!(document.groups.iter().any(AppGroup::is_default));
    }

    #[test]
    fn app_status_block_reason_is_serializable() {
        let status = AppStatus::blocked("a", 1, AppBlockReason::PortConflict);
        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(value["blockReason"], "port_conflict");
    }
}
