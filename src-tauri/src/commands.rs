use crate::model::{
    new_id, now_millis, AppGroup, AppInput, AppStatus, AppView, AuthType, CredentialInput,
    DockSnapshot, ForwardApp, GroupInput, GroupView, JumpHost, JumpInput, SshCredentials,
    TunnelConfig, TunnelInput, TunnelTestResult, TunnelView, DEFAULT_GROUP_ID,
};
use crate::secrets::Secrets;
use crate::AppState;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// One atomic frontend snapshot: definitions plus live statuses. Keeping this in
/// a single command avoids rendering a half-updated list during startup.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockView {
    pub tunnels: Vec<TunnelView>,
    pub apps: Vec<AppView>,
    pub groups: Vec<GroupView>,
    pub statuses: Vec<AppStatus>,
}

#[tauri::command]
pub async fn load_dock(state: State<'_, AppState>) -> Result<DockView, String> {
    let snapshot = state.store.snapshot().await;
    let app_ids: HashSet<String> = snapshot.apps.iter().map(|app| app.id.clone()).collect();
    let statuses = state
        .runtime
        .statuses_snapshot()
        .await
        .into_iter()
        .filter(|status| app_ids.contains(&status.app_id))
        .collect();
    Ok(DockView {
        tunnels: snapshot.tunnels.iter().map(TunnelView::from).collect(),
        apps: snapshot.apps.iter().map(AppView::from).collect(),
        groups: snapshot.groups.iter().map(GroupView::from).collect(),
        statuses,
    })
}

#[tauri::command]
pub async fn save_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    input: TunnelInput,
) -> Result<TunnelView, String> {
    let existing = match input.id.as_deref() {
        Some(id) => state.store.get_tunnel(id).await,
        None => None,
    };
    let config = build_tunnel(input, existing.as_ref(), &state.secrets)?;
    let saved = state.runtime.clone().save_tunnel(config).await?;
    crate::sync_autostart(&app).await;
    Ok(TunnelView::from(&saved))
}

