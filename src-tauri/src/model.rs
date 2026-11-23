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
///  - 3：一条隧道持有**一组有序的 SSH 线路**；
///  - 4：线路概念取消。一条隧道就是一次**确定的** SSH 接入（用户名 / 地址 / 端口 /
///       跳板 / 凭据），多条接入由**应用**持有：应用保存一个有序的隧道列表，启动时
///       按顺序尝试，本地端口对第三方透明。
pub const SNAPSHOT_VERSION: u32 = 4;

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

/// 一次确定的 SSH 接入：用户名 / 地址 / 端口 / 跳板链 / 凭据。
///
/// 一个隧道**只描述一条接入路径**，不再持有可选线路；同一服务需要多个入口时，
/// 由应用持有多个隧道并按顺序尝试，本地端口不变。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TunnelConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credentials: SshCredentials,
    pub jumps: Vec<JumpHost>,
    pub auto_reconnect: bool,
    pub keep_alive_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub strict_host_key: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        let now = now_millis();
        Self {
            id: new_id(),
            name: String::new(),
            host: String::new(),
            port: DEFAULT_SSH_PORT,
            username: String::new(),
            credentials: SshCredentials::default(),
            jumps: Vec::new(),
            auto_reconnect: true,
            keep_alive_seconds: DEFAULT_KEEPALIVE_SECONDS,
            connect_timeout_seconds: DEFAULT_CONNECT_TIMEOUT_SECONDS,
            strict_host_key: true,
            created_at: now,
            updated_at: now,
        }
    }
}

impl TunnelConfig {
    /// `user@host:port`，界面上展示隧道身份的稳定标识。
    pub fn endpoint(&self) -> String {
        format!("{}@{}:{}", self.username, self.host, self.port)
    }

    /// 完整链路：从第一级跳板机到最终的 SSH 服务器。
    pub fn chain_summary(&self) -> String {
        let mut hops: Vec<&str> = self.jumps.iter().map(|jump| jump.host.as_str()).collect();
        hops.push(self.host.as_str());
        hops.join("  →  ")
    }

    /// russh 是否具备发起连接的全部条件。
    pub fn is_dialable(&self) -> bool {
        !self.host.trim().is_empty()
            && !self.host.contains(char::is_whitespace)
            && !self.username.trim().is_empty()
            && self.port > 0
            && self.credentials.has_secret()
    }

    pub fn with_updated_at(mut self) -> Self {
        self.updated_at = now_millis();
        self
    }
}

