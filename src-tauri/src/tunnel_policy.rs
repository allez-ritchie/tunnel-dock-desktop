//! 隧道选择策略：应用启动时该按什么顺序拨号，以及某个失败要不要换下一条隧道。
//!
//! 应用持有一个**有序的隧道列表**，顺序由用户在应用编辑器里决定。规则刻意做成
//! 纯函数，便于单元测试，也不依赖 Tauri / 网络：
//!
//! 1. 只保留引用得到、且真的可拨号的隧道（地址 / 用户名 / 端口 / 凭据齐全）。
//! 2. 顺序就是应用里的声明顺序，第一条是首选；换隧道不会改变本地端口。
//! 3. 安全类与本地端口类失败是应用级问题：换 SSH 入口既解决不了，还会把真正的
//!    原因盖住，因此必须当场结束本轮。

use crate::model::{DockSnapshot, ForwardApp, TunnelConfig};
use crate::ssh::TunnelError;

/// 一条隧道都拨不出去——界面据此给出可执行的提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoTunnelReason {
    /// 应用还没有选择任何隧道。
    NoTunnels,
    /// 应用引用的隧道全都被删除了。
    TunnelMissing,
    /// 隧道存在但缺地址、用户名、端口或凭据。
    Incomplete,
}

impl NoTunnelReason {
    /// 稳定的语言无关错误码，文案由界面提供。
    pub fn error_code(self) -> &'static str {
        match self {
            NoTunnelReason::NoTunnels => "no_tunnel",
            NoTunnelReason::TunnelMissing => "tunnel_missing",
            NoTunnelReason::Incomplete => "tunnel_incomplete",
        }
    }
}

/// 应用按尝试顺序可以拨号的隧道。
pub fn candidates<'a>(snapshot: &'a DockSnapshot, app: &ForwardApp) -> Vec<&'a TunnelConfig> {
    snapshot.dialable_tunnels_for(app)
}

/// 一条隧道都拨不出去时，到底是哪种原因。
pub fn no_tunnel_reason(snapshot: &DockSnapshot, app: &ForwardApp) -> NoTunnelReason {
    if app.tunnel_ids.is_empty() {
        return NoTunnelReason::NoTunnels;
    }
    if app.tunnel_ids.iter().all(|id| snapshot.tunnel(id).is_none()) {
        return NoTunnelReason::TunnelMissing;
    }
    NoTunnelReason::Incomplete
}

/// 这个失败是否“换条隧道可能就好了”。
///
/// 安全类与本地端口类失败是应用级问题：换 SSH 路径既解决不了，还会把真正的原因
/// 盖住，因此必须当场结束本轮。
pub fn is_tunnel_scoped(error: &TunnelError) -> bool {
    !matches!(
        error,
        TunnelError::PortInUse
            | TunnelError::HostKeyChanged
            | TunnelError::MissingCredential
            | TunnelError::SecretDecrypt
    )
}

/// 本地端口问题换哪条隧道都没用。
pub fn is_local_port_failure(error: &TunnelError) -> bool {
    matches!(error, TunnelError::PortInUse)
}

