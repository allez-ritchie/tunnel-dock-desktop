use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_SSH_PORT: u16 = 22;
pub const DEFAULT_KEEPALIVE_SECONDS: u32 = 30;
pub const DEFAULT_CONNECT_TIMEOUT_SECONDS: u32 = 15;
pub const LOOPBACK: &str = "127.0.0.1";
pub const PORT_PLACEHOLDER: &str = "{port}";
pub const DEFAULT_GROUP_ID: &str = "default";
pub const DEFAULT_GROUP_NAME: &str = "default";
/// 文档版本历史：
///  - 1：裸数组，SSH 接入信息与转发规则混在一条配置里；
///  - 2：隧道 / 应用 / 应用组拆成独立实体；
///  - 3：一条隧道持有**一组有序的 SSH 线路**，应用配置不变，因此本地端口对第三方透明。
pub const SNAPSHOT_VERSION: u32 = 3;

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    Password,
    PrivateKey,
}

impl Default for AuthType {
    fn default() -> Self {
        AuthType::Password
    }
}

/// Ciphertext produced by `secrets.rs`. Plaintext never reaches this struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SshCredentials {
    pub auth_type: AuthType,
    pub password_cipher: Option<String>,
    pub private_key_cipher: Option<String>,
    pub passphrase_cipher: Option<String>,
}

impl SshCredentials {
    pub fn has_secret(&self) -> bool {
        match self.auth_type {
            AuthType::Password => self.password_cipher.is_some(),
            AuthType::PrivateKey => self.private_key_cipher.is_some(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JumpHost {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credentials: SshCredentials,
}

impl Default for JumpHost {
    fn default() -> Self {
        Self {
            id: new_id(),
            label: String::new(),
            host: String::new(),
            port: DEFAULT_SSH_PORT,
            username: String::new(),
            credentials: SshCredentials::default(),
        }
    }
}

impl JumpHost {
    pub fn display_name(&self) -> &str {
        if self.label.is_empty() {
            &self.host
        } else {
            &self.label
        }
    }
}

/// 一条 SSH 线路：如何到达同一台 SSH 服务器（用户名 / 地址 / 端口 / 跳板 / 凭据）。
///
/// 一条隧道可以配置多条线路，让同一个应用在**本地端口不变**的前提下，按场景切换
/// SSH 入口（家里走局域网，外出走 frp 之类）。选路由连接时决定，应用配置完全无感。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TunnelEndpoint {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credentials: SshCredentials,
    pub jumps: Vec<JumpHost>,
    /// 只在连上局域网（家里的服务器）时才尝试这条线路。
    pub wifi_only: bool,
    /// 停用后保留配置，但连接时不会被拨号。
    pub enabled: bool,
}

impl Default for TunnelEndpoint {
    fn default() -> Self {
        Self {
            id: new_id(),
            label: String::new(),
            host: String::new(),
            port: DEFAULT_SSH_PORT,
            username: String::new(),
            credentials: SshCredentials::default(),
            jumps: Vec::new(),
            wifi_only: false,
            enabled: true,
        }
    }
}

impl TunnelEndpoint {
    /// `user@host:port`，界面上展示线路的稳定标识。
    pub fn endpoint(&self) -> String {
        format!("{}@{}:{}", self.username, self.host, self.port)
    }

    pub fn display_name(&self) -> &str {
        let label = self.label.trim();
        if !label.is_empty() {
            return label;
        }
        let host = self.host.trim();
        if host.is_empty() {
            "?"
        } else {
            host
        }
    }

    /// 完整链路：从第一级跳板机到最终的 SSH 服务器。
    pub fn chain_summary(&self) -> String {
        let mut hops: Vec<&str> = self.jumps.iter().map(|jump| jump.host.as_str()).collect();
        hops.push(self.host.as_str());
        hops.join("  →  ")
    }

    /// russh 是否具备发起连接的全部条件。
    pub fn is_dialable(&self) -> bool {
        self.enabled
            && !self.host.trim().is_empty()
            && !self.host.contains(char::is_whitespace)
            && !self.username.trim().is_empty()
            && self.port > 0
            && self.credentials.has_secret()
    }
}

/// A reusable SSH access definition: one application-facing name plus the
/// ordered list of endpoints that can reach the same SSH server.
///
/// A tunnel owns no local port and no remote target; applications reference it
/// and add their own `localPort -> remoteHost:remotePort` rule, so the third
/// party on the local port never notices which path is carrying the traffic.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TunnelConfig {
    pub id: String,
    pub name: String,
    /// 按尝试顺序排列的线路；第一条是默认线路。
    pub endpoints: Vec<TunnelEndpoint>,
    pub auto_reconnect: bool,
    pub keep_alive_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub strict_host_key: bool,
    /// 用户手工钉住的首选线路；`None` 表示自动选择。
    pub preferred_endpoint_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        let now = now_millis();
        Self {
            id: new_id(),
            name: String::new(),
            endpoints: Vec::new(),
            auto_reconnect: true,
            keep_alive_seconds: DEFAULT_KEEPALIVE_SECONDS,
            connect_timeout_seconds: DEFAULT_CONNECT_TIMEOUT_SECONDS,
            strict_host_key: true,
            preferred_endpoint_id: None,
            created_at: now,
            updated_at: now,
        }
    }
}

impl TunnelConfig {
    pub fn endpoint_by_id(&self, id: Option<&str>) -> Option<&TunnelEndpoint> {
        let wanted = id?;
        self.endpoints.iter().find(|endpoint| endpoint.id == wanted)
    }

