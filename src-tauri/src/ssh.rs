use crate::known_hosts::{HostKeyDecision, KnownHosts};
use crate::model::{AuthType, ForwardApp, SshCredentials, TunnelConfig, LOOPBACK};
use crate::secrets::Secrets;
use russh::client::{self, AuthResult, Config, Handle, Handler};
use russh::keys::{decode_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::Disconnect;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, thiserror::Error)]
pub enum TunnelError {
    #[error("host key changed")]
    HostKeyChanged,
    #[error("authentication failed")]
    AuthFailed,
    #[error("missing credential")]
    MissingCredential,
    #[error("secret decryption failed")]
    SecretDecrypt,
    #[error("local port in use")]
    PortInUse,
    #[error("unknown host")]
    UnknownHost,
    #[error("connection timed out")]
    Timeout,
    #[error("connection refused")]
    ConnectionRefused,
    #[error("local forward lost")]
    LocalForwardLost,
    #[error("connection closed")]
    ConnectionClosed,
    #[error("{0}")]
    Connect(String),
    #[error("{0}")]
    Io(String),
}

impl TunnelError {
    pub fn code(&self) -> &'static str {
        match self {
            TunnelError::HostKeyChanged => "host_key_changed",
            TunnelError::AuthFailed => "auth_failed",
            TunnelError::MissingCredential | TunnelError::SecretDecrypt => "secret_decrypt",
            TunnelError::PortInUse => "port_in_use",
            TunnelError::UnknownHost => "unknown_host",
            TunnelError::Timeout => "timeout",
            TunnelError::ConnectionRefused => "connection_refused",
            TunnelError::LocalForwardLost => "local_forward_lost",
            TunnelError::ConnectionClosed => "connection_closed",
            TunnelError::Connect(_) | TunnelError::Io(_) => "generic",
        }
    }

    pub fn security_alert(&self) -> bool {
        matches!(self, TunnelError::HostKeyChanged)
    }

    pub fn detail(&self) -> Option<String> {
        match self {
            TunnelError::Connect(message) | TunnelError::Io(message) => {
                Some(message.chars().take(220).collect())
            }
            _ => None,
        }
    }
}

fn map_russh_error(error: russh::Error) -> TunnelError {
    match &error {
        russh::Error::IO(io) => match io.kind() {
            std::io::ErrorKind::ConnectionRefused => TunnelError::ConnectionRefused,
            std::io::ErrorKind::TimedOut => TunnelError::Timeout,
            std::io::ErrorKind::Other => {
                let text = io.to_string();
                let lower = text.to_lowercase();
                if lower.contains("lookup")
                    || lower.contains("resolution")
                    || lower.contains("name or service")
                    || text.contains("解析")
                    || text.contains("名称")
                {
                    TunnelError::UnknownHost
                } else {
                    TunnelError::Connect(text)
                }
            }
            _ => TunnelError::Connect(io.to_string()),
        },
        russh::Error::ConnectionTimeout
        | russh::Error::KeepaliveTimeout
        | russh::Error::InactivityTimeout => TunnelError::Timeout,
        russh::Error::HUP | russh::Error::Disconnect => TunnelError::ConnectionClosed,
        _ => {
            let text = error.to_string();
            let lower = text.to_lowercase();
            if lower.contains("name or service not known") || lower.contains("dns error") {
                TunnelError::UnknownHost
            } else {
                TunnelError::Connect(text)
            }
        }
    }
}

async fn timed<F, T>(duration: Duration, future: F) -> Result<T, TunnelError>
where
    F: Future<Output = Result<T, russh::Error>>,
{
    match tokio::time::timeout(duration, future).await {
        Err(_) => Err(TunnelError::Timeout),
        Ok(Err(error)) => Err(map_russh_error(error)),
        Ok(Ok(value)) => Ok(value),
    }
}

/// Verifies the server key against the TOFU store. `strict_host_key = false`
/// behaves like OpenSSH: accept anything and do not pin it.
pub struct ClientHandler {
    host: String,
    port: u16,
    strict: bool,
    known_hosts: Arc<KnownHosts>,
    security_alert: Arc<AtomicBool>,
}

