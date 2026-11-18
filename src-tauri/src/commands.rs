use crate::model::{
    new_id, now_millis, AuthType, CredentialInput, JumpHost, JumpInput, SshCredentials,
    TunnelConfig, TunnelInput, TunnelStatus, TunnelView, DEFAULT_CONNECT_TIMEOUT_SECONDS,
    DEFAULT_KEEPALIVE_SECONDS, LOOPBACK,
};
use crate::secrets::Secrets;
use crate::AppState;
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn list_tunnels(state: State<'_, AppState>) -> Result<Vec<TunnelView>, String> {
    let configs = state.store.snapshot().await;
    Ok(configs.iter().map(TunnelView::from).collect())
}

#[tauri::command]
pub async fn list_statuses(state: State<'_, AppState>) -> Result<Vec<TunnelStatus>, String> {
    Ok(state.runtime.statuses_snapshot().await)
}

#[tauri::command]
pub async fn save_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    input: TunnelInput,
) -> Result<TunnelView, String> {
    let existing = match input.id.as_deref() {
        Some(id) => state.store.get(id).await,
        None => None,
    };
    let config = build_config(input, existing.as_ref(), &state.secrets)?;
    if let Some(other) = state.store.snapshot().await.iter().find(|other| {
        other.id != config.id
            && other.local_host == config.local_host
            && other.local_port == config.local_port
    }) {
        return Err(format!("本地端口与“{}”冲突", other.name));
    }
    let restart = existing.as_ref().map(|old| old.desired_on).unwrap_or(false)
        || state.runtime.is_live(&config.id).await;
    let saved = state
        .store
        .upsert(config)
        .await
        .map_err(|error| error.to_string())?;
    if restart {
        state
            .runtime
            .reconnect(&saved.id)
            .await
            .map_err(|error| error)?;
    }
    crate::sync_autostart(&app).await;
    Ok(TunnelView::from(&saved))
}

#[tauri::command]
pub async fn delete_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.runtime.delete(&id).await;
    crate::sync_autostart(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn connect_tunnel(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.runtime.clone().connect(&id).await
}

#[tauri::command]
pub async fn connect_all(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.clone().connect_all().await
}

#[tauri::command]
pub async fn disconnect_tunnel(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.runtime.disconnect(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn disconnect_all(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.disconnect_all().await;
    Ok(())
}

#[tauri::command]
pub async fn reconnect_tunnel(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.runtime.clone().reconnect(&id).await
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
    let valid = trimmed.starts_with("http://") || trimmed.starts_with("https://");
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

fn build_config(
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
    if input.local_port < 1024 {
        return Err("本地端口需在 1024–65535 之间".into());
    }
    if input.remote_host.trim().is_empty() || input.remote_host.contains(char::is_whitespace) {
        return Err("请填写远程主机".into());
    }
    if input.remote_port == 0 {
        return Err("远程端口需在 1–65535 之间".into());
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
    let browser_url = input.browser_url.trim().to_string();
    if !browser_url.is_empty()
        && !browser_url.starts_with("http://")
        && !browser_url.starts_with("https://")
    {
        return Err("地址需以 http:// 或 https:// 开头".into());
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
        id: id.clone(),
        name: name.to_string(),
        local_host: if input.local_host.trim().is_empty() {
            LOOPBACK.to_string()
        } else {
            input.local_host.trim().to_string()
        },
        local_port: input.local_port,
        remote_host: input.remote_host.trim().to_string(),
        remote_port: input.remote_port,
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
        auto_start_on_boot: input.auto_start_on_boot,
        keep_alive_seconds: input.keep_alive_seconds,
        connect_timeout_seconds: input.connect_timeout_seconds,
        strict_host_key: input.strict_host_key,
        browser_url,
        desired_on: existing.map(|config| config.desired_on).unwrap_or(false),
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
    if config.keep_alive_seconds == 0 {
        return Err(format!(
            "保活间隔需在 5–300 秒之间（默认 {DEFAULT_KEEPALIVE_SECONDS}）"
        ));
    }
    if config.connect_timeout_seconds == 0 {
        return Err(format!(
            "连接超时需在 5–120 秒之间（默认 {DEFAULT_CONNECT_TIMEOUT_SECONDS}）"
        ));
    }
    Ok(config)
}
