//! 线路选择策略：一轮连接里该按什么顺序拨号，以及某个失败要不要换下一条线路。
//!
//! 规则刻意做成纯函数，便于单元测试，也不依赖 Tauri / 网络：
//!
//! 1. 只有**可拨号**的线路是候选（启用 + 地址/用户/端口/凭据齐全）。
//! 2. `wifi_only` 的线路在非局域网下被跳过——这才让“家里局域网”这条线路
//!    在任何时候都可以留着配置。
//! 3. 用户钉住的首选线路优先；否则先试上次成功的线路，重连时不必重新探测
//!    刚刚失败的路径。
//! 4. 其余线路按编辑器里的声明顺序。

use crate::model::{TunnelConfig, TunnelEndpoint};
use crate::ssh::TunnelError;

/// 为什么一条线路都拨不出去——界面据此给出可执行的提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoCandidateReason {
    /// 隧道还没有配置任何线路。
    NoEndpoints,
    /// 所有线路都被停用了。
    AllDisabled,
    /// 可用的线路只有“仅局域网”的，而当前不在局域网。
    WifiOnly,
    /// 线路存在但缺地址、用户名、端口或凭据。
    Incomplete,
}

impl NoCandidateReason {
    /// 稳定的语言无关错误码，文案由界面提供。
    pub fn error_code(self) -> &'static str {
        match self {
            NoCandidateReason::NoEndpoints => "no_endpoint",
            NoCandidateReason::AllDisabled => "all_endpoints_disabled",
            NoCandidateReason::WifiOnly => "wifi_only_endpoint",
            NoCandidateReason::Incomplete => "endpoint_incomplete",
        }
    }
}

/// 有序的候选线路列表，用于一轮连接。
///
/// - `pinned`：用户手工钉住的首选线路 id（`None` 表示自动）；
/// - `last_good`：上一次成功承载流量的线路 id（`None` 表示还没有）；
/// - `on_local_network`：本机当前是否在局域网（有线 / Wi-Fi）。
pub fn order<'a>(
    tunnel: &'a TunnelConfig,
    pinned: Option<&str>,
    last_good: Option<&str>,
    on_local_network: bool,
) -> Vec<&'a TunnelEndpoint> {
    let candidates: Vec<&TunnelEndpoint> = tunnel
        .dialable_endpoints()
        .into_iter()
        .filter(|endpoint| on_local_network || !endpoint.wifi_only)
        .collect();
    if candidates.is_empty() {
        return candidates;
    }
    let preferred = [pinned, last_good]
        .into_iter()
        .flatten()
        .find_map(|id| candidates.iter().copied().find(|endpoint| endpoint.id == id));
    match preferred {
        None => candidates,
        Some(first) => {
            let mut ordered = Vec::with_capacity(candidates.len());
            ordered.push(first);
            ordered.extend(
                candidates
                    .iter()
                    .copied()
                    .filter(|endpoint| endpoint.id != first.id),
            );
            ordered
        }
    }
}

/// 因为网络条件被跳过的线路，让界面可以解释“共 2 条线路，当前网络可用 1 条”。
pub fn skipped_for_network<'a>(
    tunnel: &'a TunnelConfig,
    on_local_network: bool,
) -> Vec<&'a TunnelEndpoint> {
    if on_local_network {
        return Vec::new();
    }
    tunnel
        .dialable_endpoints()
        .into_iter()
        .filter(|endpoint| endpoint.wifi_only)
        .collect()
}

/// 一条线路都拨不出去时，到底是哪种原因。
pub fn no_candidate_reason(tunnel: &TunnelConfig, on_local_network: bool) -> NoCandidateReason {
    if tunnel.endpoints.is_empty() {
        return NoCandidateReason::NoEndpoints;
    }
    if !tunnel.endpoints.iter().any(|endpoint| endpoint.enabled) {
        return NoCandidateReason::AllDisabled;
    }
    let dialable = tunnel.dialable_endpoints();
    let skipped = skipped_for_network(tunnel, on_local_network);
    if !skipped.is_empty() && skipped.len() == dialable.len() {
        return NoCandidateReason::WifiOnly;
    }
    NoCandidateReason::Incomplete
}

/// 这个失败是否“换条线路可能就好了”。
///
/// 安全类与本地端口类失败是应用级问题：换 SSH 路径既解决不了，还会把真正的原因
/// 盖住，因此必须当场结束本轮。
pub fn is_endpoint_scoped(error: &TunnelError) -> bool {
    !matches!(
        error,
        TunnelError::PortInUse
            | TunnelError::HostKeyChanged
            | TunnelError::MissingCredential
            | TunnelError::SecretDecrypt
    )
}

/// 本地端口问题换哪条线路都没用。
pub fn is_local_port_failure(error: &TunnelError) -> bool {
    matches!(error, TunnelError::PortInUse)
}