    /// 尚未连接时界面应该描述的线路：首选线路，否则第一条。
    pub fn primary_endpoint(&self) -> Option<&TunnelEndpoint> {
        self.endpoint_by_id(self.preferred_endpoint_id.as_deref())
            .or_else(|| self.endpoints.first())
    }

    /// 首选线路的 `user@host:port`，用于卡片与列表。
    pub fn endpoint(&self) -> String {
        self.primary_endpoint()
            .map(TunnelEndpoint::endpoint)
            .unwrap_or_default()
    }

    /// 首选线路的完整链路。
    pub fn chain_summary(&self) -> String {
        self.primary_endpoint()
            .map(TunnelEndpoint::chain_summary)
            .unwrap_or_default()
    }

    /// 配置完整、可以被拨号的线路，保持声明顺序。
    pub fn dialable_endpoints(&self) -> Vec<&TunnelEndpoint> {
        self.endpoints
            .iter()
            .filter(|endpoint| endpoint.is_dialable())
            .collect()
    }

    /// 定义层面是否还能连上：至少有一条线路带凭据。
    pub fn is_dialable(&self) -> bool {
        self.endpoints.iter().any(TunnelEndpoint::is_dialable)
    }

    pub fn with_updated_at(mut self) -> Self {
        self.updated_at = now_millis();
        self
    }
}

/// One forwarded `localPort -> remoteHost:remotePort` rule. The SSH access
/// lives in [`TunnelConfig`]; an application only picks one and adds a target.
/// `desired_on` is the persisted user intent, never live state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ForwardApp {
    pub id: String,
    pub name: String,
    pub group_id: String,
    pub tunnel_id: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub browser_url: String,
    pub auto_start_on_boot: bool,
    pub desired_on: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Default for ForwardApp {
    fn default() -> Self {
        let now = now_millis();
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
            created_at: now,
            updated_at: now,
        }
    }
}

impl ForwardApp {
    pub fn resolved_browser_url(&self) -> String {
        let custom = self.browser_url.trim();
        if custom.is_empty() {
            return self.local_url();
        }
        custom.replace(PORT_PLACEHOLDER, &self.local_port.to_string())
    }

    pub fn local_url(&self) -> String {
        format!("http://{}:{}/", LOOPBACK, self.local_port)
    }

    pub fn with_desired(mut self, on: bool) -> Self {
        self.desired_on = on;
        self.updated_at = now_millis();
        self
    }
}

/// A user-defined bucket of applications used for one-tap start / stop.
/// The `default` group always exists and cannot be deleted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppGroup {
    pub id: String,
    pub name: String,
    pub created_at: u64,
}

impl Default for AppGroup {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: String::new(),
            created_at: now_millis(),
        }
    }
}

impl AppGroup {
    pub fn default_group() -> Self {
        Self {
            id: DEFAULT_GROUP_ID.to_string(),
            name: DEFAULT_GROUP_NAME.to_string(),
            created_at: 0,
        }
    }

    pub fn is_default(&self) -> bool {
        self.id == DEFAULT_GROUP_ID
    }
}

/// Everything the user configured, persisted as one atomic JSON document so a
/// multi-entity change (delete a group plus its apps) cannot be half written.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DockSnapshot {
    pub version: u32,
    pub tunnels: Vec<TunnelConfig>,
    pub apps: Vec<ForwardApp>,
    pub groups: Vec<AppGroup>,
}

impl Default for DockSnapshot {
    fn default() -> Self {
        Self {
            version: SNAPSHOT_VERSION,
            tunnels: Vec::new(),
            apps: Vec::new(),
            groups: vec![AppGroup::default_group()],
        }
    }
}

impl DockSnapshot {
    pub fn tunnel(&self, id: &str) -> Option<&TunnelConfig> {
        self.tunnels.iter().find(|tunnel| tunnel.id == id)
    }

    pub fn app(&self, id: &str) -> Option<&ForwardApp> {
        self.apps.iter().find(|app| app.id == id)
    }

    pub fn group(&self, id: &str) -> Option<&AppGroup> {
        self.groups.iter().find(|group| group.id == id)
    }

    pub fn apps_using_tunnel(&self, tunnel_id: &str) -> Vec<ForwardApp> {
        self.apps
            .iter()
            .filter(|app| app.tunnel_id == tunnel_id)
            .cloned()
            .collect()
    }