/// 安全类失败必须停止自动重连，绝不能悄悄换下一条隧道。
pub fn is_security_failure(error: &TunnelError) -> bool {
    matches!(error, TunnelError::HostKeyChanged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AppGroup, AuthType, SshCredentials};

    fn credentials(with_secret: bool) -> SshCredentials {
        SshCredentials {
            auth_type: AuthType::Password,
            password_cipher: with_secret.then(|| "v1.aa.bb".to_string()),
            ..SshCredentials::default()
        }
    }

    fn tunnel(id: &str, host: &str, with_secret: bool) -> TunnelConfig {
        TunnelConfig {
            id: id.into(),
            name: id.into(),
            host: host.into(),
            username: "ops".into(),
            credentials: credentials(with_secret),
            ..TunnelConfig::default()
        }
    }

    fn app(tunnel_ids: &[&str]) -> ForwardApp {
        ForwardApp {
            name: "Web".into(),
            tunnel_ids: tunnel_ids.iter().map(|id| id.to_string()).collect(),
            remote_host: "10.0.0.5".into(),
            ..ForwardApp::default()
        }
    }

    fn snapshot(tunnels: Vec<TunnelConfig>) -> DockSnapshot {
        DockSnapshot {
            tunnels,
            apps: Vec::new(),
            groups: vec![AppGroup::default_group()],
            ..DockSnapshot::default()
        }
        .normalized()
    }

    fn ids(tunnels: Vec<&TunnelConfig>) -> Vec<&str> {
        tunnels.into_iter().map(|item| item.id.as_str()).collect()
    }

    #[test]
    fn candidates_follow_the_application_order() {
        let document = snapshot(vec![
            tunnel("home", "192.168.1.10", true),
            tunnel("away", "frp.example.com", true),
        ]);
        let app = app(&["away", "home"]);
        assert_eq!(ids(candidates(&document, &app)), vec!["away", "home"]);
    }

    #[test]
    fn disabled_or_secretless_tunnels_never_become_candidates() {
        let document = snapshot(vec![
            tunnel("nosecret", "a.example.com", false),
            tunnel("ok", "b.example.com", true),
            TunnelConfig {
                port: 0,
                ..tunnel("badport", "c.example.com", true)
            },
        ]);
        let app = app(&["nosecret", "badport", "ok"]);
        assert_eq!(ids(candidates(&document, &app)), vec!["ok"]);
    }

    /// 引用了不存在的隧道时跳过它，而不是让整个应用失效。
    #[test]
    fn a_deleted_tunnel_is_skipped_but_the_others_still_work() {
        let document = snapshot(vec![tunnel("away", "frp.example.com", true)]);
        let app = app(&["gone", "away"]);
        assert_eq!(ids(candidates(&document, &app)), vec!["away"]);
    }

    #[test]
    fn no_tunnel_reasons_distinguish_the_failure_causes() {
        let document = snapshot(vec![
            tunnel("ok", "a.example.com", true),
            tunnel("nosecret", "b.example.com", false),
        ]);
        assert_eq!(
            no_tunnel_reason(&document, &app(&[])),
            NoTunnelReason::NoTunnels
        );
        assert_eq!(
            no_tunnel_reason(&document, &app(&["gone", "also-gone"])),
            NoTunnelReason::TunnelMissing
        );
        assert_eq!(
            no_tunnel_reason(&document, &app(&["nosecret"])),
            NoTunnelReason::Incomplete
        );
    }

    /// 一条可用、一条不可用时不是“没有可用隧道”，候选里还有那一条。
    #[test]
    fn one_usable_tunnel_is_enough() {
        let document = snapshot(vec![
            tunnel("nosecret", "a.example.com", false),
            tunnel("ok", "b.example.com", true),
        ]);
        let app = app(&["nosecret", "ok"]);
        assert!(!candidates(&document, &app).is_empty());
    }

    #[test]
    fn every_reason_exposes_a_stable_error_code() {
        for reason in [
            NoTunnelReason::NoTunnels,
            NoTunnelReason::TunnelMissing,
            NoTunnelReason::Incomplete,
        ] {
            assert!(!reason.error_code().is_empty());
        }
        assert_eq!(NoTunnelReason::TunnelMissing.error_code(), "tunnel_missing");
    }

    #[test]
    fn local_port_and_security_failures_stop_the_round() {
        assert!(!is_tunnel_scoped(&TunnelError::PortInUse));
        assert!(!is_tunnel_scoped(&TunnelError::HostKeyChanged));
        assert!(!is_tunnel_scoped(&TunnelError::MissingCredential));
        assert!(!is_tunnel_scoped(&TunnelError::SecretDecrypt));
        assert!(is_tunnel_scoped(&TunnelError::Timeout));
        assert!(is_tunnel_scoped(&TunnelError::AuthFailed));
        assert!(is_tunnel_scoped(&TunnelError::ConnectionRefused));
        assert!(is_tunnel_scoped(&TunnelError::UnknownHost));
        assert!(is_tunnel_scoped(&TunnelError::Connect("boom".into())));
    }

    #[test]
    fn security_and_port_failures_are_classified_for_the_ui() {
        assert!(is_security_failure(&TunnelError::HostKeyChanged));
        assert!(!is_security_failure(&TunnelError::Timeout));
        assert!(is_local_port_failure(&TunnelError::PortInUse));
        assert!(!is_local_port_failure(&TunnelError::Timeout));
    }
}