impl ClientHandler {
    fn new(
        host: &str,
        port: u16,
        strict: bool,
        known_hosts: Arc<KnownHosts>,
        security_alert: Arc<AtomicBool>,
    ) -> Self {
        Self {
            host: host.to_string(),
            port,
            strict,
            known_hosts,
            security_alert,
        }
    }
}

impl Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        if !self.strict {
            return Ok(true);
        }
        let public_key = server_public_key.public_key();
        let fingerprint = public_key.fingerprint(HashAlg::Sha256).to_string();
        let key_type = public_key.algorithm().as_str().to_string();
        let host_key = format!("{}:{}", self.host, self.port);
        match self
            .known_hosts
            .check_and_pin(&host_key, &fingerprint, &key_type)
            .await
        {
            HostKeyDecision::Match | HostKeyDecision::Pinned => Ok(true),
            HostKeyDecision::Changed => {
                self.security_alert.store(true, Ordering::SeqCst);
                Ok(false)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Hop {
    host: String,
    port: u16,
    username: String,
    credentials: SshCredentials,
}

pub struct ConnectContext {
    pub secrets: Arc<Secrets>,
    pub known_hosts: Arc<KnownHosts>,
}

/// The local listener and remote target for one running application.
#[derive(Debug, Clone)]
pub struct ForwardSpec {
    pub local_host: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

impl From<&ForwardApp> for ForwardSpec {
    fn from(app: &ForwardApp) -> Self {
        Self {
            local_host: LOOPBACK.to_string(),
            local_port: app.local_port,
            remote_host: app.remote_host.clone(),
            remote_port: app.remote_port,
        }
    }
}

fn hop_chain(tunnel: &TunnelConfig) -> Vec<Hop> {
    let mut hops: Vec<Hop> = tunnel
        .jumps
        .iter()
        .map(|jump| Hop {
            host: jump.host.clone(),
            port: jump.port,
            username: jump.username.clone(),
            credentials: jump.credentials.clone(),
        })
        .collect();
    hops.push(Hop {
        host: tunnel.ssh_host.clone(),
        port: tunnel.ssh_port,
        username: tunnel.ssh_user.clone(),
        credentials: tunnel.credentials.clone(),
    });
    hops
}

fn decrypt_secret(
    context: &ConnectContext,
    cipher: Option<&String>,
) -> Result<Option<String>, TunnelError> {
    context
        .secrets
        .decrypt_opt(cipher)
        .map_err(|_| TunnelError::SecretDecrypt)
}

async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    hop: &Hop,
    context: &ConnectContext,
    timeout: Duration,
) -> Result<(), TunnelError> {
    let result = match hop.credentials.auth_type {
        AuthType::Password => {
            let password = decrypt_secret(context, hop.credentials.password_cipher.as_ref())?
                .ok_or(TunnelError::MissingCredential)?;
            timed(
                timeout,
                handle.authenticate_password(&hop.username, password),
            )
            .await?
        }
        AuthType::PrivateKey => {
            let pem = decrypt_secret(context, hop.credentials.private_key_cipher.as_ref())?
                .ok_or(TunnelError::MissingCredential)?;
            let passphrase = decrypt_secret(context, hop.credentials.passphrase_cipher.as_ref())?;
            let key = decode_secret_key(&pem, passphrase.as_deref())
                .map_err(|error| TunnelError::Connect(format!("私钥无法解析: {error}")))?;
            let hash = handle
                .best_supported_rsa_hash()
                .await
                .ok()
                .flatten()
                .flatten();
            timed(
                timeout,
                handle.authenticate_publickey(
                    &hop.username,
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                ),
            )
            .await?
        }
    };
    match result {
        AuthResult::Success => Ok(()),
        AuthResult::Failure { .. } => Err(TunnelError::AuthFailed),
    }
}

async fn disconnect_handles(handles: &[Handle<ClientHandler>]) {
    for handle in handles.iter().rev() {
        let _ = handle.disconnect(Disconnect::ByApplication, "", "").await;
    }
}

/// Opens every SSH hop in order, authenticates each one, and returns the
/// handles from the nearest jump to the target server. No local listener is
/// created. Both the forwarding connection and the connectivity probe use this.
async fn dial_chain(
    context: &ConnectContext,
    tunnel: &TunnelConfig,
) -> Result<Vec<Handle<ClientHandler>>, TunnelError> {
    let timeout = Duration::from_secs(tunnel.connect_timeout_seconds.clamp(5, 120) as u64);
    let keepalive = Duration::from_secs(tunnel.keep_alive_seconds.clamp(5, 300) as u64);
    let ssh_config = Arc::new(Config {
        keepalive_interval: Some(keepalive),
        keepalive_max: 3,
        nodelay: true,
        ..Config::default()
    });

    let hops = hop_chain(tunnel);
    let mut handles: Vec<Handle<ClientHandler>> = Vec::new();
    let mut upstream: Option<Handle<ClientHandler>> = None;

    for hop in &hops {
        if hop.host.trim().is_empty() {
            disconnect_handles(&handles).await;
            return Err(TunnelError::Connect("SSH 主机地址为空".into()));
        }
        let security_alert = Arc::new(AtomicBool::new(false));
        let handler = ClientHandler::new(
            &hop.host,
            hop.port,
            tunnel.strict_host_key,
            context.known_hosts.clone(),
            security_alert.clone(),
        );

        let mut handle = match upstream.take() {
            None => {
                let result = timed(
                    timeout,
                    client::connect(ssh_config.clone(), (hop.host.as_str(), hop.port), handler),
                )
                .await;
                match result {
                    Ok(handle) => handle,
                    Err(error) => {
                        if security_alert.load(Ordering::SeqCst) {
                            return Err(TunnelError::HostKeyChanged);
                        }
                        return Err(error);
                    }
                }
            }
            Some(parent) => {
                let channel = match timed(
                    timeout,
                    parent.channel_open_direct_tcpip(
                        hop.host.clone(),
                        hop.port as u32,
                        LOOPBACK,
                        0,
                    ),
                )
                .await
                {
                    Ok(channel) => channel,
                    Err(error) => {
                        let _ = parent.disconnect(Disconnect::ByApplication, "", "").await;
                        disconnect_handles(&handles).await;
                        return Err(error);
                    }
                };
                handles.push(parent);
                let result = timed(
                    timeout,
                    client::connect_stream(ssh_config.clone(), channel.into_stream(), handler),
                )
                .await;
                match result {
                    Ok(handle) => handle,
                    Err(error) => {
                        disconnect_handles(&handles).await;
                        if security_alert.load(Ordering::SeqCst) {
                            return Err(TunnelError::HostKeyChanged);
                        }
                        return Err(error);
                    }
                }
            }
        };

        if let Err(error) = authenticate(&mut handle, hop, context, timeout).await {
            let _ = handle.disconnect(Disconnect::ByApplication, "", "").await;
            disconnect_handles(&handles).await;
            return Err(error);
        }
        upstream = Some(handle);
    }

    match upstream {
        Some(target) => {
            handles.push(target);
            Ok(handles)
        }
        None => Err(TunnelError::Connect("没有可用的 SSH 连接".into())),
    }
}

#[derive(Debug)]
pub struct SshTunnel {
    local_port: u16,
    cancel: CancellationToken,
    task: Option<JoinHandle<TunnelError>>,
}

impl SshTunnel {
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// Resolves when the supervisor exits, with the reason. On a user-driven
    /// `close()` this returns `ConnectionClosed`.
    pub async fn wait(&mut self) -> TunnelError {
        match self.task.take() {
            Some(task) => match task.await {
                Ok(reason) => reason,
                Err(_) => TunnelError::ConnectionClosed,
            },
            None => TunnelError::ConnectionClosed,
        }
    }

    pub async fn close(&mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Authenticates through the whole jump chain without installing a local
/// listener, then disconnects. Used by the tunnel connectivity test so a probe
/// never touches a running application or binds a port.
pub async fn probe(context: &ConnectContext, tunnel: &TunnelConfig) -> Result<(), TunnelError> {
    let handles = dial_chain(context, tunnel).await?;
    disconnect_handles(&handles).await;
    Ok(())
}

/// Dials the tunnel and installs the application's local forward.
pub async fn connect(
    context: &ConnectContext,
    tunnel: &TunnelConfig,
    forward: &ForwardSpec,
) -> Result<SshTunnel, TunnelError> {
    let handles = dial_chain(context, tunnel).await?;

    let listener = match TcpListener::bind((forward.local_host.as_str(), forward.local_port)).await
    {
        Ok(listener) => listener,
        Err(error) => {
            disconnect_handles(&handles).await;
            return if error.kind() == std::io::ErrorKind::AddrInUse {
                Err(TunnelError::PortInUse)
            } else {
                Err(TunnelError::Io(error.to_string()))
            };
        }
    };
    let local_port = listener
        .local_addr()
        .map(|addr| addr.port())
        .unwrap_or(forward.local_port);

    let cancel = CancellationToken::new();
    let remote_host = forward.remote_host.clone();
    let remote_port = forward.remote_port;
    let task = tokio::spawn(supervise(
        handles,
        listener,
        remote_host,
        remote_port,
        cancel.clone(),
    ));

    tracing::info!(
        local = %format!("{}:{}", forward.local_host, local_port),
        remote = %format!("{}:{}", forward.remote_host, forward.remote_port),
        "应用转发已建立"
    );

    Ok(SshTunnel {
        local_port,
        cancel,
        task: Some(task),
    })
}

async fn supervise(
    mut handles: Vec<Handle<ClientHandler>>,
    listener: TcpListener,
    remote_host: String,
    remote_port: u16,
    cancel: CancellationToken,
) -> TunnelError {
    let target = match handles.pop() {
        Some(handle) => handle,
        None => return TunnelError::Connect("没有目标 SSH 会话".into()),
    };
    let mut connections: JoinSet<()> = JoinSet::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let reason = loop {
        tokio::select! {
            _ = cancel.cancelled() => break TunnelError::ConnectionClosed,
            _ = ticker.tick() => {
                if target.is_closed() || handles.iter().any(|handle| handle.is_closed()) {
                    break TunnelError::ConnectionClosed;
                }
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
            accepted = listener.accept() => {
                let (socket, peer) = match accepted {
                    Ok(value) => value,
                    Err(_error) => break TunnelError::LocalForwardLost,
                };
                let channel = match target
                    .channel_open_direct_tcpip(
                        remote_host.clone(),
                        remote_port as u32,
                        peer.ip().to_string(),
                        peer.port() as u32,
                    )
                    .await
                {
                    Ok(channel) => channel,
                    Err(error) => {
                        tracing::warn!(%error, "无法打开 direct-tcpip 通道");
                        continue;
                    }
                };
                let mut stream = channel.into_stream();
                let mut socket = socket;
                connections.spawn(async move {
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                    let _ = stream.shutdown().await;
                });
            }
        }
    };

    let _ = target.disconnect(Disconnect::ByApplication, "", "").await;
    disconnect_handles(&handles).await;
    connections.shutdown().await;
    reason
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::JumpHost;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    fn env_or(key: &str, fallback: &str) -> String {
        std::env::var(key).unwrap_or_else(|_| fallback.to_string())
    }

    fn env_required(key: &str, hint: &str) -> String {
        std::env::var(key).unwrap_or_else(|_| panic!("{key} must be set: {hint}"))
    }

    #[test]
    fn maps_io_errors_to_readable_codes() {
        let refused = russh::Error::IO(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));
        assert!(matches!(
            map_russh_error(refused),
            TunnelError::ConnectionRefused
        ));
        let timeout = russh::Error::ConnectionTimeout;
        assert!(matches!(map_russh_error(timeout), TunnelError::Timeout));
    }

    #[test]
    fn forward_spec_comes_from_the_application_not_the_tunnel() {
        let app = ForwardApp {
            local_port: 18080,
            remote_host: "10.0.0.5".into(),
            remote_port: 5000,
            ..ForwardApp::default()
        };
        let spec = ForwardSpec::from(&app);
        assert_eq!(spec.local_port, 18080);
        assert_eq!(spec.local_host, LOOPBACK);
        assert_eq!(spec.remote_host, "10.0.0.5");
        assert_eq!(spec.remote_port, 5000);
    }

    /// Shared fixture for the sshd-backed tests. Everything (master key,
    /// known-hosts store, tunnel config) lives in a temp dir so runs do not
    /// touch the real application data.
    struct E2e {
        dir: tempfile::TempDir,
        secrets: Arc<Secrets>,
        known_hosts: Arc<KnownHosts>,
        tunnel: TunnelConfig,
        forward: ForwardSpec,
    }

    impl E2e {
        async fn new(auth_type: AuthType, secret: &str) -> Self {
            let host = env_or("TD_E2E_HOST", "127.0.0.1");
            let port: u16 = env_or("TD_E2E_PORT", "2222").parse().unwrap();
            let user = env_or("TD_E2E_USER", "rt");
            let dir = tempfile::tempdir().unwrap();
            let secrets = Secrets::load(dir.path()).unwrap();
            let known_hosts = Arc::new(KnownHosts::load(dir.path()).await.unwrap());
            let credentials = match auth_type {
                AuthType::Password => SshCredentials {
                    auth_type,
                    password_cipher: Some(secrets.encrypt(secret).unwrap()),
                    ..SshCredentials::default()
                },
                AuthType::PrivateKey => SshCredentials {
                    auth_type,
                    private_key_cipher: Some(secrets.encrypt(secret).unwrap()),
                    ..SshCredentials::default()
                },
            };
            let tunnel = TunnelConfig {
                name: "e2e".into(),
                ssh_host: host,
                ssh_port: port,
                ssh_user: user,
                credentials,
                keep_alive_seconds: 5,
                connect_timeout_seconds: 10,
                ..TunnelConfig::default()
            };
            let forward = ForwardSpec {
                local_host: LOOPBACK.to_string(),
                local_port: 0,
                remote_host: env_or("TD_E2E_REMOTE_HOST", "127.0.0.1"),
                remote_port: env_or("TD_E2E_REMOTE_PORT", "8080").parse().unwrap(),
            };
            Self {
                dir,
                secrets,
                known_hosts,
                tunnel,
                forward,
            }
        }

        fn context(&self) -> ConnectContext {
            ConnectContext {
                secrets: self.secrets.clone(),
                known_hosts: self.known_hosts.clone(),
            }
        }

        fn host_key(&self) -> String {
            format!("{}:{}", self.tunnel.ssh_host, self.tunnel.ssh_port)
        }
    }

    async fn assert_http_forwarded(local_port: u16) {
        let marker = env_or("TD_E2E_EXPECT", "TUNNELDOCK_E2E_MARKER_42");
        let mut stream = TcpStream::connect(("127.0.0.1", local_port)).await.unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let text = String::from_utf8_lossy(&response);
        assert!(text.contains(&marker), "unexpected response: {text}");
    }

    /// End-to-end forwarding against a real sshd. Run with:
    /// `TD_E2E_KEY=/path/to/key TD_E2E_WITH_JUMP=1 cargo test e2e_forward -- --ignored`
    #[tokio::test]
    #[ignore = "requires a local sshd and HTTP server"]
    async fn e2e_forward() {
        let private_key = std::fs::read_to_string(env_required(
            "TD_E2E_KEY",
            "path to an authorized private key",
        ))
        .unwrap();
        let setup = E2e::new(AuthType::PrivateKey, &private_key).await;
        let mut tunnel = setup.tunnel.clone();
        if std::env::var("TD_E2E_WITH_JUMP").is_ok() {
            tunnel.jumps = vec![JumpHost {
                host: tunnel.ssh_host.clone(),
                port: tunnel.ssh_port,
                username: tunnel.ssh_user.clone(),
                credentials: tunnel.credentials.clone(),
                ..JumpHost::default()
            }];
        }

        let mut tunnel = connect(&setup.context(), &tunnel, &setup.forward)
            .await
            .expect("tunnel connects");
        assert!(tunnel.local_port() > 0);
        assert_http_forwarded(tunnel.local_port()).await;
        tunnel.close().await;
    }

    /// The probe authenticates through the chain but never binds a local port.
    #[tokio::test]
    #[ignore = "requires a local sshd"]
    async fn e2e_probe_authenticates_without_forward() {
        let private_key =
            std::fs::read_to_string(env_required("TD_E2E_KEY", "private key")).unwrap();
        let setup = E2e::new(AuthType::PrivateKey, &private_key).await;
        probe(&setup.context(), &setup.tunnel)
            .await
            .expect("probe authenticates");
    }

    /// A pinned host key that no longer matches must abort the connection and
    /// surface the security error instead of silently re-pinning.
    #[tokio::test]
    #[ignore = "requires a local sshd"]
    async fn e2e_rejects_changed_host_key() {
        let private_key =
            std::fs::read_to_string(env_required("TD_E2E_KEY", "private key")).unwrap();
        let setup = E2e::new(AuthType::PrivateKey, &private_key).await;
        let mut tunnel = connect(&setup.context(), &setup.tunnel, &setup.forward)
            .await
            .expect("first connect pins the key");
        tunnel.close().await;

        let mut hosts = serde_json::Map::new();
        hosts.insert(
            setup.host_key(),
            serde_json::json!({
                "fingerprint": "SHA256:changed-on-purpose",
                "keyType": "ssh-ed25519",
                "addedAt": 0
            }),
        );
        let document = serde_json::json!({ "version": 1, "hosts": hosts });
        tokio::fs::write(
            setup.dir.path().join("known_hosts.json"),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .await
        .unwrap();

        let known_hosts = Arc::new(KnownHosts::load(setup.dir.path()).await.unwrap());
        let context = ConnectContext {
            secrets: setup.secrets.clone(),
            known_hosts,
        };
        let error = connect(&context, &setup.tunnel, &setup.forward)
            .await
            .expect_err("changed host key must be refused");
        assert!(
            matches!(error, TunnelError::HostKeyChanged),
            "unexpected error: {error:?}"
        );
        assert!(error.security_alert());
    }

    #[tokio::test]
    #[ignore = "requires a local sshd"]
    async fn e2e_reports_port_in_use() {
        let private_key =
            std::fs::read_to_string(env_required("TD_E2E_KEY", "private key")).unwrap();
        let setup = E2e::new(AuthType::PrivateKey, &private_key).await;
        let occupied = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = occupied.local_addr().unwrap().port();
        let mut forward = setup.forward.clone();
        forward.local_port = port;

        let error = connect(&setup.context(), &setup.tunnel, &forward)
            .await
            .expect_err("bound local port must be refused");
        assert!(
            matches!(error, TunnelError::PortInUse),
            "unexpected error: {error:?}"
        );
    }

    #[tokio::test]
    #[ignore = "requires a local sshd"]
    async fn e2e_reports_auth_failure() {
        let unauthorized = std::fs::read_to_string(env_required(
            "TD_E2E_BAD_KEY",
            "an ed25519 private key that is NOT in authorized_keys",
        ))
        .unwrap();
        let setup = E2e::new(AuthType::PrivateKey, &unauthorized).await;
        let error = connect(&setup.context(), &setup.tunnel, &setup.forward)
            .await
            .expect_err("unauthorized key must fail");
        assert!(
            matches!(error, TunnelError::AuthFailed),
            "unexpected error: {error:?}"
        );
    }

    /// Password auth against an sshd configured with a known password.
    #[tokio::test]
    #[ignore = "requires a local sshd with password authentication and TD_E2E_PASSWORD"]
    async fn e2e_password_auth() {
        let password = env_required("TD_E2E_PASSWORD", "test account password");
        let setup = E2e::new(AuthType::Password, &password).await;
        let mut tunnel = connect(&setup.context(), &setup.tunnel, &setup.forward)
            .await
            .expect("password auth connects");
        assert_http_forwarded(tunnel.local_port()).await;
        tunnel.close().await;
    }
}
