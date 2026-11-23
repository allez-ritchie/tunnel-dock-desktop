use crate::model::{
    new_id, now_millis, AppGroup, DockSnapshot, ForwardApp, JumpHost, SshCredentials, TunnelConfig,
    DEFAULT_GROUP_ID, SNAPSHOT_VERSION,
};
use std::collections::{HashMap, HashSet};
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
        self.mutate(|doc| {
            doc.tunnels.retain(|tunnel| tunnel.id != id);
            // 应用里残留的隧道引用必须一起清掉，否则界面会一直指着一个不存在的 id。
            for app in &mut doc.apps {
                app.tunnel_ids.retain(|tunnel_id| tunnel_id != id);
            }
        })
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
/// 旧文档**不会报错**——多出来的字段会被静默丢弃，得到一堆没有任何接入信息的隧道。
/// 所以必须先读 `version` 字段，低于当前版本时改用历史的布局解码，再升级成当前结构。
fn decode_document(raw: &[u8]) -> DockSnapshot {
    let version = document_version(raw);
    if version >= SNAPSHOT_VERSION {
        match serde_json::from_slice::<DockSnapshot>(raw) {
            Ok(document) => return document,
            Err(error) => tracing::warn!(%error, "当前版本的隧道配置解析失败，尝试迁移旧版"),
        }
    }
    if version >= 3 {
        match serde_json::from_slice::<DocumentV3>(raw) {
            Ok(document) => {
                let migrated = document.into_snapshot();
                tracing::info!(
                    tunnels = migrated.tunnels.len(),
                    apps = migrated.apps.len(),
                    "已把多线路隧道拆分为多条隧道，并把线路顺序移到应用上"
                );
                return migrated;
            }
            Err(error) => tracing::warn!(%error, "v3 隧道配置解析失败，尝试迁移 v2"),
        }
    }
    if version >= 2 {
        match serde_json::from_slice::<DocumentV2>(raw) {
            Ok(document) => {
                let migrated = document.into_snapshot();
                tracing::info!(
                    tunnels = migrated.tunnels.len(),
                    apps = migrated.apps.len(),
                    "已把 v2 隧道配置升级为当前结构"
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

/// v3 文档布局：隧道上挂着一组线路，应用用单个 `tunnelId` 引用隧道。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct DocumentV3 {
    tunnels: Vec<TunnelV3>,
    apps: Vec<AppV3>,
    groups: Vec<AppGroup>,
}

impl Default for DocumentV3 {
    fn default() -> Self {
        Self {
            tunnels: Vec::new(),
            apps: Vec::new(),
            groups: Vec::new(),
        }
    }
}

impl DocumentV3 {
    /// v3 → v4：每条线路变成一条独立隧道（名字带上线路名），引用旧隧道的应用
    /// 按“首选线路优先，其余按声明顺序”得到新的隧道列表，因此行为与升级前一致，
    /// 且不丢任何配置。
    fn into_snapshot(self) -> DockSnapshot {
        let mut tunnels: Vec<TunnelConfig> = Vec::new();
        // 旧隧道 id → 按尝试顺序排列的新隧道 id。
        let mut order_by_old: HashMap<String, Vec<String>> = HashMap::new();

        for old in self.tunnels {
            let ids = split_tunnel(&old, &mut tunnels);
            order_by_old.insert(old.id, ids);
        }

        let apps = self
            .apps
            .into_iter()
            .map(|old| ForwardApp {
                id: old.id,
                name: old.name,
                group_id: old.group_id,
                tunnel_ids: order_by_old
                    .get(&old.tunnel_id)
                    .cloned()
                    .unwrap_or_default(),
                local_port: old.local_port,
                remote_host: old.remote_host,
                remote_port: old.remote_port,
                browser_url: old.browser_url,
                auto_start_on_boot: old.auto_start_on_boot,
                desired_on: old.desired_on,
                created_at: old.created_at,
                updated_at: old.updated_at,
            })
            .collect();

        DockSnapshot {
            version: SNAPSHOT_VERSION,
            tunnels,
            apps,
            groups: if self.groups.is_empty() {
                vec![AppGroup::default_group()]
            } else {
                self.groups
            },
        }
        .normalized()
    }
}

/// 把一条 v3 隧道拆成一条或多条 v4 隧道，返回按尝试顺序排列的隧道 id。
///
/// 第一条线路沿用旧隧道 id（应用引用、主机密钥记录都不跳变），其余线路各建一条
/// 新隧道，名字保持可辨识。
fn split_tunnel(old: &TunnelV3, out: &mut Vec<TunnelConfig>) -> Vec<String> {
    if old.endpoints.is_empty() {
        // v3 允许“隧道还没有线路”；拆不出东西时保留一个空壳，用户可继续补全，
        // 而不是让应用凭空丢掉引用。
        out.push(TunnelConfig {
            id: old.id.clone(),
            name: old.name.clone(),
            auto_reconnect: old.auto_reconnect,
            keep_alive_seconds: old.keep_alive_seconds,
            connect_timeout_seconds: old.connect_timeout_seconds,
            strict_host_key: old.strict_host_key,
            created_at: old.created_at,
            updated_at: old.updated_at,
            ..TunnelConfig::default()
        });
        return Vec::new();
    }

    // 首选线路排在最前，其余保持声明顺序：这就是升级后应用的隧道尝试顺序。
    let preferred = old
        .preferred_endpoint_id
        .as_deref()
        .and_then(|wanted| old.endpoints.iter().position(|item| item.id == wanted));
    let mut ordered: Vec<usize> = Vec::with_capacity(old.endpoints.len());
    if let Some(index) = preferred {
        ordered.push(index);
    }
    ordered.extend((0..old.endpoints.len()).filter(|index| Some(*index) != preferred));

    let mut ids_by_index: HashMap<usize, String> = HashMap::new();
    for (position, index) in ordered.iter().copied().enumerate() {
        let endpoint = &old.endpoints[index];
        let id = if position == 0 {
            old.id.clone()
        } else {
            new_id()
        };
        let name = split_tunnel_name(&old.name, &endpoint.label, &endpoint.host);
        out.push(TunnelConfig {
            id: id.clone(),
            name,
            host: endpoint.host.clone(),
            port: endpoint.port,
            username: endpoint.username.clone(),
            credentials: endpoint.credentials.clone(),
            jumps: endpoint.jumps.clone(),
            auto_reconnect: old.auto_reconnect,
            keep_alive_seconds: old.keep_alive_seconds,
            connect_timeout_seconds: old.connect_timeout_seconds,
            strict_host_key: old.strict_host_key,
            created_at: old.created_at + position as u64,
            updated_at: old.updated_at,
        });
        ids_by_index.insert(index, id);
    }

    // 应用只引用仍然启用的线路；全都停用时保留全部，避免升级后应用直接没路可走。
    let usable: Vec<usize> = ordered
        .iter()
        .copied()
        .filter(|index| old.endpoints[*index].enabled)
        .collect();
    let wanted = if usable.is_empty() { ordered } else { usable };
    wanted
        .into_iter()
        .filter_map(|index| ids_by_index.get(&index).cloned())
        .collect()
}

/// 拆分后的隧道名：`原名 · 线路名`，名字超长时截断到 40 个字符。
fn split_tunnel_name(name: &str, label: &str, host: &str) -> String {
    let base = {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            "SSH"
        } else {
            trimmed
        }
    };
    let suffix = {
        let label = label.trim();
        if !label.is_empty() {
            label
        } else {
            host.trim()
        }
    };
    let combined = if suffix.is_empty() {
        base.to_string()
    } else {
        format!("{base} · {suffix}")
    };
    combined.chars().take(40).collect()
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct TunnelV3 {
    id: String,
    name: String,
    endpoints: Vec<EndpointV3>,
    auto_reconnect: bool,
    keep_alive_seconds: u32,
    connect_timeout_seconds: u32,
    strict_host_key: bool,
    preferred_endpoint_id: Option<String>,
    created_at: u64,
    updated_at: u64,
}

impl Default for TunnelV3 {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: String::new(),
            endpoints: Vec::new(),
            auto_reconnect: true,
            keep_alive_seconds: crate::model::DEFAULT_KEEPALIVE_SECONDS,
            connect_timeout_seconds: crate::model::DEFAULT_CONNECT_TIMEOUT_SECONDS,
            strict_host_key: true,
            preferred_endpoint_id: None,
            created_at: 0,
            updated_at: 0,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct EndpointV3 {
    id: String,
    label: String,
    host: String,
    port: u16,
    username: String,
    credentials: SshCredentials,
    jumps: Vec<JumpHost>,
    wifi_only: bool,
    enabled: bool,
}

impl Default for EndpointV3 {
    fn default() -> Self {
        Self {
            id: new_id(),
            label: String::new(),
            host: String::new(),
            port: crate::model::DEFAULT_SSH_PORT,
            username: String::new(),
            credentials: SshCredentials::default(),
            jumps: Vec::new(),
            wifi_only: false,
            enabled: true,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AppV3 {
    id: String,
    name: String,
    group_id: String,
    tunnel_id: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
    browser_url: String,
    auto_start_on_boot: bool,
    desired_on: bool,
    created_at: u64,
    updated_at: u64,
}

impl Default for AppV3 {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: String::new(),
            group_id: DEFAULT_GROUP_ID.to_string(),
            tunnel_id: String::new(),
            local_port: 18080,
            remote_host: String::new(),
            remote_port: 80,
            browser_url: String::new(),
            auto_start_on_boot: false,
            desired_on: false,
            created_at: 0,
            updated_at: 0,
        }
    }
}

/// v2 文档布局：隧道上直接存一对 SSH 接入信息。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct DocumentV2 {
    tunnels: Vec<TunnelV2>,
    apps: Vec<ForwardAppV2>,
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
    /// v2 → v4：隧道上的那对 SSH 信息就是唯一一条（也是唯一条）接入路径。
    fn into_snapshot(self) -> DockSnapshot {
        let tunnels: Vec<TunnelConfig> = self
            .tunnels
            .into_iter()
            .map(TunnelV2::into_tunnel)
            .collect();
        let known: HashSet<String> = tunnels.iter().map(|tunnel| tunnel.id.clone()).collect();
        let apps = self
            .apps
            .into_iter()
            .map(|old| ForwardApp {
                id: old.id,
                name: old.name,
                group_id: old.group_id,
                tunnel_ids: if known.contains(&old.tunnel_id) {
                    vec![old.tunnel_id]
                } else {
                    Vec::new()
                },
                local_port: old.local_port,
                remote_host: old.remote_host,
                remote_port: old.remote_port,
                browser_url: old.browser_url,
                auto_start_on_boot: old.auto_start_on_boot,
                desired_on: old.desired_on,
                created_at: old.created_at,
                updated_at: old.updated_at,
            })
            .collect();
        DockSnapshot {
            version: SNAPSHOT_VERSION,
            tunnels,
            apps,
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
        TunnelConfig {
            id: self.id,
            name: self.name,
            host: self.host,
            port: self.port,
            username: self.username,
            credentials: self.credentials,
            jumps: self.jumps,
            auto_reconnect: self.auto_reconnect,
            keep_alive_seconds: self.keep_alive_seconds,
            connect_timeout_seconds: self.connect_timeout_seconds,
            strict_host_key: self.strict_host_key,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// v2 的应用布局：单个 `tunnelId`。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ForwardAppV2 {
    id: String,
    name: String,
    group_id: String,
    tunnel_id: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
    browser_url: String,
    auto_start_on_boot: bool,
    desired_on: bool,
    created_at: u64,
    updated_at: u64,
}

impl Default for ForwardAppV2 {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: String::new(),
            group_id: DEFAULT_GROUP_ID.to_string(),
            tunnel_id: String::new(),
            local_port: 18080,
            remote_host: String::new(),
            remote_port: 80,
            browser_url: String::new(),
            auto_start_on_boot: false,
            desired_on: false,
            created_at: 0,
            updated_at: 0,
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
/// 应用各自引用它。
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
                    &tunnel.host,
                    tunnel.port,
                    &tunnel.username,
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
                    host: old.ssh_host.clone(),
                    port: old.ssh_port,
                    username: old.ssh_user.clone(),
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
            tunnel_ids: vec![tunnel],
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
    use crate::model::{AppStatus, AppBlockReason};

    fn credentials() -> SshCredentials {
        SshCredentials {
            auth_type: crate::model::AuthType::Password,
            password_cipher: Some("v1.aa.bb".into()),
            ..SshCredentials::default()
        }
    }

    fn tunnel(name: &str) -> TunnelConfig {
        TunnelConfig {
            name: name.into(),
            host: "example.com".into(),
            username: "ops".into(),
            credentials: credentials(),
            ..TunnelConfig::default()
        }
    }

    fn app(name: &str, tunnel_id: &str) -> ForwardApp {
        ForwardApp {
            name: name.into(),
            tunnel_ids: vec![tunnel_id.into()],
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
        assert_eq!(document.apps[0].tunnel_ids, vec![tunnel_id]);
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

    /// 删掉隧道时应用里的引用必须一起清掉，否则界面会指向不存在的 id。
    #[tokio::test]
    async fn deleting_a_tunnel_drops_its_reference_from_apps() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::load(dir.path()).await.unwrap();
        let home = store.upsert_tunnel(tunnel("家里")).await.unwrap();
        let away = store.upsert_tunnel(tunnel("外网")).await.unwrap();
        let saved = store
            .upsert_app(ForwardApp {
                tunnel_ids: vec![home.id.clone(), away.id.clone()],
                ..app("Web", &home.id)
            })
            .await
            .unwrap();

        store.delete_tunnel(&home.id).await.unwrap();
        let document = store.snapshot().await;
        let updated = document.app(&saved.id).unwrap();
        assert_eq!(updated.tunnel_ids, vec![away.id.clone()]);
    }

    #[tokio::test]
    async fn migrates_the_legacy_tunnel_array_and_dedupes_tunnels() {
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
        assert_eq!(document.tunnels[0].host, "example.com");
        assert_eq!(document.apps.len(), 2);
        assert!(document
            .apps
            .iter()
            .all(|app| app.tunnel_ids == vec!["old-1".to_string()]));
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

    /// 关键回归：用当前 schema 直接解码 v3 文档不会报错，只会静默丢掉
    /// `endpoints`，得到一堆空隧道。必须先看 `version` 字段。
    #[tokio::test]
    async fn migrates_a_v3_document_by_splitting_lines_into_tunnels() {
        let dir = tempfile::tempdir().unwrap();
        let v3 = serde_json::json!({
            "version": 3,
            "tunnels": [{
                "id": "t1",
                "name": "公司 NAS",
                "endpoints": [
                    {
                        "id": "home",
                        "label": "家庭局域网",
                        "host": "192.168.1.10",
                        "port": 22,
                        "username": "ops",
                        "credentials": { "authType": "password", "passwordCipher": "v1.home.data" },
                        "wifiOnly": true,
                        "enabled": true
                    },
                    {
                        "id": "away",
                        "label": "frp 外网",
                        "host": "frp.example.com",
                        "port": 2222,
                        "username": "ops",
                        "credentials": { "authType": "password", "passwordCipher": "v1.away.data" },
                        "jumps": [{
                            "id": "j1",
                            "label": "堡垒机",
                            "host": "bastion",
                            "port": 22,
                            "username": "ops",
                            "credentials": { "authType": "password", "passwordCipher": "v1.jj.kk" }
                        }],
                        "enabled": true
                    }
                ],
                "preferredEndpointId": "away",
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
                "remotePort": 5000,
                "desiredOn": true
            }],
            "groups": [{ "id": "default", "name": "default", "createdAt": 0 }]
        });
        let path = dir.path().join(FILE_NAME);
        tokio::fs::create_dir_all(dir.path()).await.unwrap();
        tokio::fs::write(&path, serde_json::to_vec(&v3).unwrap())
            .await
            .unwrap();

        let store = Store::load(dir.path()).await.unwrap();
        let document = store.snapshot().await;
        assert_eq!(document.version, SNAPSHOT_VERSION);
        assert_eq!(document.tunnels.len(), 2, "每条线路变成一条独立隧道");

        let app = &document.apps[0];
        assert_eq!(app.tunnel_ids.len(), 2);
        // 首选线路（外网）排在最前，其余按声明顺序；旧隧道 id 由首选线路继承。
        assert_eq!(app.tunnel_ids[0], "t1");
        let preferred = document.tunnel("t1").unwrap();
        assert_eq!(preferred.name, "公司 NAS · frp 外网");
        assert_eq!(preferred.host, "frp.example.com");
        assert_eq!(preferred.port, 2222);
        assert_eq!(preferred.jumps.len(), 1, "线路自己的跳板链要跟着走");
        assert!(!preferred.auto_reconnect);
        assert_eq!(preferred.keep_alive_seconds, 45);
        assert_eq!(preferred.connect_timeout_seconds, 20);
        assert!(!preferred.strict_host_key);

        let fallback = document.tunnel(&app.tunnel_ids[1]).unwrap();
        assert_eq!(fallback.name, "公司 NAS · 家庭局域网");
        assert_eq!(fallback.host, "192.168.1.10");
        assert_eq!(fallback.port, 22);
        assert_ne!(fallback.id, "t1");
    }

    /// 首页序：没有首选时按声明顺序，停用的线路不进入应用的隧道列表。
    #[test]
    fn splitting_skips_disabled_lines_but_keeps_the_declaration_order() {
        let mut out = Vec::new();
        let old = TunnelV3 {
            id: "t1".into(),
            name: "NAS".into(),
            endpoints: vec![
                EndpointV3 {
                    id: "dead".into(),
                    label: "金丝雀".into(),
                    host: "dead.example.com".into(),
                    username: "ops".into(),
                    enabled: false,
                    ..EndpointV3::default()
                },
                EndpointV3 {
                    id: "a".into(),
                    label: "甲".into(),
                    host: "a.example.com".into(),
                    username: "ops".into(),
                    ..EndpointV3::default()
                },
                EndpointV3 {
                    id: "b".into(),
                    label: "乙".into(),
                    host: "b.example.com".into(),
                    username: "ops".into(),
                    ..EndpointV3::default()
                },
            ],
            ..TunnelV3::default()
        };
        let ids = split_tunnel(&old, &mut out);
        assert_eq!(ids.len(), 2);
        assert_eq!(out.len(), 3, "停用的线路仍然保留成一条可编辑的隧道");
        assert_eq!(out[0].host, "dead.example.com");
        assert_eq!(out[1].host, "a.example.com");
        assert_eq!(out[2].host, "b.example.com");
        assert_eq!(out[1].name, "NAS · 甲");
    }

    /// 全部线路都被停用时宁可全带上，也不能让应用升级后一条路都没有。
    #[test]
    fn splitting_keeps_every_line_when_all_were_disabled() {
        let mut out = Vec::new();
        let old = TunnelV3 {
            id: "t1".into(),
            name: "NAS".into(),
            endpoints: vec![EndpointV3 {
                id: "a".into(),
                label: "甲".into(),
                host: "a.example.com".into(),
                username: "ops".into(),
                enabled: false,
                ..EndpointV3::default()
            }],
            ..TunnelV3::default()
        };
        assert_eq!(split_tunnel(&old, &mut out), vec!["t1".to_string()]);
    }

    /// v3 允许“隧道还没有线路”，拆分时保留一个空壳，应用引用不丢。
    #[test]
    fn splitting_a_tunnel_without_lines_keeps_an_empty_shell() {
        let mut out = Vec::new();
        let old = TunnelV3 {
            id: "t1".into(),
            name: "没配置".into(),
            ..TunnelV3::default()
        };
        assert!(split_tunnel(&old, &mut out).is_empty());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "t1");
        assert_eq!(out[0].name, "没配置");
        assert!(!out[0].is_dialable());
    }

    #[test]
    fn split_tunnel_names_stay_readable_and_bounded() {
        assert_eq!(split_tunnel_name("NAS", "外网", "x"), "NAS · 外网");
        assert_eq!(split_tunnel_name("NAS", "  ", "frp.example.com"), "NAS · frp.example.com");
        assert_eq!(split_tunnel_name("  ", "", ""), "SSH");
        let long = split_tunnel_name(&"名".repeat(40), "外网", "x");
        assert_eq!(long.chars().count(), 40);
    }

    /// 迁移结果只是内存中的结构；下一次写入才会把文档升级为 v4。
    #[tokio::test]
    async fn the_migrated_document_is_written_back_as_the_current_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        tokio::fs::create_dir_all(dir.path()).await.unwrap();
        let v3 = serde_json::json!({
            "version": 3,
            "tunnels": [{
                "id": "t1",
                "name": "NAS",
                "endpoints": [{
                    "id": "e1", "label": "外网", "host": "example.com", "port": 22,
                    "username": "ops",
                    "credentials": { "authType": "password", "passwordCipher": "v1.a.b" }
                }]
            }],
            "apps": [{
                "id": "a1", "name": "Web", "tunnelId": "t1", "localPort": 18080,
                "remoteHost": "127.0.0.1"
            }]
        });
        tokio::fs::write(&path, serde_json::to_vec(&v3).unwrap())
            .await
            .unwrap();
        let store = Store::load(dir.path()).await.unwrap();
        store.set_app_desired("a1", true).await.unwrap();

        let raw: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.unwrap()).unwrap();
        assert_eq!(raw["version"], SNAPSHOT_VERSION);
        assert_eq!(raw["tunnels"][0]["host"], "example.com");
        assert_eq!(raw["apps"][0]["tunnelIds"][0], "t1");
        // v3 的线路字段不再写出。
        assert!(raw["tunnels"][0].get("endpoints").is_none());
    }

    /// v2 文档：隧道上那对 SSH 信息直接成为唯一的接入路径。
    #[tokio::test]
    async fn migrates_a_v2_document_into_a_single_tunnel() {
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
        assert_eq!(tunnel.host, "example.com");
        assert_eq!(tunnel.port, 2222);
        assert_eq!(tunnel.username, "ops");
        assert_eq!(tunnel.jumps.len(), 1);
        // 隧道级设置必须原样保留，不能被默认值覆盖。
        assert!(!tunnel.auto_reconnect);
        assert_eq!(tunnel.keep_alive_seconds, 45);
        assert_eq!(tunnel.connect_timeout_seconds, 20);
        assert!(!tunnel.strict_host_key);
        assert_eq!(tunnel.created_at, 11);
        assert_eq!(document.apps.len(), 1);
        assert_eq!(document.apps[0].tunnel_ids, vec!["t1"]);
    }

    /// 一份当前文档必须原样读回，不能被任何迁移逻辑误伤。
    #[tokio::test]
    async fn a_current_document_round_trips_without_migration() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::load(dir.path()).await.unwrap();
        let saved = store.upsert_tunnel(tunnel("多入口")).await.unwrap();
        store
            .upsert_app(ForwardApp {
                tunnel_ids: vec![saved.id.clone()],
                ..app("Web", &saved.id)
            })
            .await
            .unwrap();
        let reloaded = Store::load(dir.path()).await.unwrap();
        let document = reloaded.snapshot().await;
        assert_eq!(document.tunnels.len(), 1);
        assert_eq!(document.tunnels[0].host, "example.com");
        assert_eq!(document.apps[0].tunnel_ids, vec![saved.id]);
    }

    #[test]
    fn the_document_version_is_read_from_any_layout() {
        assert_eq!(document_version(br#"{"version":4}"#), 4);
        assert_eq!(document_version(br#"{"version":3,"tunnels":[]}"#), 3);
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