/// 安全类失败必须停止自动重连，绝不能悄悄换下一条线路。
pub fn is_security_failure(error: &TunnelError) -> bool {
    matches!(error, TunnelError::HostKeyChanged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AuthType, SshCredentials, TunnelEndpoint};

    fn endpoint(
        id: &str,
        host: &str,
        wifi_only: bool,
        enabled: bool,
        with_secret: bool,
    ) -> TunnelEndpoint {
        TunnelEndpoint {
            id: id.into(),
            label: id.into(),
            host: host.into(),
            username: "ops".into(),
            credentials: if with_secret {
                SshCredentials {
                    auth_type: AuthType::Password,
                    password_cipher: Some("v1.aa.bb".into()),
                    ..SshCredentials::default()
                }
            } else {
                SshCredentials::default()
            },
            wifi_only,
            enabled,
            ..TunnelEndpoint::default()
        }
    }

    fn home() -> TunnelEndpoint {
        endpoint("home", "192.168.1.10", true, true, true)
    }

    fn away() -> TunnelEndpoint {
        endpoint("away", "frp.example.com", false, true, true)
    }

    fn tunnel(endpoints: Vec<TunnelEndpoint>, preferred: Option<&str>) -> TunnelConfig {
        TunnelConfig {
            name: "NAS".into(),
            endpoints,
            preferred_endpoint_id: preferred.map(str::to_string),
            ..TunnelConfig::default()
        }
    }

    fn ids(endpoints: Vec<&TunnelEndpoint>) -> Vec<&str> {
        endpoints.into_iter().map(|item| item.id.as_str()).collect()
    }

    #[test]
    fn on_the_local_network_the_declaration_order_is_preserved() {
        let config = tunnel(vec![home(), away()], None);
        let ordered = order(&config, None, None, true);
        assert_eq!(ids(ordered), vec!["home", "away"]);
    }

    #[test]
    fn away_from_the_local_network_wifi_only_lines_are_skipped() {
        let config = tunnel(vec![home(), away()], None);
        let ordered = order(&config, None, None, false);
        assert_eq!(ids(ordered), vec!["away"]);
        assert_eq!(ids(skipped_for_network(&config, false)), vec!["home"]);
        assert!(skipped_for_network(&config, true).is_empty());
    }

    #[test]
    fn a_pinned_line_is_tried_first_but_stays_fallback_able() {
        let config = tunnel(vec![away(), home()], Some("home"));
        let ordered = order(&config, Some("home"), None, true);
        assert_eq!(ids(ordered), vec!["home", "away"]);
    }

    #[test]
    fn the_line_that_worked_last_time_is_tried_before_the_others() {
        let config = tunnel(vec![home(), away()], None);
        let ordered = order(&config, None, Some("away"), true);
        assert_eq!(ids(ordered), vec!["away", "home"]);
    }

    #[test]
    fn a_pinned_line_wins_over_the_last_good_one() {
        let config = tunnel(vec![home(), away()], Some("home"));
        let ordered = order(&config, Some("home"), Some("away"), true);
        assert_eq!(ids(ordered), vec!["home", "away"]);
    }

    #[test]
    fn a_pinned_line_that_is_unreachable_on_this_network_is_ignored() {
        let config = tunnel(vec![home(), away()], Some("home"));
        let ordered = order(&config, Some("home"), None, false);
        assert_eq!(ids(ordered), vec!["away"]);
    }

    #[test]
    fn a_stale_pinned_id_falls_back_to_the_declaration_order() {
        let config = tunnel(vec![home(), away()], Some("deleted"));
        let ordered = order(&config, Some("deleted"), Some("also-gone"), true);
        assert_eq!(ids(ordered), vec!["home", "away"]);
    }

    #[test]
    fn disabled_and_secretless_lines_never_become_candidates() {
        let config = tunnel(
            vec![
                endpoint("off", "a.example.com", false, false, true),
                endpoint("nosecret", "b.example.com", false, true, false),
                away(),
            ],
            None,
        );
        let ordered = order(&config, Some("off"), None, true);
        assert_eq!(ids(ordered), vec!["away"]);
    }

    #[test]
    fn no_candidate_reasons_distinguish_the_failure_causes() {
        assert_eq!(
            no_candidate_reason(&tunnel(Vec::new(), None), true),
            NoCandidateReason::NoEndpoints
        );
        assert_eq!(
            no_candidate_reason(
                &tunnel(vec![endpoint("off", "a.example.com", false, false, true)], None),
                true
            ),
            NoCandidateReason::AllDisabled
        );
        assert_eq!(
            no_candidate_reason(&tunnel(vec![home()], None), false),
            NoCandidateReason::WifiOnly
        );
        assert_eq!(
            no_candidate_reason(
                &tunnel(vec![endpoint("nosecret", "a.example.com", false, true, false)], None),
                true
            ),
            NoCandidateReason::Incomplete
        );
    }

    #[test]
    fn every_reason_exposes_a_stable_error_code() {
        for reason in [
            NoCandidateReason::NoEndpoints,
            NoCandidateReason::AllDisabled,
            NoCandidateReason::WifiOnly,
            NoCandidateReason::Incomplete,
        ] {
            assert!(!reason.error_code().is_empty());
        }
        assert_eq!(
            NoCandidateReason::WifiOnly.error_code(),
            "wifi_only_endpoint"
        );
    }

    #[test]
    fn local_port_and_security_failures_stop_the_round() {
        assert!(!is_endpoint_scoped(&TunnelError::PortInUse));
        assert!(!is_endpoint_scoped(&TunnelError::HostKeyChanged));
        assert!(!is_endpoint_scoped(&TunnelError::MissingCredential));
        assert!(!is_endpoint_scoped(&TunnelError::SecretDecrypt));
        assert!(is_endpoint_scoped(&TunnelError::Timeout));
        assert!(is_endpoint_scoped(&TunnelError::AuthFailed));
        assert!(is_endpoint_scoped(&TunnelError::ConnectionRefused));
        assert!(is_endpoint_scoped(&TunnelError::UnknownHost));
        assert!(is_endpoint_scoped(&TunnelError::Connect("boom".into())));
    }

    #[test]
    fn security_and_port_failures_are_classified_for_the_ui() {
        assert!(is_security_failure(&TunnelError::HostKeyChanged));
        assert!(!is_security_failure(&TunnelError::Timeout));
        assert!(is_local_port_failure(&TunnelError::PortInUse));
        assert!(!is_local_port_failure(&TunnelError::Timeout));
    }
}
