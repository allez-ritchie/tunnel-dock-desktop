use crate::model::{
    new_id, now_millis, AppGroup, DockSnapshot, ForwardApp, JumpHost, SshCredentials,
    TunnelConfig, TunnelEndpoint, DEFAULT_GROUP_ID, SNAPSHOT_VERSION,
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

/// Reads the stored document in whatever layout it was written with.
///
/// **关键坑**：`TunnelConfig` 的所有字段都有默认值，因此用当前 schema 直接解码一份
/// v2 文档**不会报错**——`host`/`port`/`username` 这些字段会被静默丢弃，得到一堆
/// 没有任何线路的隧道。所以必须先读 `version` 字段，低于当前版本时改用历史的
/// `DocumentV2` 布局来解码，再升级成当前结构。
fn decode_document(raw: &[u8]) -> DockSnapshot {
    let version = document_version(raw);
    if version >= SNAPSHOT_VERSION {
        match serde_json::from_slice::<DockSnapshot>(raw) {
            Ok(document) => return document,
            Err(error) => tracing::warn!(%error, "当前版本的隧道配置解析失败，尝试迁移旧版"),
        }
    }
    if version >= 2 {
        match serde_json::from_slice::<DocumentV2>(raw) {
            Ok(document) => {
                let migrated = document.into_snapshot();
                tracing::info!(
                    tunnels = migrated.tunnels.len(),
                    apps = migrated.apps.len(),
                    "已把 v2 隧道配置升级为多线路结构"
                );
                return migrated;
            }
            Err(error) => tracing::warn!(%error, "v2 隧道配置解析失败，尝试迁移 v1"),
        }
    }
    match serde_json::from_slice::<Vec<LegacyTunnelConfig>>(raw) {
        Ok(legacy) => {
            let migrated = migrate_legacy(legacy);
            tracing::info!(
                tunnels = migrated.tunnels.len(),
                apps = migrated.apps.len(),
                "已把旧版隧道配置迁移为隧道 + 应用"
            );
            migrated
        }
        Err(error) => {
            tracing::error!(%error, "隧道配置 JSON 损坏，暂以空配置启动");
            DockSnapshot::default()
        }
    }
}

/// 读取文档的 `version` 字段；没有该字段的老格式按 0 处理。
fn document_version(raw: &[u8]) -> u32 {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return 0;
    };
    value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .map(|version| version.min(u32::MAX as u64) as u32)
        .unwrap_or(0)
}

/// v2 文档布局：隧道上直接存一对 SSH 接入信息。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct DocumentV2 {
    tunnels: Vec<TunnelV2>,
    apps: Vec<ForwardApp>,
    groups: Vec<AppGroup>,
}

impl Default for DocumentV2 {
    fn default() -> Self {
        Self {
            tunnels: Vec::new(),
            apps: Vec::new(),
            groups: Vec::new(),
        }
    }
}

impl DocumentV2 {
    /// v2 → v3：隧道上的 `host`/`port`/`username`/`credentials`/`jumps` 变成
    /// 唯一的一条线路，命名为隧道名。用户添加第二条线路之前，行为完全不变。
    fn into_snapshot(self) -> DockSnapshot {
        DockSnapshot {
            version: SNAPSHOT_VERSION,
            tunnels: self.tunnels.into_iter().map(TunnelV2::into_tunnel).collect(),
            apps: self.apps,
            groups: if self.groups.is_empty() {
                vec![AppGroup::default_group()]
            } else {
                self.groups
            },
        }
        .normalized()
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct TunnelV2 {
    id: String,
    name: String,
    host: String,
    port: u16,
    username: String,
    credentials: SshCredentials,
    jumps: Vec<JumpHost>,
    auto_reconnect: bool,
    keep_alive_seconds: u32,
    connect_timeout_seconds: u32,
    strict_host_key: bool,
    created_at: u64,
    updated_at: u64,
}

impl Default for TunnelV2 {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: String::new(),
            host: String::new(),
            port: crate::model::DEFAULT_SSH_PORT,
            username: String::new(),
            credentials: SshCredentials::default(),
            jumps: Vec::new(),
            auto_reconnect: true,
            keep_alive_seconds: crate::model::DEFAULT_KEEPALIVE_SECONDS,
            connect_timeout_seconds: crate::model::DEFAULT_CONNECT_TIMEOUT_SECONDS,
            strict_host_key: true,
            created_at: 0,
            updated_at: 0,
        }
    }
}

