use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_SSH_PORT: u16 = 22;
pub const DEFAULT_KEEPALIVE_SECONDS: u32 = 30;
pub const DEFAULT_CONNECT_TIMEOUT_SECONDS: u32 = 15;
pub const LOOPBACK: &str = "127.0.0.1";
pub const PORT_PLACEHOLDER: &str = "{port}";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TunnelConfig {
    pub id: String,
    pub name: String,
    pub local_host: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_user: String,
    pub credentials: SshCredentials,
    pub jumps: Vec<JumpHost>,
    pub auto_reconnect: bool,
    pub auto_start_on_boot: bool,
    pub keep_alive_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub strict_host_key: bool,
    pub browser_url: String,
    pub desired_on: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        let now = now_millis();
        Self {
            id: new_id(),
            name: String::new(),
            local_host: LOOPBACK.to_string(),
            local_port: 18080,
            remote_host: String::new(),
            remote_port: 80,
            ssh_host: String::new(),
            ssh_port: DEFAULT_SSH_PORT,
            ssh_user: String::new(),
            credentials: SshCredentials::default(),
            jumps: Vec::new(),
            auto_reconnect: true,
            auto_start_on_boot: false,
            keep_alive_seconds: DEFAULT_KEEPALIVE_SECONDS,
            connect_timeout_seconds: DEFAULT_CONNECT_TIMEOUT_SECONDS,
            strict_host_key: true,
            browser_url: String::new(),
            desired_on: false,
            created_at: now,
            updated_at: now,
        }
    }
}

impl TunnelConfig {
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

    pub fn chain_summary(&self) -> String {
        let mut hops: Vec<&str> = self.jumps.iter().map(|j| j.host.as_str()).collect();
        hops.push(self.ssh_host.as_str());
        hops.join("  →  ")
    }

    pub fn with_desired(mut self, on: bool) -> Self {
        self.desired_on = on;
        self.updated_at = now_millis();
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

/// Frontend-safe representation: secrets are reduced to presence flags.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelView {
    pub id: String,
    pub name: String,
    pub local_host: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_user: String,
    pub credentials: CredentialState,
    pub jumps: Vec<JumpView>,
    pub auto_reconnect: bool,
    pub auto_start_on_boot: bool,
    pub keep_alive_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub strict_host_key: bool,
    pub browser_url: String,
    pub desired_on: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub local_url: String,
    pub browser_open_url: String,
    pub chain_summary: String,
}

impl From<&TunnelConfig> for TunnelView {
    fn from(value: &TunnelConfig) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            local_host: value.local_host.clone(),
            local_port: value.local_port,
            remote_host: value.remote_host.clone(),
            remote_port: value.remote_port,
            ssh_host: value.ssh_host.clone(),
            ssh_port: value.ssh_port,
            ssh_user: value.ssh_user.clone(),
            credentials: CredentialState::from(&value.credentials),
            jumps: value.jumps.iter().map(JumpView::from).collect(),
            auto_reconnect: value.auto_reconnect,
            auto_start_on_boot: value.auto_start_on_boot,
            keep_alive_seconds: value.keep_alive_seconds,
            connect_timeout_seconds: value.connect_timeout_seconds,
            strict_host_key: value.strict_host_key,
            browser_url: value.browser_url.clone(),
            desired_on: value.desired_on,
            created_at: value.created_at,
            updated_at: value.updated_at,
            local_url: value.local_url(),
            browser_open_url: value.resolved_browser_url(),
            chain_summary: value.chain_summary(),
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
    Stopping,
}

impl TunnelPhase {
    pub fn is_live(self) -> bool {
        matches!(
            self,
            TunnelPhase::Connecting
                | TunnelPhase::Connected
                | TunnelPhase::Reconnecting
                | TunnelPhase::Stopping
        )
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    pub tunnel_id: String,
    pub phase: TunnelPhase,
    pub attempt: u32,
    pub next_retry_seconds: Option<u32>,
    pub connected_at: Option<u64>,
    pub local_port: Option<u16>,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub security_alert: bool,
    pub updated_at: u64,
}

impl TunnelStatus {
    fn base(id: &str, phase: TunnelPhase) -> Self {
        Self {
            tunnel_id: id.to_string(),
            phase,
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

    pub fn idle(id: &str) -> Self {
        Self::base(id, TunnelPhase::Idle)
    }

    pub fn connecting(id: &str) -> Self {
        Self::base(id, TunnelPhase::Connecting)
    }

    pub fn connected(id: &str, local_port: u16) -> Self {
        Self {
            phase: TunnelPhase::Connected,
            connected_at: Some(now_millis()),
            local_port: Some(local_port),
            ..Self::base(id, TunnelPhase::Connected)
        }
    }

    pub fn reconnecting(
        id: &str,
        attempt: u32,
        wait_seconds: u32,
        error_code: Option<String>,
        error_detail: Option<String>,
    ) -> Self {
        Self {
            attempt,
            next_retry_seconds: Some(wait_seconds),
            error_code,
            error_detail,
            ..Self::base(id, TunnelPhase::Reconnecting)
        }
    }

    pub fn error(
        id: &str,
        error_code: String,
        error_detail: Option<String>,
        security_alert: bool,
    ) -> Self {
        Self {
            error_code: Some(error_code),
            error_detail,
            security_alert,
            ..Self::base(id, TunnelPhase::Error)
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_local_host() -> String {
    LOOPBACK.to_string()
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

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CredentialInput {
    pub auth_type: AuthType,
    /// `None` keeps the stored secret, `Some("")` clears it, anything else
    /// is encrypted and replaces the stored value.
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub passphrase: Option<String>,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(default = "default_local_host")]
    pub local_host: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub ssh_host: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    pub ssh_user: String,
    pub credentials: CredentialInput,
    #[serde(default)]
    pub jumps: Vec<JumpInput>,
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    #[serde(default)]
    pub auto_start_on_boot: bool,
    #[serde(default = "default_keepalive")]
    pub keep_alive_seconds: u32,
    #[serde(default = "default_timeout")]
    pub connect_timeout_seconds: u32,
    #[serde(default = "default_true")]
    pub strict_host_key: bool,
    #[serde(default)]
    pub browser_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TunnelConfig {
        TunnelConfig {
            name: "NAS".into(),
            local_port: 18080,
            remote_host: "10.0.0.5".into(),
            remote_port: 5000,
            ssh_host: "example.com".into(),
            ssh_port: 2222,
            ssh_user: "ops".into(),
            ..TunnelConfig::default()
        }
    }

    #[test]
    fn browser_url_defaults_to_loopback() {
        assert_eq!(sample().resolved_browser_url(), "http://127.0.0.1:18080/");
    }

    #[test]
    fn browser_url_expands_port_placeholder() {
        let cfg = TunnelConfig {
            local_port: 19000,
            browser_url: "https://127.0.0.1:{port}/console".into(),
            ..sample()
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
            ..sample()
        };
        assert_eq!(
            cfg.chain_summary(),
            "bastion-1  →  bastion-2  →  example.com"
        );
    }

    #[test]
    fn jump_display_name_falls_back_to_host() {
        let jump = JumpHost {
            host: "bastion".into(),
            ..JumpHost::default()
        };
        assert_eq!(jump.display_name(), "bastion");
    }
}