    /// Finds the tunnel (and the endpoint of it) that defines the SSH access of
    /// a jump host.
    ///
    /// Importing `~/.ssh/config` creates one tunnel per `Host` alias and turns
    /// `ProxyJump` aliases into jump hosts. Those jump hosts carry no secret
    /// (the file has none), yet the same host exists as its own tunnel where the
    /// user *did* enter the password. Matching the jump back to that endpoint is
    /// what lets one password serve every tunnel jumping through it.
    ///
    /// The exact `user@host:port` match wins because it identifies the same SSH
    /// account unambiguously; the alias is the fallback for jump hosts whose
    /// address was edited after the import.
    pub fn credential_source_for_jump<'a>(
        &'a self,
        owner: &TunnelConfig,
        jump: &JumpHost,
    ) -> Option<&'a TunnelEndpoint> {
        let owner_id = owner.id.as_str();
        let host = jump.host.trim();
        let username = jump.username.trim();
        let endpoint_matches = |endpoint: &&TunnelEndpoint| {
            !host.is_empty()
                && endpoint.host.trim().eq_ignore_ascii_case(host)
                && endpoint.port == jump.port
                && endpoint.username.trim() == username
                && endpoint.credentials.has_secret()
        };
        let label = jump.label.trim();
        let name_matches = |endpoint: &&TunnelEndpoint| {
            let name = endpoint.display_name();
            !label.is_empty() && name.trim().eq_ignore_ascii_case(label)
        };
        // 这些候选都来自“别的隧道”，否则会自己引用自己。
        let external = || {
            self.tunnels
                .iter()
                .filter(|tunnel| tunnel.id != owner_id)
                .flat_map(|tunnel| tunnel.endpoints.iter())
                .filter(|endpoint| endpoint.credentials.has_secret())
        };
        external()
            .clone()
            .find(endpoint_matches)
            .or_else(|| external().find(|endpoint| name_matches(endpoint)))
    }

    /// 跳板机凭据来自哪条隧道（用于界面提示“凭据复用自…”）。
    pub fn credential_source_name_for_jump(
        &self,
        owner: &TunnelConfig,
        jump: &JumpHost,
    ) -> Option<String> {
        let endpoint = self.credential_source_for_jump(owner, jump)?;
        self.tunnels
            .iter()
            .find(|tunnel| {
                tunnel.id != owner.id
                    && tunnel
                        .endpoints
                        .iter()
                        .any(|candidate| candidate.id == endpoint.id)
            })
            .map(|tunnel| tunnel.name.clone())
    }

    /// Returns `owner` with every secret-less jump host taking its credentials
    /// from another tunnel that defines that host. Used by both the connect path
    /// and the UI state, so "已配置密码" always means the same thing.
    pub fn with_inherited_jump_credentials(&self, owner: &TunnelConfig) -> TunnelConfig {
        let mut resolved = owner.clone();
        for endpoint in &mut resolved.endpoints {
            let mut inherited: Vec<(usize, SshCredentials)> = Vec::new();
            for (index, jump) in endpoint.jumps.iter().enumerate() {
                if jump.credentials.has_secret() {
                    continue;
                }
                if let Some(source) = self.credential_source_for_jump(owner, jump) {
                    inherited.push((index, source.credentials.clone()));
                }
            }
            for (index, credentials) in inherited {
                endpoint.jumps[index].credentials = credentials;
            }
        }
        resolved
    }

    /// The default group always exists and comes first; dangling app group
    /// references fall back to it instead of disappearing from the UI.
    pub fn normalized(mut self) -> Self {
        if self.version == 0 {
            self.version = SNAPSHOT_VERSION;
        }
        if self.groups.is_empty() {
            self.groups.push(AppGroup::default_group());
        } else if !self.groups.iter().any(AppGroup::is_default) {
            self.groups.push(AppGroup::default_group());
        }
        self.groups.sort_by_key(|group| {
            (
                !group.is_default(),
                group.created_at,
                group.name.clone(),
                group.id.clone(),
            )
        });
        let group_ids: Vec<String> = self.groups.iter().map(|group| group.id.clone()).collect();
        for app in &mut self.apps {
            if !group_ids.contains(&app.group_id) {
                app.group_id = DEFAULT_GROUP_ID.to_string();
            }
        }
        self.tunnels.sort_by_key(|tunnel| tunnel.created_at);
        self.apps.sort_by_key(|app| app.created_at);
        self
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialState {
    pub auth_type: AuthType,
    pub has_password: bool,
    pub has_private_key: bool,
    pub has_passphrase: bool,
}

impl From<&SshCredentials> for CredentialState {
    fn from(value: &SshCredentials) -> Self {
        Self {
            auth_type: value.auth_type,
            has_password: value.password_cipher.is_some(),
            has_private_key: value.private_key_cipher.is_some(),
            has_passphrase: value.passphrase_cipher.is_some(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JumpView {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credentials: CredentialState,
    /// 凭据来自哪个隧道（跳板机与某个隧道是同一台主机时）。`None` 表示凭据自己的。
    pub inherited_from: Option<String>,
}

impl From<&JumpHost> for JumpView {
    fn from(value: &JumpHost) -> Self {
        Self {
            id: value.id.clone(),
            label: value.label.clone(),
            host: value.host.clone(),
            port: value.port,
            username: value.username.clone(),
            credentials: CredentialState::from(&value.credentials),
            inherited_from: None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointView {
    pub id: String,
    pub label: String,
    pub display_name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credentials: CredentialState,
    pub jumps: Vec<JumpView>,
    pub wifi_only: bool,
    pub enabled: bool,
    /// `user@host:port`，列表里直接展示。
    pub endpoint: String,
    pub chain_summary: String,
    /// 定义完整、当前可以被拨号（启用 + 有地址/用户/凭据）。
    pub dialable: bool,
}

impl EndpointView {
    fn from_resolved(value: &TunnelEndpoint) -> Self {
        Self {
            id: value.id.clone(),
            label: value.label.clone(),
            display_name: value.display_name().to_string(),
            host: value.host.clone(),
            port: value.port,
            username: value.username.clone(),
            credentials: CredentialState::from(&value.credentials),
            jumps: value.jumps.iter().map(JumpView::from).collect(),
            wifi_only: value.wifi_only,
            enabled: value.enabled,
            endpoint: value.endpoint(),
            chain_summary: value.chain_summary(),
            dialable: value.is_dialable(),
        }
    }
}

/// Frontend-safe tunnel representation: secrets are reduced to presence flags.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelView {
    pub id: String,
    pub name: String,
    pub endpoints: Vec<EndpointView>,
    pub auto_reconnect: bool,
    pub keep_alive_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub strict_host_key: bool,
    pub preferred_endpoint_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    /// 首选线路（或第一条）的 `user@host:port`。
    pub endpoint: String,
    pub chain_summary: String,
    /// 配置了但当前不可拨号的线路数量，卡片据此提示“需补全”。
    pub dialable_count: usize,
}

impl From<&TunnelConfig> for TunnelView {
    fn from(value: &TunnelConfig) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            endpoints: value
                .endpoints
                .iter()
                .map(EndpointView::from_resolved)
                .collect(),
            auto_reconnect: value.auto_reconnect,
            keep_alive_seconds: value.keep_alive_seconds,
            connect_timeout_seconds: value.connect_timeout_seconds,
            strict_host_key: value.strict_host_key,
            preferred_endpoint_id: value.preferred_endpoint_id.clone(),
            created_at: value.created_at,
            updated_at: value.updated_at,
            endpoint: value.endpoint(),
            chain_summary: value.chain_summary(),
            dialable_count: value.dialable_endpoints().len(),
        }
    }
}

impl TunnelView {
    /// Builds the frontend view with jump credentials resolved to their final,
    /// effective state so the editor can say "已保存密码" when the secret actually
    /// comes from another tunnel.
    pub fn resolved(value: &TunnelConfig, snapshot: &DockSnapshot) -> Self {
        let resolved = snapshot.with_inherited_jump_credentials(value);
        let mut view = TunnelView::from(&resolved);
        for (endpoint_view, original) in view.endpoints.iter_mut().zip(value.endpoints.iter()) {
            for (jump_view, original_jump) in
                endpoint_view.jumps.iter_mut().zip(original.jumps.iter())
            {
                if original_jump.credentials.has_secret() {
                    continue;
                }
                let effective = &jump_view.credentials;
                if !(effective.has_password || effective.has_private_key) {
                    continue;
                }
                jump_view.inherited_from =
                    snapshot.credential_source_name_for_jump(value, original_jump);
            }
        }
        view
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppView {
    pub id: String,
    pub name: String,
    pub group_id: String,
    pub tunnel_id: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub browser_url: String,
    pub browser_open_url: String,
    pub local_url: String,
    pub auto_start_on_boot: bool,
    pub desired_on: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

impl From<&ForwardApp> for AppView {
    fn from(value: &ForwardApp) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            group_id: value.group_id.clone(),
            tunnel_id: value.tunnel_id.clone(),
            local_port: value.local_port,
            remote_host: value.remote_host.clone(),
            remote_port: value.remote_port,
            browser_url: value.browser_url.clone(),
            browser_open_url: value.resolved_browser_url(),
            local_url: value.local_url(),
            auto_start_on_boot: value.auto_start_on_boot,
            desired_on: value.desired_on,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupView {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub created_at: u64,
}

impl From<&AppGroup> for GroupView {
    fn from(value: &AppGroup) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            is_default: value.is_default(),
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelPhase {
    Idle,
    Connecting,
    Connected,
    Reconnecting,
    Error,
}

impl TunnelPhase {
    pub fn is_live(self) -> bool {
        matches!(
            self,
            TunnelPhase::Connecting | TunnelPhase::Connected | TunnelPhase::Reconnecting
        )
    }
}

/// Why an application cannot start or why it just stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppBlockReason {
    None,
    /// The referenced tunnel definition no longer exists.
    TunnelDeleted,
    /// The referenced app group no longer exists.
    GroupMissing,
    /// The tunnel exists but dialling it failed.
    TunnelDown,
    /// The forward itself failed (remote refused, target invalid, …).
    ForwardFailed,
    /// Another desired application claims the same local port.
    PortConflict,
}

impl AppBlockReason {
    pub fn error_code(self) -> Option<&'static str> {
        match self {
            AppBlockReason::None => None,
            AppBlockReason::TunnelDeleted => Some("tunnel_deleted"),
            AppBlockReason::GroupMissing => Some("group_missing"),
            AppBlockReason::TunnelDown => Some("tunnel_down"),
            AppBlockReason::ForwardFailed => Some("forward_failed"),
            AppBlockReason::PortConflict => Some("port_conflict"),
        }
    }
}

/// 某一轮连接里，一条线路为什么没能建立连接。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointFailure {
    pub endpoint_id: String,
    pub label: String,
    /// 语言无关的错误码，文案由界面提供。
    pub code: String,
    pub detail: Option<String>,
}

/// 当前正在承载流量的线路，让卡片、错误详情页都能说清“走的是哪条路”。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointSelection {
    pub endpoint_id: String,
    pub label: String,
    pub endpoint: String,
}

impl EndpointSelection {
    pub fn of(endpoint: &TunnelEndpoint) -> Self {
        Self {
            endpoint_id: endpoint.id.clone(),
            label: endpoint.display_name().to_string(),
            endpoint: endpoint.endpoint(),
        }
    }
}

/// Live state of one forwarded application. Each application owns its own SSH
/// connection and local listener; a shared tunnel is only shared config.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub app_id: String,
    pub phase: TunnelPhase,
    pub block_reason: AppBlockReason,
    pub attempt: u32,
    pub next_retry_seconds: Option<u32>,
    pub connected_at: Option<u64>,
    pub local_port: Option<u16>,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub security_alert: bool,
    /// 当前（或最后一次尝试）承载转发的线路。
    pub endpoint: Option<EndpointSelection>,
    /// 本轮已经失败的线路，按尝试顺序。
    pub endpoint_failures: Vec<EndpointFailure>,
    pub updated_at: u64,
}

impl AppStatus {
    fn base(app_id: &str, phase: TunnelPhase) -> Self {
        Self {
            app_id: app_id.to_string(),
            phase,
            block_reason: AppBlockReason::None,
            attempt: 0,
            next_retry_seconds: None,
            connected_at: None,
            local_port: None,
            error_code: None,
            error_detail: None,
            security_alert: false,
            endpoint: None,
            endpoint_failures: Vec::new(),
            updated_at: now_millis(),
        }
    }

    pub fn idle(app_id: &str) -> Self {
        Self::base(app_id, TunnelPhase::Idle)
    }

    pub fn connecting(app_id: &str, local_port: u16) -> Self {
        Self {
            local_port: Some(local_port),
            ..Self::base(app_id, TunnelPhase::Connecting)
        }
    }

    pub fn connected(app_id: &str, local_port: u16) -> Self {
        Self {
            phase: TunnelPhase::Connected,
            connected_at: Some(now_millis()),
            local_port: Some(local_port),
            ..Self::base(app_id, TunnelPhase::Connected)
        }
    }

    pub fn reconnecting(
        app_id: &str,
        attempt: u32,
        wait_seconds: u32,
        local_port: u16,
        error_code: Option<String>,
        error_detail: Option<String>,
        block_reason: AppBlockReason,
    ) -> Self {
        Self {
            attempt,
            next_retry_seconds: Some(wait_seconds),
            error_code,
            error_detail,
            block_reason,
            local_port: Some(local_port),
            ..Self::base(app_id, TunnelPhase::Reconnecting)
        }
    }

    pub fn error(
        app_id: &str,
        local_port: u16,
        error_code: String,
        error_detail: Option<String>,
        security_alert: bool,
        block_reason: AppBlockReason,
    ) -> Self {
        Self {
            error_code: Some(error_code),
            error_detail,
            security_alert,
            block_reason,
            local_port: Some(local_port),
            ..Self::base(app_id, TunnelPhase::Error)
        }
    }

    /// A static, non-retrying failure such as a dangling tunnel reference.
    pub fn blocked(app_id: &str, local_port: u16, reason: AppBlockReason) -> Self {
        Self {
            error_code: reason.error_code().map(str::to_string),
            block_reason: reason,
            local_port: Some(local_port),
            ..Self::base(app_id, TunnelPhase::Error)
        }
    }

    /// 记录本轮选了哪条线路、哪些线路已经失败。
    pub fn with_endpoint_route(
        mut self,
        endpoint: Option<EndpointSelection>,
        failures: Vec<EndpointFailure>,
    ) -> Self {
        self.endpoint = endpoint;
        self.endpoint_failures = failures;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestState {
    Success,
    Failure,
}

/// Result of a user-triggered tunnel connectivity test. The probe authenticates
/// through the full jump chain but installs no local forward. Multiple endpoints
/// are tried in policy order; every failure is reported when none works.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelTestResult {
    pub tunnel_id: String,
    pub state: TestState,
    pub elapsed_ms: u64,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub security_alert: bool,
    /// 测试成功的线路名。
    pub endpoint_label: Option<String>,
    /// 逐条线路的失败原因。
    pub endpoint_failures: Vec<EndpointFailure>,
}

fn default_true() -> bool {
    true
}

fn default_keepalive() -> u32 {
    DEFAULT_KEEPALIVE_SECONDS
}

fn default_timeout() -> u32 {
    DEFAULT_CONNECT_TIMEOUT_SECONDS
}

fn default_group_id() -> String {
    DEFAULT_GROUP_ID.to_string()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CredentialInput {
    pub auth_type: AuthType,
    /// `None` keeps the stored secret, `Some("")` clears it, anything else is
    /// encrypted and replaces the stored value.
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub passphrase: Option<String>,
    /// `~/.ssh/<name>` 的文件名。设置后由后端读取该文件作为私钥，
    /// 前端不再接触密钥内容；与 `private_key` 同时出现时以本字段为准。
    pub private_key_path: Option<String>,
}

impl CredentialInput {
    /// 是否声明了“从本机 ~/.ssh 读取私钥”。
    pub fn uses_local_key(&self) -> bool {
        self.private_key_path
            .as_deref()
            .is_some_and(|path| !path.trim().is_empty())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JumpInput {
    pub id: Option<String>,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credentials: CredentialInput,
}

impl Default for JumpInput {
    fn default() -> Self {
        Self {
            id: None,
            label: String::new(),
            host: String::new(),
            port: DEFAULT_SSH_PORT,
            username: String::new(),
            credentials: CredentialInput::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EndpointInput {
    pub id: Option<String>,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credentials: CredentialInput,
    pub jumps: Vec<JumpInput>,
    pub wifi_only: bool,
    pub enabled: bool,
}

impl Default for EndpointInput {
    fn default() -> Self {
        Self {
            id: None,
            label: String::new(),
            host: String::new(),
            port: DEFAULT_SSH_PORT,
            username: String::new(),
            credentials: CredentialInput::default(),
            jumps: Vec::new(),
            wifi_only: false,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub endpoints: Vec<EndpointInput>,
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    #[serde(default = "default_keepalive")]
    pub keep_alive_seconds: u32,
    #[serde(default = "default_timeout")]
    pub connect_timeout_seconds: u32,
    #[serde(default = "default_true")]
    pub strict_host_key: bool,
    /// 首选线路；`None` 表示自动（按声明顺序）。
    #[serde(default)]
    pub preferred_endpoint_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(default = "default_group_id")]
    pub group_id: String,
    pub tunnel_id: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    #[serde(default)]
    pub browser_url: String,
    #[serde(default)]
    pub auto_start_on_boot: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInput {
    pub id: Option<String>,
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一条直连线路（无跳板）。
    fn endpoint(host: &str) -> TunnelEndpoint {
        TunnelEndpoint {
            host: host.into(),
            username: "ops".into(),
            credentials: SshCredentials {
                auth_type: AuthType::Password,
                password_cipher: Some("v1.iv.data".into()),
                ..SshCredentials::default()
            },
            ..TunnelEndpoint::default()
        }
    }

    fn tunnel() -> TunnelConfig {
        TunnelConfig {
            name: "NAS".into(),
            endpoints: vec![TunnelEndpoint {
                port: 2222,
                ..endpoint("example.com")
            }],
            ..TunnelConfig::default()
        }
    }

    fn app() -> ForwardApp {
        ForwardApp {
            name: "Web".into(),
            tunnel_id: "t1".into(),
            local_port: 18080,
            remote_host: "10.0.0.5".into(),
            remote_port: 5000,
            ..ForwardApp::default()
        }
    }

    #[test]
    fn browser_url_defaults_to_loopback() {
        assert_eq!(app().resolved_browser_url(), "http://127.0.0.1:18080/");
    }

    #[test]
    fn browser_url_expands_port_placeholder() {
        let cfg = ForwardApp {
            local_port: 19000,
            browser_url: "https://127.0.0.1:{port}/console".into(),
            ..app()
        };
        assert_eq!(
            cfg.resolved_browser_url(),
            "https://127.0.0.1:19000/console"
        );
    }

    #[test]
    fn chain_summary_lists_jumps_before_target() {
        let cfg = TunnelConfig {
            endpoints: vec![TunnelEndpoint {
                jumps: vec![
                    JumpHost {
                        host: "bastion-1".into(),
                        ..JumpHost::default()
                    },
                    JumpHost {
                        host: "bastion-2".into(),
                        ..JumpHost::default()
                    },
                ],
                ..TunnelEndpoint {
                    port: 2222,
                    ..endpoint("example.com")
                }
            }],
            ..tunnel()
        };
        assert_eq!(
            cfg.chain_summary(),
            "bastion-1  →  bastion-2  →  example.com"
        );
        assert_eq!(cfg.endpoint(), "ops@example.com:2222");
    }

    /// 首选线路决定卡片显示哪一条，但不会改变声明顺序。
    #[test]
    fn the_preferred_endpoint_drives_the_card_summary() {
        let home = TunnelEndpoint {
            id: "home".into(),
            label: "家庭局域网".into(),
            ..endpoint("192.168.1.10")
        };
        let away = TunnelEndpoint {
            id: "away".into(),
            label: "frp 外网".into(),
            ..endpoint("frp.example.com")
        };
        let pinned = TunnelConfig {
            endpoints: vec![home.clone(), away.clone()],
            preferred_endpoint_id: Some("away".into()),
            ..TunnelConfig::default()
        };
        assert_eq!(pinned.endpoint(), "ops@frp.example.com:22");
        assert_eq!(pinned.primary_endpoint().map(|item| item.id.as_str()), Some("away"));
        // 首选 id 失效时回落到第一条，不能让卡片变成空白。
        let stale = TunnelConfig {
            preferred_endpoint_id: Some("gone".into()),
            ..pinned
        };
        assert_eq!(stale.endpoint(), "ops@192.168.1.10:22");
    }

    #[test]
    fn endpoint_label_falls_back_to_host_then_placeholder() {
        let unnamed = TunnelEndpoint {
            label: String::new(),
            ..endpoint("bastion.example.com")
        };
        assert_eq!(unnamed.display_name(), "bastion.example.com");
        let empty = TunnelEndpoint {
            label: "   ".into(),
            host: String::new(),
            ..TunnelEndpoint::default()
        };
        assert_eq!(empty.display_name(), "?");
    }

    /// 停用 / 缺凭据 / 地址非法的线路都不算可拨号，界面据此提示“需补全”。
    #[test]
    fn dialable_endpoints_require_enabled_address_user_and_secret() {
        let ok = TunnelEndpoint {
            id: "ok".into(),
            ..endpoint("a.example.com")
        };
        let off = TunnelEndpoint {
            id: "off".into(),
            enabled: false,
            ..endpoint("b.example.com")
        };
        let secretless = TunnelEndpoint {
            id: "nosecret".into(),
            credentials: SshCredentials::default(),
            ..endpoint("c.example.com")
        };
        let bad_host = TunnelEndpoint {
            id: "bad".into(),
            host: "bad host".into(),
            ..endpoint("d.example.com")
        };
        let config = TunnelConfig {
            endpoints: vec![ok, off, secretless, bad_host],
            ..TunnelConfig::default()
        };
        let ids: Vec<&str> = config
            .dialable_endpoints()
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        assert_eq!(ids, vec!["ok"]);
        assert!(config.is_dialable());
        assert!(!TunnelConfig::default().is_dialable());
    }

    #[test]
    fn jump_display_name_falls_back_to_host() {
        let jump = JumpHost {
            host: "bastion".into(),
            ..JumpHost::default()
        };
        assert_eq!(jump.display_name(), "bastion");
    }

    #[test]
    fn normalization_keeps_default_group_first_and_repairs_orphans() {
        let doc = DockSnapshot {
            groups: vec![AppGroup {
                id: "g1".into(),
                name: "Prod".into(),
                created_at: 5,
            }],
            apps: vec![ForwardApp {
                group_id: "missing".into(),
                ..app()
            }],
            ..DockSnapshot::default()
        }
        .normalized();
        assert_eq!(doc.groups[0].id, DEFAULT_GROUP_ID);
        assert_eq!(doc.apps[0].group_id, DEFAULT_GROUP_ID);
    }

    #[test]
    fn blocked_status_exposes_a_stable_error_code() {
        let status = AppStatus::blocked("a1", 18080, AppBlockReason::TunnelDeleted);
        assert_eq!(status.phase, TunnelPhase::Error);
        assert_eq!(status.error_code.as_deref(), Some("tunnel_deleted"));
        assert_eq!(status.block_reason, AppBlockReason::TunnelDeleted);
    }

    #[test]
    fn status_carries_the_endpoint_that_is_in_use() {
        let source = endpoint("frp.example.com");
        let status = AppStatus::connected("a1", 18080).with_endpoint_route(
            Some(EndpointSelection::of(&source)),
            vec![EndpointFailure {
                endpoint_id: "home".into(),
                label: "家庭局域网".into(),
                code: "connection_refused".into(),
                detail: None,
            }],
        );
        assert_eq!(
            status.endpoint.as_ref().map(|item| item.label.as_str()),
            Some("frp.example.com")
        );
        assert_eq!(status.endpoint_failures.len(), 1);
        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(value["endpoint"]["endpoint"], "ops@frp.example.com:22");
        assert_eq!(value["endpointFailures"][0]["code"], "connection_refused");
    }

    fn password_credentials() -> SshCredentials {
        SshCredentials {
            auth_type: AuthType::Password,
            password_cipher: Some("v1.iv.data".into()),
            ..SshCredentials::default()
        }
    }

    /// Importing `~/.ssh/config` makes the jump host its own tunnel; once that
    /// tunnel has a password, the tunnel jumping through it must not ask again.
    #[test]
    fn jump_credentials_are_inherited_from_the_matching_tunnel() {
        let bastion = TunnelConfig {
            id: "bastion".into(),
            name: "dpi-frpc".into(),
            endpoints: vec![TunnelEndpoint {
                host: "10.0.0.1".into(),
                port: 22,
                username: "root".into(),
                credentials: password_credentials(),
                ..TunnelEndpoint::default()
            }],
            ..TunnelConfig::default()
        };
        let target = TunnelConfig {
            id: "vdi".into(),
            name: "dpi-vdi".into(),
            endpoints: vec![TunnelEndpoint {
                host: "10.0.0.2".into(),
                username: "root".into(),
                jumps: vec![JumpHost {
                    id: "j1".into(),
                    label: "dpi-frpc".into(),
                    host: "10.0.0.1".into(),
                    port: 22,
                    username: "root".into(),
                    credentials: SshCredentials::default(),
                }],
                ..TunnelEndpoint::default()
            }],
            ..TunnelConfig::default()
        };
        let snapshot = DockSnapshot {
            tunnels: vec![bastion, target.clone()],
            ..DockSnapshot::default()
        };

        let resolved = snapshot.with_inherited_jump_credentials(&target);
        assert!(resolved.endpoints[0].jumps[0].credentials.has_secret());
        assert_eq!(
            resolved.endpoints[0].jumps[0]
                .credentials
                .password_cipher
                .as_deref(),
            Some("v1.iv.data")
        );
        // 原始配置不改写：继承只发生在解析出的副本上。
        assert!(!target.endpoints[0].jumps[0].credentials.has_secret());
    }

    /// 每条线路的跳板机都要能各自继承，不能只处理第一条。
    #[test]
    fn every_endpoint_inherits_jump_credentials() {
        let bastion = TunnelConfig {
            id: "bastion".into(),
            name: "dpi-frpc".into(),
            endpoints: vec![TunnelEndpoint {
                host: "10.0.0.1".into(),
                port: 22,
                username: "root".into(),
                credentials: password_credentials(),
                ..TunnelEndpoint::default()
            }],
            ..TunnelConfig::default()
        };
        let jump = || JumpHost {
            id: "j1".into(),
            label: String::new(),
            host: "10.0.0.1".into(),
            port: 22,
            username: "root".into(),
            credentials: SshCredentials::default(),
        };
        let target = TunnelConfig {
            id: "vdi".into(),
            endpoints: vec![
                TunnelEndpoint {
                    id: "home".into(),
                    host: "10.0.0.2".into(),
                    username: "root".into(),
                    jumps: vec![jump()],
                    ..TunnelEndpoint::default()
                },
                TunnelEndpoint {
                    id: "away".into(),
                    host: "frp.example.com".into(),
                    username: "root".into(),
                    jumps: vec![jump()],
                    ..TunnelEndpoint::default()
                },
            ],
            ..TunnelConfig::default()
        };
        let snapshot = DockSnapshot {
            tunnels: vec![bastion, target.clone()],
            ..DockSnapshot::default()
        };
        let resolved = snapshot.with_inherited_jump_credentials(&target);
        for endpoint in &resolved.endpoints {
            assert!(endpoint.jumps[0].credentials.has_secret());
        }
        assert_eq!(
            snapshot.credential_source_name_for_jump(&target, &target.endpoints[0].jumps[0]),
            Some("dpi-frpc".to_string())
        );
    }

    #[test]
    fn jump_credentials_fall_back_to_endpoint_when_the_label_is_empty() {
        let bastion = TunnelConfig {
            id: "bastion".into(),
            name: "已改名".into(),
            endpoints: vec![TunnelEndpoint {
                host: "10.0.0.1".into(),
                port: 2222,
                username: "ops".into(),
                credentials: password_credentials(),
                ..TunnelEndpoint::default()
            }],
            ..TunnelConfig::default()
        };
        let target = TunnelConfig {
            id: "vdi".into(),
            endpoints: vec![TunnelEndpoint {
                jumps: vec![JumpHost {
                    id: "j1".into(),
                    label: String::new(),
                    host: "10.0.0.1".into(),
                    port: 2222,
                    username: "ops".into(),
                    credentials: SshCredentials::default(),
                }],
                ..TunnelEndpoint::default()
            }],
            ..TunnelConfig::default()
        };
        let snapshot = DockSnapshot {
            tunnels: vec![bastion, target.clone()],
            ..DockSnapshot::default()
        };
        let resolved = snapshot.with_inherited_jump_credentials(&target);
        assert!(resolved.endpoints[0].jumps[0].credentials.has_secret());
    }

    #[test]
    fn an_exact_endpoint_match_beats_a_name_match() {
        let same_name = TunnelConfig {
            id: "other".into(),
            name: "dpi-frpc".into(),
            endpoints: vec![TunnelEndpoint {
                host: "198.51.100.7".into(),
                port: 22,
                username: "root".into(),
                credentials: SshCredentials {
                    auth_type: AuthType::Password,
                    password_cipher: Some("v1.wrong.data".into()),
                    ..SshCredentials::default()
                },
                ..TunnelEndpoint::default()
            }],
            ..TunnelConfig::default()
        };
        let exact = TunnelConfig {
            id: "exact".into(),
            name: "另一个名字".into(),
            endpoints: vec![TunnelEndpoint {
                host: "10.0.0.1".into(),
                port: 22,
                username: "root".into(),
                credentials: password_credentials(),
                ..TunnelEndpoint::default()
            }],
            ..TunnelConfig::default()
        };
        let target = TunnelConfig {
            id: "vdi".into(),
            endpoints: vec![TunnelEndpoint {
                jumps: vec![JumpHost {
                    id: "j1".into(),
                    label: "dpi-frpc".into(),
                    host: "10.0.0.1".into(),
                    port: 22,
                    username: "root".into(),
                    credentials: SshCredentials::default(),
                }],
                ..TunnelEndpoint::default()
            }],
            ..TunnelConfig::default()
        };
        let snapshot = DockSnapshot {
            tunnels: vec![same_name, exact, target.clone()],
            ..DockSnapshot::default()
        };
        let resolved = snapshot.with_inherited_jump_credentials(&target);
        assert_eq!(
            resolved.endpoints[0].jumps[0]
                .credentials
                .password_cipher
                .as_deref(),
            Some("v1.iv.data")
        );
    }

    #[test]
    fn a_jump_with_its_own_secret_is_not_overridden() {
        let bastion = TunnelConfig {
            id: "bastion".into(),
            name: "dpi-frpc".into(),
            endpoints: vec![TunnelEndpoint {
                host: "10.0.0.1".into(),
                port: 22,
                username: "root".into(),
                credentials: password_credentials(),
                ..TunnelEndpoint::default()
            }],
            ..TunnelConfig::default()
        };
        let target = TunnelConfig {
            id: "vdi".into(),
            endpoints: vec![TunnelEndpoint {
                jumps: vec![JumpHost {
                    id: "j1".into(),
                    label: "dpi-frpc".into(),
                    host: "10.0.0.1".into(),
                    port: 22,
                    username: "root".into(),
                    credentials: SshCredentials {
                        auth_type: AuthType::Password,
                        password_cipher: Some("v1.own.data".into()),
                        ..SshCredentials::default()
                    },
                }],
                ..TunnelEndpoint::default()
            }],
            ..TunnelConfig::default()
        };
        let snapshot = DockSnapshot {
            tunnels: vec![bastion, target.clone()],
            ..DockSnapshot::default()
        };
        let resolved = snapshot.with_inherited_jump_credentials(&target);
        assert_eq!(
            resolved.endpoints[0].jumps[0]
                .credentials
                .password_cipher
                .as_deref(),
            Some("v1.own.data")
        );
    }
}