impl TunnelV2 {
    fn into_tunnel(self) -> TunnelConfig {
        let label = if self.name.trim().is_empty() {
            self.host.trim().to_string()
        } else {
            self.name.trim().to_string()
        };
        TunnelConfig {
            id: self.id,
            name: self.name,
            endpoints: vec![TunnelEndpoint {
                label,
                host: self.host,
                port: self.port,
                username: self.username,
                credentials: self.credentials,
                jumps: self.jumps,
                wifi_only: false,
                enabled: true,
                ..TunnelEndpoint::default()
            }],
            auto_reconnect: self.auto_reconnect,
            keep_alive_seconds: self.keep_alive_seconds,
            connect_timeout_seconds: self.connect_timeout_seconds,
            strict_host_key: self.strict_host_key,
            preferred_endpoint_id: None,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
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

/// v1 迁移：同一个 SSH 接入信息被多条旧配置共用时，只保留一条隧道，
/// 并把它当作该隧道唯一的一条线路。
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
                tunnel.endpoints.iter().any(|endpoint| {
                    legacy_endpoint_key(
                        &endpoint.host,
                        endpoint.port,
                        &endpoint.username,
                        &endpoint.credentials,
                        &endpoint.jumps,
                    ) == key
                })
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
                    name: name.clone(),
                    endpoints: vec![TunnelEndpoint {
                        label: name,
                        host: old.ssh_host.clone(),
                        port: old.ssh_port,
                        username: old.ssh_user.clone(),
                        credentials: old.credentials.clone(),
                        jumps: old.jumps.clone(),
                        wifi_only: false,
                        enabled: true,
                        ..TunnelEndpoint::default()
                    }],
                    auto_reconnect: old.auto_reconnect,
                    keep_alive_seconds: old.keep_alive_seconds,
                    connect_timeout_seconds: old.connect_timeout_seconds,
                    strict_host_key: old.strict_host_key,
                    preferred_endpoint_id: None,
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
            endpoints: vec![TunnelEndpoint {
                label: name.into(),
                host: "example.com".into(),
                username: "ops".into(),
                credentials: SshCredentials {
                    auth_type: crate::model::AuthType::Password,
                    password_cipher: Some("v1.aa.bb".into()),
                    ..SshCredentials::default()
                },
                ..TunnelEndpoint::default()
            }],
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
        assert_eq!(document.tunnels[0].endpoints.len(), 1);
        assert_eq!(document.tunnels[0].endpoints[0].host, "example.com");
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

    /// 关键回归：用当前 schema 直接解码 v2 文档不会报错，只会静默丢掉
    /// `host`/`port`/`username`，得到一堆空隧道。必须先看 `version` 字段。
    #[tokio::test]
    async fn migrates_a_v2_document_into_a_single_endpoint() {
        let dir = tempfile::tempdir().unwrap();
        let v2 = serde_json::json!({
            "version": 2,
            "tunnels": [{
                "id": "t1",
                "name": "公司跳板",
                "host": "example.com",
                "port": 2222,
                "username": "ops",
                "credentials": { "authType": "password", "passwordCipher": "v1.aa.bb" },
                "jumps": [{
                    "id": "j1",
                    "label": "堡垒机",
                    "host": "bastion",
                    "port": 22,
                    "username": "ops",
                    "credentials": { "authType": "password", "passwordCipher": "v1.jj.kk" }
                }],
                "autoReconnect": false,
                "keepAliveSeconds": 45,
                "connectTimeoutSeconds": 20,
                "strictHostKey": false,
                "createdAt": 11,
                "updatedAt": 12
            }],
            "apps": [{
                "id": "a1",
                "name": "NAS",
                "groupId": "default",
                "tunnelId": "t1",
                "localPort": 18080,
                "remoteHost": "10.0.0.5",
                "remotePort": 5000
            }],
            "groups": [{ "id": "default", "name": "default", "createdAt": 0 }]
        });
        let path = dir.path().join(FILE_NAME);
        tokio::fs::create_dir_all(dir.path()).await.unwrap();
        tokio::fs::write(&path, serde_json::to_vec(&v2).unwrap())
            .await
            .unwrap();

        let store = Store::load(dir.path()).await.unwrap();
        let document = store.snapshot().await;
        assert_eq!(document.version, SNAPSHOT_VERSION);
        assert_eq!(document.tunnels.len(), 1);
        let tunnel = &document.tunnels[0];
        assert_eq!(tunnel.name, "公司跳板");
        assert_eq!(tunnel.endpoints.len(), 1, "v2 的一对 SSH 信息变成一条线路");
        assert_eq!(tunnel.endpoints[0].host, "example.com");
        assert_eq!(tunnel.endpoints[0].port, 2222);
        assert_eq!(tunnel.endpoints[0].username, "ops");
        assert_eq!(tunnel.endpoints[0].label, "公司跳板");
        assert_eq!(tunnel.endpoints[0].jumps.len(), 1);
        assert!(tunnel.endpoints[0].enabled);
        assert!(!tunnel.endpoints[0].wifi_only);
        assert!(tunnel.preferred_endpoint_id.is_none());
        // 隧道级设置必须原样保留，不能被默认值覆盖。
        assert!(!tunnel.auto_reconnect);
        assert_eq!(tunnel.keep_alive_seconds, 45);
        assert_eq!(tunnel.connect_timeout_seconds, 20);
        assert!(!tunnel.strict_host_key);
        assert_eq!(tunnel.created_at, 11);
        assert_eq!(document.apps.len(), 1);
        assert_eq!(document.apps[0].tunnel_id, "t1");
    }

    /// 迁移结果只是内存中的结构；下一次写入才会把文档升级为 v3。
    #[tokio::test]
    async fn the_migrated_document_is_written_back_as_v3() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        tokio::fs::create_dir_all(dir.path()).await.unwrap();
        let v2 = serde_json::json!({
            "version": 2,
            "tunnels": [{
                "id": "t1", "name": "NAS", "host": "example.com", "port": 22, "username": "ops"
            }]
        });
        tokio::fs::write(&path, serde_json::to_vec(&v2).unwrap())
            .await
            .unwrap();
        let store = Store::load(dir.path()).await.unwrap();
        store.set_app_desired("nothing", true).await.unwrap();

        let raw: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.unwrap()).unwrap();
        assert_eq!(raw["version"], SNAPSHOT_VERSION);
        assert_eq!(raw["tunnels"][0]["endpoints"][0]["host"], "example.com");
        // v2 的扁平字段不再写出。
        assert!(raw["tunnels"][0].get("sshHost").is_none());
    }

    /// 一份当前文档必须原样读回，不能被 v2 迁移逻辑误伤。
    #[tokio::test]
    async fn a_current_document_round_trips_without_migration() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::load(dir.path()).await.unwrap();
        store.upsert_tunnel(tunnel("多线路")).await.unwrap();
        let reloaded = Store::load(dir.path()).await.unwrap();
        let document = reloaded.snapshot().await;
        assert_eq!(document.tunnels.len(), 1);
        assert_eq!(document.tunnels[0].endpoints.len(), 1);
        assert_eq!(document.tunnels[0].endpoints[0].host, "example.com");
    }

    #[test]
    fn the_document_version_is_read_from_any_layout() {
        assert_eq!(document_version(br#"{"version":3}"#), 3);
        assert_eq!(document_version(br#"{"version":2,"tunnels":[]}"#), 2);
        assert_eq!(document_version(br#"{"tunnels":[]}"#), 0);
        assert_eq!(document_version(br#"[{"id":"x"}]"#), 0);
        assert_eq!(document_version(b"not json"), 0);
    }

    #[test]
    fn app_status_block_reason_is_serializable() {
        let status = AppStatus::blocked("a", 1, AppBlockReason::PortConflict);
        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(value["blockReason"], "port_conflict");
    }
}