/// One forwarded `localPort -> remoteHost:remotePort` rule. The SSH access lives
/// in [`TunnelConfig`]; an application picks one or more tunnels in the order it
/// wants them dialled. `desired_on` is the persisted user intent, never live
/// state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ForwardApp {
    pub id: String,
    pub name: String,
    pub group_id: String,
    /// 按尝试顺序排列的隧道 ID；第一条是首选。至少一条。
    pub tunnel_ids: Vec<String>,
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
            tunnel_ids: Vec::new(),
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
    /// 首选隧道（列表第一条）。
    pub fn primary_tunnel_id(&self) -> Option<&str> {
        self.tunnel_ids.first().map(String::as_str)
    }

    pub fn uses_tunnel(&self, tunnel_id: &str) -> bool {
        self.tunnel_ids.iter().any(|id| id == tunnel_id)
    }

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
            .filter(|app| app.uses_tunnel(tunnel_id))
            .cloned()
            .collect()
    }

    /// 应用按尝试顺序可以真正拿去拨号的隧道。
    pub fn dialable_tunnels_for<'a>(&'a self, app: &ForwardApp) -> Vec<&'a TunnelConfig> {
        app.tunnel_ids
            .iter()
            .filter_map(|id| self.tunnel(id))
            .filter(|tunnel| tunnel.is_dialable())
            .collect()
    }

    /// Finds the tunnel that defines the SSH access of a jump host.
    ///
    /// Importing `~/.ssh/config` creates one tunnel per `Host` alias and turns
    /// `ProxyJump` aliases into jump hosts. Those jump hosts carry no secret
    /// (the file has none), yet the same host exists as its own tunnel where the
    /// user *did* enter the password. Matching the jump back to that tunnel is
    /// what lets one password serve every tunnel jumping through it.
    ///
    /// The exact `user@host:port` match wins because it identifies the same SSH
    /// account unambiguously; the name is the fallback for jump hosts whose
    /// address was edited after the import.
    pub fn credential_source_for_jump<'a>(
        &'a self,
        owner: &TunnelConfig,
        jump: &JumpHost,
    ) -> Option<&'a TunnelConfig> {
        let owner_id = owner.id.as_str();
        let host = jump.host.trim();
        let username = jump.username.trim();
        let exact_match = |tunnel: &&TunnelConfig| {
            !host.is_empty()
                && tunnel.host.trim().eq_ignore_ascii_case(host)
                && tunnel.port == jump.port
                && tunnel.username.trim() == username
                && tunnel.credentials.has_secret()
        };
        let label = jump.label.trim();
        let name_matches = |tunnel: &&TunnelConfig| {
            !label.is_empty() && tunnel.name.trim().eq_ignore_ascii_case(label)
        };
        // 候选都来自“别的隧道”，否则会自己引用自己。
        let external = || {
            self.tunnels
                .iter()
                .filter(|tunnel| tunnel.id != owner_id)
                .filter(|tunnel| tunnel.credentials.has_secret())
        };
        external().clone().find(exact_match).or_else(|| {
            // 名称匹配也要求同一台主机：只有名字相同、地址不同的隧道不能借用凭据。
            external().find(|tunnel| {
                name_matches(tunnel) && tunnel.host.trim().eq_ignore_ascii_case(host)
            })
        })
    }

    /// 跳板机凭据来自哪条隧道（用于界面提示“凭据复用自…”）。
    pub fn credential_source_name_for_jump(
        &self,
        owner: &TunnelConfig,
        jump: &JumpHost,
    ) -> Option<String> {
        self.credential_source_for_jump(owner, jump)
            .map(|tunnel| tunnel.name.clone())
    }

    /// 用户在编辑器里“从现有隧道选择”了一台跳板机时，把该隧道的凭据密文
    /// 复制到这台跳板机上，让它不再依赖源隧道。
    ///
    /// 与 [`Self::with_inherited_jump_credentials`] 的区别：那是**运行时解析**
    /// （只读、不落盘），这里是**保存时固化**。用户主动选了“这台主机就是用这个
    /// 账号密码”，之后删掉源隧道不该把已经建好的跳板链弄断。
    pub fn apply_jump_credential_source(
        &self,
        jump: &mut JumpHost,
        source: &JumpCredentialSource,
    ) -> Result<(), String> {
        let tunnel = self
            .tunnel(&source.tunnel_id)
            .ok_or_else(|| "所选的隧道已被删除，请重新选择".to_string())?;
        let credentials = match source.hop {
            None => tunnel.credentials.clone(),
            Some(index) => tunnel
                .jumps
                .get(index)
                .map(|hop| hop.credentials.clone())
                .ok_or_else(|| format!("隧道“{}”的跳板链已变化，请重新选择", tunnel.name))?,
        };
        if !credentials.has_secret() {
            return Err(format!(
                "隧道“{}”还没有保存凭据，无法复制给跳板机",
                tunnel.name
            ));
        }
        // 凭据自带认证方式：选了私钥认证的隧道，跳板机也必须按私钥拨号。
        jump.credentials = credentials;
        Ok(())
    }

    /// Returns `owner` with every secret-less jump host taking its credentials
    /// from another tunnel that defines that host. Used by both the connect path
    /// and the UI state, so "已配置密码" always means the same thing.
    pub fn with_inherited_jump_credentials(&self, owner: &TunnelConfig) -> TunnelConfig {
        let mut resolved = owner.clone();
        let mut inherited: Vec<(usize, SshCredentials)> = Vec::new();
        for (index, jump) in resolved.jumps.iter().enumerate() {
            if jump.credentials.has_secret() {
                continue;
            }
            if let Some(source) = self.credential_source_for_jump(owner, jump) {
                inherited.push((index, source.credentials.clone()));
            }
        }
        for (index, credentials) in inherited {
            resolved.jumps[index].credentials = credentials;
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
            // 隧道列表本身保持顺序，只去掉重复项，避免同一个入口被拨两次。
            let mut seen = std::collections::HashSet::new();
            app.tunnel_ids.retain(|id| seen.insert(id.clone()));
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

/// Frontend-safe tunnel representation: secrets are reduced to presence flags.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelView {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credentials: CredentialState,
    pub jumps: Vec<JumpView>,
    pub auto_reconnect: bool,
    pub keep_alive_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub strict_host_key: bool,
    pub created_at: u64,
    pub updated_at: u64,
    /// `user@host:port`，列表里直接展示。
    pub endpoint: String,
    pub chain_summary: String,
    /// 定义完整、当前可以被拨号（有地址/用户/端口/凭据）。
    pub dialable: bool,
}

impl From<&TunnelConfig> for TunnelView {
    fn from(value: &TunnelConfig) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            host: value.host.clone(),
            port: value.port,
            username: value.username.clone(),
            credentials: CredentialState::from(&value.credentials),
            jumps: value.jumps.iter().map(JumpView::from).collect(),
            auto_reconnect: value.auto_reconnect,
            keep_alive_seconds: value.keep_alive_seconds,
            connect_timeout_seconds: value.connect_timeout_seconds,
            strict_host_key: value.strict_host_key,
            created_at: value.created_at,
            updated_at: value.updated_at,
            endpoint: value.endpoint(),
            chain_summary: value.chain_summary(),
            dialable: value.is_dialable(),
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
        for (jump_view, original) in view.jumps.iter_mut().zip(value.jumps.iter()) {
            if original.credentials.has_secret() {
                continue;
            }
            let effective = &jump_view.credentials;
            if !(effective.has_password || effective.has_private_key) {
                continue;
            }
            jump_view.inherited_from = snapshot.credential_source_name_for_jump(value, original);
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
    /// 按尝试顺序排列的隧道 ID，第一条是首选。
    pub tunnel_ids: Vec<String>,
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
            tunnel_ids: value.tunnel_ids.clone(),
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

/// 某一轮连接里，一个隧道为什么没能建立连接。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelFailure {
    pub tunnel_id: String,
    pub name: String,
    /// 语言无关的错误码，文案由界面提供。
    pub code: String,
    pub detail: Option<String>,
}

/// 当前正在承载流量的隧道，让卡片、错误详情页都能说清“走的是哪条路”。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelSelection {
    pub tunnel_id: String,
    pub name: String,
    pub endpoint: String,
}

impl TunnelSelection {
    pub fn of(tunnel: &TunnelConfig) -> Self {
        Self {
            tunnel_id: tunnel.id.clone(),
            name: tunnel.name.clone(),
            endpoint: tunnel.endpoint(),
        }
    }
}

/// Live state of one forwarded application. Each application owns its own SSH
/// connection and local listener; shared tunnels are only configuration.
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
    /// 当前（或最后一次尝试）承载转发的隧道。
    pub tunnel: Option<TunnelSelection>,
    /// 本轮已经失败的隧道，按尝试顺序。
    pub tunnel_failures: Vec<TunnelFailure>,
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
            tunnel: None,
            tunnel_failures: Vec::new(),
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

    /// 记录本轮选了哪条隧道、哪些隧道已经失败。
    pub fn with_tunnel_route(
        mut self,
        tunnel: Option<TunnelSelection>,
        failures: Vec<TunnelFailure>,
    ) -> Self {
        self.tunnel = tunnel;
        self.tunnel_failures = failures;
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
/// through the full jump chain but installs no local forward: a tunnel now
/// describes exactly one SSH access, so there is nothing to fall back to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelTestResult {
    pub tunnel_id: String,
    pub state: TestState,
    pub elapsed_ms: u64,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub security_alert: bool,
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JumpCredentialSource {
    /// 提供凭据的隧道。
    pub tunnel_id: String,
    /// 该隧道自身跳板链里的第几级；`None` 表示隧道本身这台主机。
    pub hop: Option<usize>,
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
    /// 这台跳板机是从哪条已有隧道“选”来的。保存时把该隧道的凭据密文复制过来，
    /// 让跳板机保持自包含：源隧道之后被删除也不影响已有的跳板链。
    pub credential_from: Option<JumpCredentialSource>,
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
            credential_from: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub credentials: CredentialInput,
    #[serde(default)]
    pub jumps: Vec<JumpInput>,
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    #[serde(default = "default_keepalive")]
    pub keep_alive_seconds: u32,
    #[serde(default = "default_timeout")]
    pub connect_timeout_seconds: u32,
    #[serde(default = "default_true")]
    pub strict_host_key: bool,
}

fn default_ssh_port() -> u16 {
    DEFAULT_SSH_PORT
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(default = "default_group_id")]
    pub group_id: String,
    /// 按尝试顺序排列的隧道；第一条是首选。
    #[serde(default)]
    pub tunnel_ids: Vec<String>,
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

    fn credentials() -> SshCredentials {
        SshCredentials {
            auth_type: AuthType::Password,
            password_cipher: Some("v1.iv.data".into()),
            ..SshCredentials::default()
        }
    }

    fn tunnel(host: &str) -> TunnelConfig {
        TunnelConfig {
            name: "NAS".into(),
            host: host.into(),
            username: "ops".into(),
            port: 2222,
            credentials: credentials(),
            ..TunnelConfig::default()
        }
    }

    fn app() -> ForwardApp {
        ForwardApp {
            name: "Web".into(),
            tunnel_ids: vec!["t1".into()],
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
            ..tunnel("example.com")
        };
        assert_eq!(
            cfg.chain_summary(),
            "bastion-1  →  bastion-2  →  example.com"
        );
        assert_eq!(cfg.endpoint(), "ops@example.com:2222");
    }

    /// 隧道自己拿不出“首选入口”了：接入是确定的，入口顺序属于应用。
    #[test]
    fn a_tunnel_describes_exactly_one_access_path() {
        let cfg = tunnel("example.com");
        assert_eq!(cfg.endpoint(), "ops@example.com:2222");
        assert_eq!(cfg.chain_summary(), "example.com");
        assert!(cfg.is_dialable());
    }

    /// 地址 / 用户 / 端口 / 凭据任一缺失都不算可拨号。
    #[test]
    fn a_tunnel_is_dialable_only_when_the_access_is_complete() {
        assert!(!TunnelConfig::default().is_dialable());
        assert!(!TunnelConfig {
            credentials: SshCredentials::default(),
            ..tunnel("a.example.com")
        }
        .is_dialable());
        assert!(!TunnelConfig {
            host: "bad host".into(),
            ..tunnel("a.example.com")
        }
        .is_dialable());
        assert!(!TunnelConfig {
            port: 0,
            ..tunnel("a.example.com")
        }
        .is_dialable());
        assert!(!TunnelConfig {
            username: "  ".into(),
            ..tunnel("a.example.com")
        }
        .is_dialable());
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

    /// 隧道列表的顺序就是尝试顺序，归一化只去重、不排序。
    #[test]
    fn normalization_dedupes_tunnel_lists_without_reordering() {
        let doc = DockSnapshot {
            apps: vec![ForwardApp {
                tunnel_ids: vec!["away".into(), "home".into(), "away".into()],
                ..app()
            }],
            ..DockSnapshot::default()
        }
        .normalized();
        assert_eq!(doc.apps[0].tunnel_ids, vec!["away", "home"]);
        assert_eq!(doc.apps[0].primary_tunnel_id(), Some("away"));
    }

    #[test]
    fn blocked_status_exposes_a_stable_error_code() {
        let status = AppStatus::blocked("a1", 18080, AppBlockReason::TunnelDeleted);
        assert_eq!(status.phase, TunnelPhase::Error);
        assert_eq!(status.error_code.as_deref(), Some("tunnel_deleted"));
        assert_eq!(status.block_reason, AppBlockReason::TunnelDeleted);
    }

    #[test]
    fn status_carries_the_tunnel_that_is_in_use() {
        let source = tunnel("frp.example.com");
        let status = AppStatus::connected("a1", 18080).with_tunnel_route(
            Some(TunnelSelection::of(&source)),
            vec![TunnelFailure {
                tunnel_id: "home".into(),
                name: "家庭局域网".into(),
                code: "connection_refused".into(),
                detail: None,
            }],
        );
        assert_eq!(
            status.tunnel.as_ref().map(|item| item.name.as_str()),
            Some("NAS")
        );
        assert_eq!(status.tunnel_failures.len(), 1);
        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(value["tunnel"]["endpoint"], "ops@frp.example.com:2222");
        assert_eq!(value["tunnelFailures"][0]["code"], "connection_refused");
    }

    /// Importing `~/.ssh/config` makes the jump host its own tunnel; once that
    /// tunnel has a password, the tunnel jumping through it must not ask again.
    #[test]
    fn jump_credentials_are_inherited_from_the_matching_tunnel() {
        let bastion = TunnelConfig {
            id: "bastion".into(),
            name: "dpi-frpc".into(),
            host: "10.0.0.1".into(),
            port: 22,
            username: "root".into(),
            credentials: credentials(),
            ..TunnelConfig::default()
        };
        let target = TunnelConfig {
            id: "vdi".into(),
            name: "dpi-vdi".into(),
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
            ..TunnelConfig::default()
        };
        let snapshot = DockSnapshot {
            tunnels: vec![bastion, target.clone()],
            ..DockSnapshot::default()
        };

        let resolved = snapshot.with_inherited_jump_credentials(&target);
        assert!(resolved.jumps[0].credentials.has_secret());
        assert_eq!(
            resolved.jumps[0].credentials.password_cipher.as_deref(),
            Some("v1.iv.data")
        );
        // 原始配置不改写：继承只发生在解析出的副本上。
        assert!(!target.jumps[0].credentials.has_secret());
    }

    #[test]
    fn jump_credentials_fall_back_to_the_tunnel_name() {
        let bastion = TunnelConfig {
            id: "bastion".into(),
            name: "已改名".into(),
            host: "10.0.0.1".into(),
            port: 2222,
            username: "ops".into(),
            credentials: credentials(),
            ..TunnelConfig::default()
        };
        let target = TunnelConfig {
            id: "vdi".into(),
            jumps: vec![JumpHost {
                id: "j1".into(),
                label: "已改名".into(),
                host: "10.0.0.1".into(),
                port: 2222,
                username: "ops".into(),
                credentials: SshCredentials::default(),
            }],
            ..TunnelConfig::default()
        };
        let snapshot = DockSnapshot {
            tunnels: vec![bastion, target.clone()],
            ..DockSnapshot::default()
        };
        assert_eq!(
            snapshot.credential_source_name_for_jump(&target, &target.jumps[0]),
            Some("已改名".to_string())
        );
        let resolved = snapshot.with_inherited_jump_credentials(&target);
        assert!(resolved.jumps[0].credentials.has_secret());
    }

    /// 同名但不同主机的隧道不能借用凭据：名字会变，地址才是身份。
    #[test]
    fn a_name_match_with_a_different_host_is_not_a_credential_source() {
        let other = TunnelConfig {
            id: "other".into(),
            name: "堡垒机".into(),
            host: "198.51.100.7".into(),
            port: 22,
            username: "root".into(),
            credentials: credentials(),
            ..TunnelConfig::default()
        };
        let target = TunnelConfig {
            id: "vdi".into(),
            jumps: vec![JumpHost {
                id: "j1".into(),
                label: "堡垒机".into(),
                host: "10.0.0.1".into(),
                port: 22,
                username: "root".into(),
                credentials: SshCredentials::default(),
            }],
            ..TunnelConfig::default()
        };
        let snapshot = DockSnapshot {
            tunnels: vec![other, target.clone()],
            ..DockSnapshot::default()
        };
        assert!(snapshot.credential_source_for_jump(&target, &target.jumps[0]).is_none());
    }

    #[test]
    fn an_exact_match_beats_a_name_match() {
        let same_name = TunnelConfig {
            id: "other".into(),
            name: "dpi-frpc".into(),
            host: "198.51.100.7".into(),
            port: 22,
            username: "root".into(),
            credentials: SshCredentials {
                auth_type: AuthType::Password,
                password_cipher: Some("v1.wrong.data".into()),
                ..SshCredentials::default()
            },
            ..TunnelConfig::default()
        };
        let exact = TunnelConfig {
            id: "exact".into(),
            name: "另一个名字".into(),
            host: "10.0.0.1".into(),
            port: 22,
            username: "root".into(),
            credentials: credentials(),
            ..TunnelConfig::default()
        };
        let target = TunnelConfig {
            id: "vdi".into(),
            jumps: vec![JumpHost {
                id: "j1".into(),
                label: "dpi-frpc".into(),
                host: "10.0.0.1".into(),
                port: 22,
                username: "root".into(),
                credentials: SshCredentials::default(),
            }],
            ..TunnelConfig::default()
        };
        let snapshot = DockSnapshot {
            tunnels: vec![same_name, exact, target.clone()],
            ..DockSnapshot::default()
        };
        let resolved = snapshot.with_inherited_jump_credentials(&target);
        assert_eq!(
            resolved.jumps[0].credentials.password_cipher.as_deref(),
            Some("v1.iv.data")
        );
    }

    #[test]
    fn a_jump_with_its_own_secret_is_not_overridden() {
        let bastion = TunnelConfig {
            id: "bastion".into(),
            name: "dpi-frpc".into(),
            host: "10.0.0.1".into(),
            port: 22,
            username: "root".into(),
            credentials: credentials(),
            ..TunnelConfig::default()
        };
        let target = TunnelConfig {
            id: "vdi".into(),
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
            ..TunnelConfig::default()
        };
        let snapshot = DockSnapshot {
            tunnels: vec![bastion, target.clone()],
            ..DockSnapshot::default()
        };
        let resolved = snapshot.with_inherited_jump_credentials(&target);
        assert_eq!(
            resolved.jumps[0].credentials.password_cipher.as_deref(),
            Some("v1.own.data")
        );
    }
}
