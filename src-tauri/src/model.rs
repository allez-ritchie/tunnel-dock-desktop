use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_SSH_PORT: u16 = 22;
pub const DEFAULT_KEEPALIVE_SECONDS: u32 = 30;
pub const DEFAULT_CONNECT_TIMEOUT_SECONDS: u32 = 15;
pub const LOOPBACK: &str = "127.0.0.1";
pub const PORT_PLACEHOLDER: &str = "{port}";
pub const DEFAULT_GROUP_ID: &str = "default";
pub const DEFAULT_GROUP_NAME: &str = "default";
pub const SNAPSHOT_VERSION: u32 = 2;

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

/// A reusable SSH access definition: username + host + port, plus the optional
/// jump chain. A tunnel owns no local port and no remote target; applications
/// reference it and add their own `localPort -> remoteHost:remotePort` rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TunnelConfig {
    pub id: String,
    pub name: String,
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_user: String,
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
            ssh_host: String::new(),
            ssh_port: DEFAULT_SSH_PORT,
            ssh_user: String::new(),
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
    pub fn endpoint(&self) -> String {
        format!("{}@{}:{}", self.ssh_user, self.ssh_host, self.ssh_port)
    }

    pub fn chain_summary(&self) -> String {
        let mut hops: Vec<&str> = self.jumps.iter().map(|j| j.host.as_str()).collect();
        hops.push(self.ssh_host.as_str());
        hops.join("  →  ")
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
        }
    }
}

/// Frontend-safe tunnel representation: secrets are reduced to presence flags.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelView {
    pub id: String,
    pub name: String,
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_user: String,
    pub credentials: CredentialState,
    pub jumps: Vec<JumpView>,
    pub auto_reconnect: bool,
    pub keep_alive_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub strict_host_key: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub endpoint: String,
    pub chain_summary: String,
}

impl From<&TunnelConfig> for TunnelView {
    fn from(value: &TunnelConfig) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            ssh_host: value.ssh_host.clone(),
            ssh_port: value.ssh_port,
            ssh_user: value.ssh_user.clone(),
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
        }
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestState {
    Success,
    Failure,
}

/// Result of a user-triggered tunnel connectivity test. The probe authenticates
/// through the full jump chain but installs no local forward.
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

fn default_ssh_port() -> u16 {
    DEFAULT_SSH_PORT
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
#[serde(rename_all = "camelCase")]
pub struct TunnelInput {
    pub id: Option<String>,
    pub name: String,
    pub ssh_host: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    pub ssh_user: String,
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

    fn tunnel() -> TunnelConfig {
        TunnelConfig {
            name: "NAS".into(),
            ssh_host: "example.com".into(),
            ssh_port: 2222,
            ssh_user: "ops".into(),
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
            ..tunnel()
        };
        assert_eq!(
            cfg.chain_summary(),
            "bastion-1  →  bastion-2  →  example.com"
        );
        assert_eq!(cfg.endpoint(), "ops@example.com:2222");
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
}