#[tauri::command]
pub async fn delete_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.runtime.delete_tunnel(&id).await;
    crate::sync_autostart(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn test_tunnel(
    state: State<'_, AppState>,
    id: String,
) -> Result<TunnelTestResult, String> {
    Ok(state.runtime.test_tunnel(&id).await)
}

#[tauri::command]
pub async fn save_app(
    app: AppHandle,
    state: State<'_, AppState>,
    input: AppInput,
) -> Result<AppView, String> {
    let snapshot = state.store.snapshot().await;
    let existing = match input.id.as_deref() {
        Some(id) => state.store.get_app(id).await,
        None => None,
    };
    let config = build_app(input, existing.as_ref(), &snapshot)?;
    let restart = existing
        .as_ref()
        .map(|item| item.desired_on)
        .unwrap_or(false);
    if restart {
        state.runtime.stop_app(&config.id).await;
    }
    let saved = state
        .store
        .upsert_app(config)
        .await
        .map_err(|error| error.to_string())?;
    if restart {
        // A configuration change must never be reported as "not saved" just
        // because the new forward cannot start; the card surfaces the reason.
        if let Err(error) = state.runtime.start_app(&saved.id).await {
            tracing::warn!(%error, "应用配置已保存，但按新配置重启失败");
        }
    }
    crate::sync_autostart(&app).await;
    Ok(AppView::from(&saved))
}

#[tauri::command]
pub async fn delete_app(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.runtime.delete_app(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn start_app(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.runtime.clone().start_app(&id).await
}

#[tauri::command]
pub async fn stop_app(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.runtime.stop_app(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn restart_app(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.runtime.clone().restart_app(&id).await
}

#[tauri::command]
pub async fn start_group(state: State<'_, AppState>, group_id: String) -> Result<(), String> {
    state.runtime.clone().start_group(&group_id).await
}

#[tauri::command]
pub async fn stop_group(state: State<'_, AppState>, group_id: String) -> Result<(), String> {
    state.runtime.stop_group(&group_id).await;
    Ok(())
}

#[tauri::command]
pub async fn save_group(
    state: State<'_, AppState>,
    input: GroupInput,
) -> Result<GroupView, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("请填写应用组名称".into());
    }
    if name.chars().count() > 40 {
        return Err("应用组名称不能超过 40 个字符".into());
    }
    let document = state.store.snapshot().await;
    let existing = input
        .id
        .as_deref()
        .and_then(|id| document.groups.iter().find(|group| group.id == id).cloned());
    let id = input.id.clone().unwrap_or_else(new_id);
    if id == DEFAULT_GROUP_ID {
        return Err("默认应用组不能重命名".into());
    }
    if let Some(other) = document
        .groups
        .iter()
        .find(|group| group.id != id && group.name.eq_ignore_ascii_case(name))
    {
        return Err(format!("应用组“{}”已存在", other.name));
    }
    let group = state
        .store
        .upsert_group(AppGroup {
            id,
            name: name.to_string(),
            created_at: existing
                .map(|group| group.created_at)
                .unwrap_or_else(now_millis),
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(GroupView::from(&group))
}

#[tauri::command]
pub async fn delete_group(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.runtime.clone().delete_group(&id).await
}

#[tauri::command]
pub async fn known_hosts_count(state: State<'_, AppState>) -> Result<usize, String> {
    Ok(state.known_hosts.count().await)
}

#[tauri::command]
pub async fn clear_known_hosts(state: State<'_, AppState>) -> Result<(), String> {
    state.known_hosts.clear().await;
    Ok(())
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    let valid = lower.starts_with("http://") || lower.starts_with("https://");
    if !valid {
        return Err("仅支持打开 http:// 或 https:// 地址".into());
    }
    open::that_detached(trimmed).map_err(|error| format!("无法打开浏览器: {error}"))
}

#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn merge_secret(
    existing: Option<&String>,
    input: Option<&String>,
    secrets: &Arc<Secrets>,
) -> Result<Option<String>, String> {
    match input {
        None => Ok(existing.cloned()),
        Some(value) if value.is_empty() => Ok(None),
        Some(value) => secrets
            .encrypt(value)
            .map(Some)
            .map_err(|error| error.to_string()),
    }
}

fn merge_credentials(
    existing: &SshCredentials,
    input: &CredentialInput,
    secrets: &Arc<Secrets>,
) -> Result<SshCredentials, String> {
    Ok(SshCredentials {
        auth_type: input.auth_type,
        password_cipher: merge_secret(
            existing.password_cipher.as_ref(),
            input.password.as_ref(),
            secrets,
        )?,
        private_key_cipher: merge_secret(
            existing.private_key_cipher.as_ref(),
            input.private_key.as_ref(),
            secrets,
        )?,
        passphrase_cipher: merge_secret(
            existing.passphrase_cipher.as_ref(),
            input.passphrase.as_ref(),
            secrets,
        )?,
    })
}

fn build_jumps(
    input: &[JumpInput],
    existing: &[JumpHost],
    secrets: &Arc<Secrets>,
) -> Result<Vec<JumpHost>, String> {
    if input.len() > 8 {
        return Err("最多支持 8 级跳板机".into());
    }
    let mut jumps = Vec::with_capacity(input.len());
    for jump in input {
        if jump.host.trim().is_empty() {
            return Err("请填写跳板机地址".into());
        }
        if jump.port == 0 {
            return Err("跳板机端口需在 1–65535 之间".into());
        }
        if jump.username.trim().is_empty() {
            return Err("请填写跳板机用户名".into());
        }
        let id = jump.id.clone().unwrap_or_else(new_id);
        let previous = existing.iter().find(|item| item.id == id);
        let credentials = match previous {
            Some(previous) => merge_credentials(&previous.credentials, &jump.credentials, secrets)?,
            None => merge_credentials(&SshCredentials::default(), &jump.credentials, secrets)?,
        };
        jumps.push(JumpHost {
            id,
            label: jump.label.trim().to_string(),
            host: jump.host.trim().to_string(),
            port: jump.port,
            username: jump.username.trim().to_string(),
            credentials,
        });
    }
    Ok(jumps)
}

fn build_tunnel(
    input: TunnelInput,
    existing: Option<&TunnelConfig>,
    secrets: &Arc<Secrets>,
) -> Result<TunnelConfig, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("请填写隧道名称".into());
    }
    if name.chars().count() > 40 {
        return Err("名称不能超过 40 个字符".into());
    }
    if input.ssh_host.trim().is_empty() || input.ssh_host.contains(char::is_whitespace) {
        return Err("请填写 SSH 主机地址".into());
    }
    if input.ssh_port == 0 {
        return Err("SSH 端口需在 1–65535 之间".into());
    }
    if input.ssh_user.trim().is_empty() {
        return Err("请填写 SSH 用户名".into());
    }
    if input.keep_alive_seconds < 5 || input.keep_alive_seconds > 300 {
        return Err("保活间隔需在 5–300 秒之间".into());
    }
    if input.connect_timeout_seconds < 5 || input.connect_timeout_seconds > 120 {
        return Err("连接超时需在 5–120 秒之间".into());
    }

    let id = input
        .id
        .clone()
        .or_else(|| existing.map(|config| config.id.clone()))
        .unwrap_or_else(new_id);
    let previous_credentials = existing
        .map(|config| config.credentials.clone())
        .unwrap_or_default();

    let config = TunnelConfig {
        id,
        name: name.to_string(),
        ssh_host: input.ssh_host.trim().to_string(),
        ssh_port: input.ssh_port,
        ssh_user: input.ssh_user.trim().to_string(),
        credentials: merge_credentials(&previous_credentials, &input.credentials, secrets)?,
        jumps: build_jumps(
            &input.jumps,
            existing
                .map(|config| config.jumps.as_slice())
                .unwrap_or(&[]),
            secrets,
        )?,
        auto_reconnect: input.auto_reconnect,
        keep_alive_seconds: input.keep_alive_seconds,
        connect_timeout_seconds: input.connect_timeout_seconds,
        strict_host_key: input.strict_host_key,
        created_at: existing
            .map(|config| config.created_at)
            .unwrap_or_else(now_millis),
        updated_at: now_millis(),
    };

    match input.credentials.auth_type {
        AuthType::Password if config.credentials.password_cipher.is_none() => {
            return Err("请填写登录密码".into());
        }
        AuthType::PrivateKey if config.credentials.private_key_cipher.is_none() => {
            return Err("请粘贴私钥内容".into());
        }
        _ => {}
    }
    Ok(config)
}

fn build_app(
    input: AppInput,
    existing: Option<&ForwardApp>,
    snapshot: &DockSnapshot,
) -> Result<ForwardApp, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("请填写应用名称".into());
    }
    if name.chars().count() > 40 {
        return Err("应用名称不能超过 40 个字符".into());
    }
    let group_id = if input.group_id.trim().is_empty() {
        DEFAULT_GROUP_ID.to_string()
    } else {
        input.group_id.trim().to_string()
    };
    if snapshot.group(&group_id).is_none() {
        return Err("所选应用组不存在".into());
    }
    if snapshot.tunnel(&input.tunnel_id).is_none() {
        return Err("所选隧道不存在，请先创建或选择可用隧道".into());
    }
    if input.local_port < 1024 {
        return Err("本地端口需在 1024–65535 之间".into());
    }
    if input.remote_host.trim().is_empty() || input.remote_host.contains(char::is_whitespace) {
        return Err("请填写远程主机".into());
    }
    if input.remote_port == 0 {
        return Err("远程端口需在 1–65535 之间".into());
    }
    let browser_url = input.browser_url.trim().to_string();
    let browser_url_lower = browser_url.to_ascii_lowercase();
    if !browser_url.is_empty()
        && !browser_url_lower.starts_with("http://")
        && !browser_url_lower.starts_with("https://")
    {
        return Err("地址需以 http:// 或 https:// 开头".into());
    }

    Ok(ForwardApp {
        id: input
            .id
            .or_else(|| existing.map(|app| app.id.clone()))
            .unwrap_or_else(new_id),
        name: name.to_string(),
        group_id,
        tunnel_id: input.tunnel_id,
        local_port: input.local_port,
        remote_host: input.remote_host.trim().to_string(),
        remote_port: input.remote_port,
        browser_url,
        auto_start_on_boot: input.auto_start_on_boot,
        desired_on: existing.map(|app| app.desired_on).unwrap_or(false),
        created_at: existing
            .map(|app| app.created_at)
            .unwrap_or_else(now_millis),
        updated_at: now_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        AppGroup, JumpInput, DEFAULT_CONNECT_TIMEOUT_SECONDS, DEFAULT_GROUP_ID,
        DEFAULT_KEEPALIVE_SECONDS,
    };

    fn tunnel_input() -> TunnelInput {
        TunnelInput {
            id: None,
            name: "NAS".into(),
            ssh_host: "example.com".into(),
            ssh_port: 22,
            ssh_user: "ops".into(),
            credentials: CredentialInput {
                password: Some("s3cret".into()),
                ..CredentialInput::default()
            },
            jumps: Vec::new(),
            auto_reconnect: true,
            keep_alive_seconds: DEFAULT_KEEPALIVE_SECONDS,
            connect_timeout_seconds: DEFAULT_CONNECT_TIMEOUT_SECONDS,
            strict_host_key: true,
        }
    }

    fn app_input(tunnel_id: &str) -> AppInput {
        AppInput {
            id: None,
            name: "Web".into(),
            group_id: DEFAULT_GROUP_ID.into(),
            tunnel_id: tunnel_id.into(),
            local_port: 18080,
            remote_host: "127.0.0.1".into(),
            remote_port: 80,
            browser_url: String::new(),
            auto_start_on_boot: false,
        }
    }

    fn snapshot(tunnel: &TunnelConfig) -> DockSnapshot {
        DockSnapshot {
            tunnels: vec![tunnel.clone()],
            apps: Vec::new(),
            groups: vec![AppGroup::default_group()],
            ..DockSnapshot::default()
        }
    }

    fn jump(index: usize) -> JumpInput {
        JumpInput {
            id: None,
            label: format!("jump-{index}"),
            host: "bastion".into(),
            port: 22,
            username: "ops".into(),
            credentials: CredentialInput {
                password: Some("s3cret".into()),
                ..CredentialInput::default()
            },
        }
    }

    #[tokio::test]
    async fn accepts_valid_tunnel_and_encrypts_secret() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let config = build_tunnel(tunnel_input(), None, &vault).unwrap();
        assert_eq!(config.name, "NAS");
        let cipher = config.credentials.password_cipher.unwrap();
        assert_eq!(vault.decrypt(&cipher).unwrap(), "s3cret");
    }

    #[tokio::test]
    async fn rejects_more_than_eight_jumps() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let mut candidate = tunnel_input();
        candidate.jumps = (0..9).map(jump).collect();
        let error = build_tunnel(candidate, None, &vault).unwrap_err();
        assert!(error.contains("8 级"), "unexpected: {error}");
    }

    #[tokio::test]
    async fn requires_password_when_none_is_stored() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let mut candidate = tunnel_input();
        candidate.credentials.password = None;
        let error = build_tunnel(candidate, None, &vault).unwrap_err();
        assert!(error.contains("密码"), "unexpected: {error}");
    }

    #[tokio::test]
    async fn keeps_stored_secret_when_input_is_null() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let existing = build_tunnel(tunnel_input(), None, &vault).unwrap();

        let mut edit = tunnel_input();
        edit.id = Some(existing.id.clone());
        edit.name = "NAS 2".into();
        edit.credentials.password = None; // null keeps the stored value
        let updated = build_tunnel(edit, Some(&existing), &vault).unwrap();
        assert_eq!(updated.name, "NAS 2");
        assert_eq!(
            updated.credentials.password_cipher,
            existing.credentials.password_cipher
        );
    }

    #[tokio::test]
    async fn switching_auth_type_and_clearing_password_drops_the_cipher() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let existing = build_tunnel(tunnel_input(), None, &vault).unwrap();

        let mut edit = tunnel_input();
        edit.id = Some(existing.id.clone());
        edit.credentials = CredentialInput {
            auth_type: AuthType::PrivateKey,
            password: Some(String::new()),
            private_key: Some("-----BEGIN OPENSSH PRIVATE KEY-----".into()),
            passphrase: None,
        };
        let updated = build_tunnel(edit, Some(&existing), &vault).unwrap();
        assert_eq!(updated.credentials.auth_type, AuthType::PrivateKey);
        assert!(updated.credentials.password_cipher.is_none());
        assert!(updated.credentials.private_key_cipher.is_some());
    }

    #[tokio::test]
    async fn rejects_out_of_range_tuning_values() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();

        let mut short_keepalive = tunnel_input();
        short_keepalive.keep_alive_seconds = 4;
        assert!(build_tunnel(short_keepalive, None, &vault).is_err());

        let mut long_timeout = tunnel_input();
        long_timeout.connect_timeout_seconds = 121;
        assert!(build_tunnel(long_timeout, None, &vault).is_err());
    }

    #[tokio::test]
    async fn app_requires_an_existing_group_and_tunnel() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let tunnel = build_tunnel(tunnel_input(), None, &vault).unwrap();
        let snapshot = snapshot(&tunnel);

        assert!(build_app(app_input(&tunnel.id), None, &snapshot).is_ok());

        let mut missing_tunnel = app_input("gone");
        assert!(build_app(missing_tunnel.clone(), None, &snapshot)
            .unwrap_err()
            .contains("隧道"));
        missing_tunnel.tunnel_id = tunnel.id.clone();
        missing_tunnel.group_id = "gone".into();
        assert!(build_app(missing_tunnel, None, &snapshot)
            .unwrap_err()
            .contains("应用组"));
    }

    #[tokio::test]
    async fn app_validates_ports_and_browser_url() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let tunnel = build_tunnel(tunnel_input(), None, &vault).unwrap();
        let snapshot = snapshot(&tunnel);

        let mut low_port = app_input(&tunnel.id);
        low_port.local_port = 80;
        assert!(build_app(low_port, None, &snapshot).is_err());

        let mut no_remote = app_input(&tunnel.id);
        no_remote.remote_host = "  ".into();
        assert!(build_app(no_remote, None, &snapshot).is_err());

        let mut bad_url = app_input(&tunnel.id);
        bad_url.browser_url = "ftp://127.0.0.1".into();
        assert!(build_app(bad_url, None, &snapshot).is_err());
    }

    #[tokio::test]
    async fn app_keeps_desired_and_created_at_on_edit() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let tunnel = build_tunnel(tunnel_input(), None, &vault).unwrap();
        let snapshot = snapshot(&tunnel);
        let existing = ForwardApp {
            name: "Web".into(),
            tunnel_id: tunnel.id.clone(),
            group_id: DEFAULT_GROUP_ID.into(),
            remote_host: "127.0.0.1".into(),
            local_port: 18080,
            desired_on: true,
            created_at: 42,
            ..ForwardApp::default()
        };

        let mut edit = app_input(&tunnel.id);
        edit.id = Some(existing.id.clone());
        edit.name = "Web 2".into();
        let updated = build_app(edit, Some(&existing), &snapshot).unwrap();
        assert_eq!(updated.name, "Web 2");
        assert!(updated.desired_on);
        assert_eq!(updated.created_at, 42);
    }
}
