use crate::local_ssh::{LocalSshStatus, SshConfigHost};
use crate::model::{
    new_id, now_millis, AppGroup, AppInput, AppStatus, AppView, AuthType, CredentialInput,
    DockSnapshot, ForwardApp, GroupInput, GroupView, JumpHost, JumpInput, SshCredentials,
    TunnelConfig, TunnelInput, TunnelTestResult, TunnelView, DEFAULT_GROUP_ID,
};
use crate::portable::{self, BundlePreview, ExportSummary, ImportSummary};
use crate::secrets::Secrets;
use crate::AppState;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// 一个应用最多引用多少条隧道。再多说明入口规划已经失控，应当拆成多个应用。
const MAX_APP_TUNNELS: usize = 8;
/// 单条隧道最多支持多少级跳板。
const MAX_JUMPS: usize = 8;

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
        tunnels: snapshot
            .tunnels
            .iter()
            .map(|tunnel| TunnelView::resolved(tunnel, &snapshot))
            .collect(),
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
    // 跳板机可以“从现有隧道选择”，因此保存时要能看到全部隧道。
    let snapshot = state.store.snapshot().await;
    let config = build_tunnel(
        input,
        existing.as_ref(),
        &snapshot,
        &state.secrets,
        &state.local_ssh,
    )?;
    let saved = state.runtime.clone().save_tunnel(config).await?;
    crate::sync_autostart(&app).await;
    let snapshot = state.store.snapshot().await;
    Ok(TunnelView::resolved(&saved, &snapshot))
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

// ---------------------------------------------------------------- 本机 ~/.ssh

/// 探测 `~/.ssh`：目录是否存在、有哪些私钥、`config` 里有多少可导入的主机。
#[tauri::command]
pub async fn local_ssh_status(state: State<'_, AppState>) -> Result<LocalSshStatus, String> {
    let local = state.local_ssh.clone();
    // 目录扫描是同步文件 IO，放到阻塞线程池，避免卡住 Tauri 的命令执行器。
    tokio::task::spawn_blocking(move || local.scan())
        .await
        .map_err(|error| format!("读取 ~/.ssh 失败: {error}"))
}

/// 列出 `~/.ssh/config` 中带具体 Host 名的条目。
#[tauri::command]
pub async fn local_ssh_config_hosts(
    state: State<'_, AppState>,
) -> Result<Vec<SshConfigHost>, String> {
    let local = state.local_ssh.clone();
    tokio::task::spawn_blocking(move || local.parse_config())
        .await
        .map_err(|error| format!("解析 ~/.ssh/config 失败: {error}"))?
}

/// 把选中的 `Host` 别名导入为隧道。
///
/// 只创建尚不存在的别名，已导入过的别名会被跳过，因此重复点击是安全的。
/// 私钥：`IdentityFile` 指向 `~/.ssh` 内且存在的文件时自动读取；否则留空，
/// 需要用户在隧道编辑页补充。
#[tauri::command]
pub async fn import_local_ssh_config(
    app: AppHandle,
    state: State<'_, AppState>,
    aliases: Vec<String>,
) -> Result<Vec<TunnelView>, String> {
    if aliases.is_empty() {
        return Err("请先选择要导入的主机".into());
    }
    let local = state.local_ssh.clone();
    let hosts = tokio::task::spawn_blocking(move || local.parse_config())
        .await
        .map_err(|error| format!("解析 ~/.ssh/config 失败: {error}"))??;

    let snapshot = state.store.snapshot().await;
    let mut existing_names: HashSet<String> = snapshot
        .tunnels
        .iter()
        .map(|tunnel| tunnel.name.to_lowercase())
        .collect();
    let mut created = Vec::new();

    for alias in aliases {
        let Some(host) = hosts.iter().find(|item| item.alias == alias) else {
            continue;
        };
        if !host.importable {
            return Err(format!(
                "主机“{}”包含暂不支持的配置（{}），请手动创建隧道",
                host.alias,
                host.unsupported.as_deref().unwrap_or("unsupported")
            ));
        }
        if existing_names.contains(&host.alias.to_lowercase()) {
            continue;
        }
        let credentials = local_ssh_credentials(host, &state)?;
        let jumps = build_config_jumps(host, &hosts, &state)?;
        // 导入的每个 Host 别名各建一条隧道，就是一次确定的 SSH 接入。
        // 需要多个入口时，在应用里按顺序引用多条隧道。
        let config = TunnelConfig {
            id: new_id(),
            name: host.alias.clone(),
            host: host.host_name.clone(),
            port: host.port,
            username: host.user.clone(),
            credentials,
            jumps,
            auto_reconnect: true,
            keep_alive_seconds: crate::model::DEFAULT_KEEPALIVE_SECONDS,
            connect_timeout_seconds: crate::model::DEFAULT_CONNECT_TIMEOUT_SECONDS,
            strict_host_key: true,
            created_at: now_millis(),
            updated_at: now_millis(),
        };
        let saved = state.runtime.clone().save_tunnel(config).await?;
        existing_names.insert(host.alias.to_lowercase());
        created.push(saved);
    }

    crate::sync_autostart(&app).await;
    let snapshot = state.store.snapshot().await;
    Ok(created
        .iter()
        .map(|tunnel| TunnelView::resolved(tunnel, &snapshot))
        .collect())
}

fn local_ssh_credentials(
    host: &SshConfigHost,
    state: &State<'_, AppState>,
) -> Result<SshCredentials, String> {
    let local = state.local_ssh.clone();
    let key = host
        .identity_file
        .as_deref()
        .map(|path| resolve_identity_file(&local, path))
        .transpose()?
        .flatten();
    match key {
        Some(text) => Ok(SshCredentials {
            auth_type: AuthType::PrivateKey,
            private_key_cipher: Some(
                state
                    .secrets
                    .encrypt(&text)
                    .map_err(|error| error.to_string())?,
            ),
            ..SshCredentials::default()
        }),
        // 没有可用私钥：留空凭据，用户可在隧道编辑页补充。
        None => Ok(SshCredentials::default()),
    }
}

/// 把 `IdentityFile` 解析成 `~/.ssh` 下的真实文件内容。
/// 相对路径按 `~/.ssh` 解析；`%d` 替换为 home 目录；指向目录外或不存在时返回 `None`。
fn resolve_identity_file(
    local: &crate::local_ssh::LocalSsh,
    raw: &str,
) -> Result<Option<String>, String> {
    let expanded = raw.trim().replace("%d", &home_dir().to_string_lossy());
    let candidate = if expanded.starts_with("~/") || expanded.starts_with("~\\") {
        home_dir().join(&expanded[2..])
    } else if expanded.starts_with('~') {
        home_dir().join(&expanded[1..])
    } else {
        let path = std::path::PathBuf::from(&expanded);
        if path.is_absolute() {
            path
        } else {
            local.root().join(path)
        }
    };
    let canonical = match candidate.canonicalize() {
        Ok(path) => path,
        Err(_) => return Ok(None),
    };
    let Ok(root) = local.root().canonicalize() else {
        return Ok(None);
    };
    if !canonical.starts_with(&root) {
        return Ok(None);
    }
    let Some(name) = canonical.file_name().map(|name| name.to_string_lossy().to_string()) else {
        return Ok(None);
    };
    match local.read_key(&name) {
        Ok(text) => Ok(Some(text)),
        Err(_) => Ok(None),
    }
}

fn home_dir() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}

/// `ProxyJump` 里的别名按链路顺序转成跳板机，凭据沿用各自的 `IdentityFile`。
fn build_config_jumps(
    host: &SshConfigHost,
    hosts: &[SshConfigHost],
    state: &State<'_, AppState>,
) -> Result<Vec<JumpHost>, String> {
    let mut jumps = Vec::with_capacity(host.proxy_jump.len());
    for alias in &host.proxy_jump {
        let Some(jump) = hosts.iter().find(|item| item.alias == *alias) else {
            return Err(format!("跳板机“{alias}”没有在 ~/.ssh/config 中定义"));
        };
        jumps.push(JumpHost {
            id: new_id(),
            label: jump.alias.clone(),
            host: jump.host_name.clone(),
            port: jump.port,
            username: jump.user.clone(),
            credentials: local_ssh_credentials(jump, state)?,
        });
    }
    Ok(jumps)
}

// ------------------------------------------------------------ 配置包导出导入

/// 导出应用组、应用与隧道到 `.zip`。
#[tauri::command]
pub async fn export_bundle(
    state: State<'_, AppState>,
    path: String,
) -> Result<ExportSummary, String> {
    let target = check_export_path(&path)?;
    let snapshot = state.store.snapshot().await;
    let payload = portable::build_document(&snapshot, &state.secrets)?;
    let document = payload.document.clone();
    tokio::task::spawn_blocking(move || portable::write_zip(&target, &document))
        .await
        .map_err(|error| format!("导出失败: {error}"))??;
    tracing::info!(
        tunnels = payload.document.tunnels.len(),
        apps = payload.document.apps.len(),
        groups = payload.document.groups.len(),
        "已导出配置包"
    );
    Ok(payload.summary(&path))
}

/// 只读取列表，不写入配置，供导入前的确认弹窗使用。
#[tauri::command]
pub async fn preview_bundle(path: String) -> Result<BundlePreview, String> {
    let source = check_import_path(&path)?;
    let document = tokio::task::spawn_blocking(move || portable::read_zip(&source))
        .await
        .map_err(|error| format!("读取配置包失败: {error}"))??;
    Ok(portable::preview(&document))
}

/// 把配置包合并进当前配置。不会自动启动转发。
#[tauri::command]
pub async fn import_bundle(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<ImportSummary, String> {
    let source = check_import_path(&path)?;
    let document = tokio::task::spawn_blocking(move || portable::read_zip(&source))
        .await
        .map_err(|error| format!("读取配置包失败: {error}"))??;
    let summary = portable::apply_import(document, &state.store, &state.secrets).await?;
    crate::sync_autostart(&app).await;
    tracing::info!(
        tunnels = summary.tunnels_added,
        apps = summary.apps_added,
        groups = summary.groups_added,
        "已导入配置包"
    );
    Ok(summary)
}

fn checked_path(path: &str, extension: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("请选择文件路径".into());
    }
    let path = std::path::PathBuf::from(trimmed);
    match path.extension().and_then(|value| value.to_str()) {
        Some(value) if value.eq_ignore_ascii_case(extension) => Ok(path),
        _ => Err(format!("请选择 .{extension} 文件")),
    }
}

fn check_export_path(path: &str) -> Result<std::path::PathBuf, String> {
    checked_path(path, "zip")
}

fn check_import_path(path: &str) -> Result<std::path::PathBuf, String> {
    let path = checked_path(path, "zip")?;
    if !path.is_file() {
        return Err("所选的配置包不存在".into());
    }
    Ok(path)
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

/// `~/.ssh` 下的私钥走“文件名引用”而不是“内容上传”：后端读取文件、加密落盘，
/// 前端既不读也不缓存密钥内容。文件缺失或不是私钥时回落到 `None`，
/// 由上层语义决定是保持原值还是报“缺少私钥”。
fn local_key_secret(
    file: &str,
    local_ssh: &crate::local_ssh::LocalSsh,
    secrets: &Arc<Secrets>,
) -> Result<Option<String>, String> {
    let text = local_ssh.read_key(file.trim())?;
    secrets
        .encrypt(&text)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn merge_credentials(
    existing: &SshCredentials,
    input: &CredentialInput,
    secrets: &Arc<Secrets>,
    local_ssh: &crate::local_ssh::LocalSsh,
) -> Result<SshCredentials, String> {
    // 只有私钥认证才读取本机文件；切回密码认证时保留原私钥即可。
    let use_local_key = input.uses_local_key() && input.auth_type == AuthType::PrivateKey;
    let private_key_cipher = if use_local_key {
        local_key_secret(
            input.private_key_path.as_deref().unwrap_or_default(),
            local_ssh,
            secrets,
        )?
    } else {
        merge_secret(
            existing.private_key_cipher.as_ref(),
            input.private_key.as_ref(),
            secrets,
        )?
    };
    Ok(SshCredentials {
        auth_type: input.auth_type,
        password_cipher: merge_secret(
            existing.password_cipher.as_ref(),
            input.password.as_ref(),
            secrets,
        )?,
        private_key_cipher,
        passphrase_cipher: merge_secret(
            existing.passphrase_cipher.as_ref(),
            input.passphrase.as_ref(),
            secrets,
        )?,
    })
}

/// 跳板机落盘。
///
/// 两条互不影响的来源：
/// - 手动填写的跳板机按 `credentials` 合并（保持原有的“留空即沿用旧值”语义）；
/// - 从现有隧道“选”来的跳板机按 `credential_from` 复制源隧道的凭据密文，
///   落盘后自包含，源隧道被删除也不会断开已有的跳板链。
fn build_jumps(
    input: &[JumpInput],
    existing: &[JumpHost],
    snapshot: &DockSnapshot,
    secrets: &Arc<Secrets>,
    local_ssh: &crate::local_ssh::LocalSsh,
) -> Result<Vec<JumpHost>, String> {
    if input.len() > MAX_JUMPS {
        return Err(format!("最多支持 {MAX_JUMPS} 级跳板机"));
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
        let mut host = JumpHost {
            id,
            label: jump.label.trim().to_string(),
            host: jump.host.trim().to_string(),
            port: jump.port,
            username: jump.username.trim().to_string(),
            credentials: match previous {
                Some(previous) => {
                    merge_credentials(&previous.credentials, &jump.credentials, secrets, local_ssh)?
                }
                None => merge_credentials(
                    &SshCredentials::default(),
                    &jump.credentials,
                    secrets,
                    local_ssh,
                )?,
            },
        };
        // “从现有隧道选择”的跳板机：把源隧道的凭据密文复制过来（覆盖上面的合并结果）。
        if let Some(source) = &jump.credential_from {
            snapshot.apply_jump_credential_source(&mut host, source)?;
        }
        jumps.push(host);
    }
    Ok(jumps)
}

fn build_tunnel(
    input: TunnelInput,
    existing: Option<&TunnelConfig>,
    snapshot: &DockSnapshot,
    secrets: &Arc<Secrets>,
    local_ssh: &crate::local_ssh::LocalSsh,
) -> Result<TunnelConfig, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("请填写隧道名称".into());
    }
    if name.chars().count() > 40 {
        return Err("名称不能超过 40 个字符".into());
    }
    if input.keep_alive_seconds < 5 || input.keep_alive_seconds > 300 {
        return Err("保活间隔需在 5–300 秒之间".into());
    }
    if input.connect_timeout_seconds < 5 || input.connect_timeout_seconds > 120 {
        return Err("连接超时需在 5–120 秒之间".into());
    }

    if input.host.trim().is_empty() || input.host.contains(char::is_whitespace) {
        return Err("请填写 SSH 主机地址".into());
    }
    if input.port == 0 {
        return Err("SSH 端口需在 1–65535 之间".into());
    }
    if input.username.trim().is_empty() {
        return Err("请填写 SSH 用户名".into());
    }

    let id = input
        .id
        .clone()
        .or_else(|| existing.map(|config| config.id.clone()))
        .unwrap_or_else(new_id);

    let credentials = merge_credentials(
        &existing
            .map(|config| config.credentials.clone())
            .unwrap_or_default(),
        &input.credentials,
        secrets,
        local_ssh,
    )?;
    match input.credentials.auth_type {
        AuthType::Password if credentials.password_cipher.is_none() => {
            return Err(format!("隧道“{name}”还没有登录密码"));
        }
        AuthType::PrivateKey if credentials.private_key_cipher.is_none() => {
            return Err(format!("隧道“{name}”还没有私钥"));
        }
        _ => {}
    }
    let previous_jumps = existing
        .map(|config| config.jumps.as_slice())
        .unwrap_or(&[]);
    let jumps = build_jumps(&input.jumps, previous_jumps, snapshot, secrets, local_ssh)?;

    let config = TunnelConfig {
        id,
        name: name.to_string(),
        host: input.host.trim().to_string(),
        port: input.port,
        username: input.username.trim().to_string(),
        credentials,
        jumps,
        auto_reconnect: input.auto_reconnect,
        keep_alive_seconds: input.keep_alive_seconds,
        connect_timeout_seconds: input.connect_timeout_seconds,
        strict_host_key: input.strict_host_key,
        created_at: existing
            .map(|config| config.created_at)
            .unwrap_or_else(now_millis),
        updated_at: now_millis(),
    };
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
    // 隧道列表按尝试顺序保存，去重但保留顺序；第一条是首选。
    let mut tunnel_ids: Vec<String> = Vec::with_capacity(input.tunnel_ids.len());
    for tunnel_id in &input.tunnel_ids {
        let id = tunnel_id.trim();
        if id.is_empty() || tunnel_ids.iter().any(|existing| existing == id) {
            continue;
        }
        if snapshot.tunnel(id).is_none() {
            return Err(format!(
                "所选隧道不存在（{id}），请重新选择可用隧道"
            ));
        }
        tunnel_ids.push(id.to_string());
    }
    if tunnel_ids.is_empty() {
        return Err("请至少选择一条隧道".into());
    }
    if tunnel_ids.len() > MAX_APP_TUNNELS {
        return Err(format!("一个应用最多选择 {MAX_APP_TUNNELS} 条隧道"));
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
        tunnel_ids,
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

    fn tunnel_input(name: &str, host: &str) -> TunnelInput {
        TunnelInput {
            id: None,
            name: name.into(),
            host: host.into(),
            port: 22,
            username: "ops".into(),
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
            tunnel_ids: vec![tunnel_id.into()],
            local_port: 18080,
            remote_host: "127.0.0.1".into(),
            remote_port: 80,
            browser_url: String::new(),
            auto_start_on_boot: false,
        }
    }

    fn snapshot(tunnels: &[TunnelConfig]) -> DockSnapshot {
        DockSnapshot {
            tunnels: tunnels.to_vec(),
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
            credential_from: None,
        }
    }

    /// 大多数测试只关心隧道本身，给一个空配置文档即可。
    fn build(
        input: TunnelInput,
        existing: Option<&TunnelConfig>,
        vault: &Arc<Secrets>,
        local: &crate::local_ssh::LocalSsh,
    ) -> Result<TunnelConfig, String> {
        build_tunnel(input, existing, &DockSnapshot::default(), vault, local)
    }

    /// 需要“从现有隧道选跳板”这种跨隧道解析时用它。
    fn build_in(
        snapshot: &DockSnapshot,
        input: TunnelInput,
        existing: Option<&TunnelConfig>,
        vault: &Arc<Secrets>,
        local: &crate::local_ssh::LocalSsh,
    ) -> Result<TunnelConfig, String> {
        build_tunnel(input, existing, snapshot, vault, local)
    }

    /// 测试不依赖真实的 `~/.ssh`，因此始终指向一个空的临时目录。
    fn local_ssh() -> crate::local_ssh::LocalSsh {
        let dir = std::env::temp_dir().join("tunneldock-test-ssh");
        let _ = std::fs::create_dir_all(&dir);
        crate::local_ssh::LocalSsh::at(dir)
    }

    #[tokio::test]
    async fn accepts_valid_tunnel_and_encrypts_secret() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let config = build(
            tunnel_input("NAS", "example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();
        assert_eq!(config.name, "NAS");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 22);
        assert_eq!(config.username, "ops");
        let cipher = config.credentials.password_cipher.clone().unwrap();
        assert_eq!(vault.decrypt(&cipher).unwrap(), "s3cret");
    }

    #[tokio::test]
    async fn rejects_more_than_eight_jumps() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let mut candidate = tunnel_input("NAS", "example.com");
        candidate.jumps = (0..9).map(jump).collect();
        let error = build(candidate, None, &vault, &local_ssh()).unwrap_err();
        assert!(error.contains("8 级"), "unexpected: {error}");
    }

    /// 一条隧道就是一次确定的接入：地址 / 端口 / 用户都写在自己身上。
    #[tokio::test]
    async fn a_tunnel_carries_a_single_complete_ssh_access() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let mut input = tunnel_input("外网入口", "frp.example.com");
        input.port = 2222;
        let config = build(input, None, &vault, &local_ssh()).unwrap();
        assert_eq!(config.host, "frp.example.com");
        assert_eq!(config.port, 2222);
        assert_eq!(config.endpoint(), "ops@frp.example.com:2222");
        assert!(config.is_dialable());
    }

    /// 同一个服务要两个入口时，是**两个应用中的两条隧道**，而不是一条隧道两条线路。
    #[tokio::test]
    async fn two_entrances_are_two_tunnels_and_an_app_keeps_their_order() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let home = build(
            tunnel_input("家里", "192.168.1.10"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();
        let away = build(
            tunnel_input("外网", "frp.example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();
        let document = snapshot(&[home.clone(), away.clone()]);

        // 外网优先、家里兜底。
        let mut input = app_input(&away.id);
        input.tunnel_ids = vec![away.id.clone(), home.id.clone()];
        let saved = build_app(input, None, &document).unwrap();
        assert_eq!(saved.tunnel_ids, vec![away.id.clone(), home.id.clone()]);
        assert_eq!(saved.primary_tunnel_id(), Some(away.id.as_str()));
    }

    #[tokio::test]
    async fn an_app_needs_at_least_one_tunnel() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let tunnel = build(
            tunnel_input("NAS", "example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();
        let document = snapshot(&[tunnel]);
        let mut input = app_input("ignored");
        input.tunnel_ids = Vec::new();
        let error = build_app(input, None, &document).unwrap_err();
        assert!(error.contains("隧道"), "unexpected: {error}");
    }

    /// 重复选择同一条隧道只保留一次，顺序不变。
    #[tokio::test]
    async fn duplicate_tunnel_choices_collapse_but_keep_the_order() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let a = build(tunnel_input("甲", "a.example.com"), None, &vault, &local_ssh())
            .unwrap();
        let b = build(tunnel_input("乙", "b.example.com"), None, &vault, &local_ssh())
            .unwrap();
        let document = snapshot(&[a.clone(), b.clone()]);
        let mut input = app_input(&a.id);
        input.tunnel_ids = vec![b.id.clone(), a.id.clone(), b.id.clone()];
        let saved = build_app(input, None, &document).unwrap();
        assert_eq!(saved.tunnel_ids, vec![b.id, a.id]);
    }

    #[tokio::test]
    async fn an_app_rejects_more_tunnels_than_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let mut tunnels = Vec::new();
        let mut ids = Vec::new();
        for index in 0..=MAX_APP_TUNNELS {
            let saved = build(
                tunnel_input(&format!("入口 {index}"), &format!("h{index}.example.com")),
                None,
                &vault,
                &local_ssh(),
            )
            .unwrap();
            ids.push(saved.id.clone());
            tunnels.push(saved);
        }
        let document = snapshot(&tunnels);
        let mut input = app_input(&ids[0]);
        input.tunnel_ids = ids;
        let error = build_app(input, None, &document).unwrap_err();
        assert!(error.contains("最多"), "unexpected: {error}");
    }

    #[tokio::test]
    async fn a_missing_tunnel_reference_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let tunnel = build(
            tunnel_input("NAS", "example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();
        let document = snapshot(&[tunnel.clone()]);
        let mut input = app_input(&tunnel.id);
        input.tunnel_ids = vec![tunnel.id.clone(), "gone".into()];
        assert!(build_app(input, None, &document).is_err());
    }

    #[tokio::test]
    async fn a_tunnel_needs_address_user_and_secret() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();

        let mut no_host = tunnel_input("NAS", "  ");
        no_host.host = "  ".into();
        assert!(build(no_host, None, &vault, &local_ssh()).is_err());

        let mut no_user = tunnel_input("NAS", "example.com");
        no_user.username = " ".into();
        assert!(build(no_user, None, &vault, &local_ssh()).is_err());

        let mut bad_port = tunnel_input("NAS", "example.com");
        bad_port.port = 0;
        assert!(build(bad_port, None, &vault, &local_ssh()).is_err());

        let mut no_secret = tunnel_input("NAS", "example.com");
        no_secret.credentials.password = None;
        let error = build(no_secret, None, &vault, &local_ssh()).unwrap_err();
        assert!(error.contains("密码"), "unexpected: {error}");
    }

    /// 编辑时隧道 id 必须保住，否则跳板机的密码与“上次成功的隧道”都会失效。
    #[tokio::test]
    async fn editing_keeps_the_tunnel_id_and_stored_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let existing = build(
            tunnel_input("NAS", "example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();

        let mut edit = tunnel_input("NAS", "example.com");
        edit.id = Some(existing.id.clone());
        edit.credentials.password = None; // null keeps the stored value
        let updated = build(edit, Some(&existing), &vault, &local_ssh()).unwrap();
        assert_eq!(updated.id, existing.id);
        assert_eq!(
            updated.credentials.password_cipher,
            existing.credentials.password_cipher
        );
    }

    #[tokio::test]
    async fn keeps_stored_secret_when_input_is_null() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let existing = build(
            tunnel_input("NAS", "example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();

        let mut edit = tunnel_input("NAS 2", "example.com");
        edit.id = Some(existing.id.clone());
        edit.credentials.password = None; // null keeps the stored value
        let updated = build(edit, Some(&existing), &vault, &local_ssh()).unwrap();
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
        let existing = build(
            tunnel_input("NAS", "example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();

        let mut edit = tunnel_input("NAS", "example.com");
        edit.id = Some(existing.id.clone());
        edit.credentials = CredentialInput {
            auth_type: AuthType::PrivateKey,
            password: Some(String::new()),
            private_key: Some("-----BEGIN OPENSSH PRIVATE KEY-----".into()),
            passphrase: None,
            private_key_path: None,
        };
        let updated = build(edit, Some(&existing), &vault, &local_ssh()).unwrap();
        assert_eq!(updated.credentials.auth_type, AuthType::PrivateKey);
        assert!(updated.credentials.password_cipher.is_none());
        assert!(updated.credentials.private_key_cipher.is_some());
    }

    #[tokio::test]
    async fn rejects_out_of_range_tuning_values() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();

        let mut short_keepalive = tunnel_input("NAS", "example.com");
        short_keepalive.keep_alive_seconds = 4;
        assert!(build(short_keepalive, None, &vault, &local_ssh()).is_err());

        let mut long_timeout = tunnel_input("NAS", "example.com");
        long_timeout.connect_timeout_seconds = 121;
        assert!(build(long_timeout, None, &vault, &local_ssh()).is_err());
    }

    #[tokio::test]
    async fn rejects_names_longer_than_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let error = build(
            tunnel_input(&"名".repeat(41), "example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap_err();
        assert!(error.contains("40"), "unexpected: {error}");
    }

    #[tokio::test]
    async fn app_requires_an_existing_group_and_tunnel() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let tunnel = build(
            tunnel_input("NAS", "example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();
        let document = snapshot(&[tunnel.clone()]);

        assert!(build_app(app_input(&tunnel.id), None, &document).is_ok());

        let mut missing_tunnel = app_input("gone");
        assert!(build_app(missing_tunnel.clone(), None, &document)
            .unwrap_err()
            .contains("隧道"));
        missing_tunnel.tunnel_ids = vec![tunnel.id.clone()];
        missing_tunnel.group_id = "gone".into();
        assert!(build_app(missing_tunnel, None, &document)
            .unwrap_err()
            .contains("应用组"));
    }

    #[tokio::test]
    async fn app_validates_ports_and_browser_url() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let tunnel = build(
            tunnel_input("NAS", "example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();
        let document = snapshot(&[tunnel.clone()]);

        let mut low_port = app_input(&tunnel.id);
        low_port.local_port = 80;
        assert!(build_app(low_port, None, &document).is_err());

        let mut no_remote = app_input(&tunnel.id);
        no_remote.remote_host = "  ".into();
        assert!(build_app(no_remote, None, &document).is_err());

        let mut bad_url = app_input(&tunnel.id);
        bad_url.browser_url = "ftp://127.0.0.1".into();
        assert!(build_app(bad_url, None, &document).is_err());
    }

    #[tokio::test]
    async fn imports_private_key_from_local_ssh_directory() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let ssh_dir = dir.path().join("ssh");
        std::fs::create_dir_all(&ssh_dir).unwrap();
        let key_body = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\n\
QyNTUxOQAAACBw5c7cJ4cJy0n6b2nHf5G6b2nHf5G6b2nHf5G6b2nHf5G6bQAAAJg=\n\
-----END OPENSSH PRIVATE KEY-----\n";
        std::fs::write(ssh_dir.join("id_ed25519"), key_body).unwrap();
        let local = crate::local_ssh::LocalSsh::at(&ssh_dir);

        let mut input = tunnel_input("NAS", "example.com");
        input.credentials = CredentialInput {
            auth_type: AuthType::PrivateKey,
            private_key_path: Some("id_ed25519".into()),
            ..CredentialInput::default()
        };
        let config = build(input, None, &vault, &local).unwrap();
        let cipher = config.credentials.private_key_cipher.clone().unwrap();
        assert_eq!(vault.decrypt(&cipher).unwrap(), key_body);
        assert!(config.credentials.password_cipher.is_none());
    }

    #[tokio::test]
    async fn missing_local_key_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let mut input = tunnel_input("NAS", "example.com");
        input.credentials = CredentialInput {
            auth_type: AuthType::PrivateKey,
            private_key_path: Some("nope".into()),
            ..CredentialInput::default()
        };
        let error = build(input, None, &vault, &local_ssh()).unwrap_err();
        assert!(error.contains("私钥"), "unexpected: {error}");
    }

    #[test]
    fn export_path_requires_a_zip_extension() {
        assert!(check_export_path("").is_err());
        assert!(check_export_path("backup.json").is_err());
        assert!(check_export_path("backup.zip").is_ok());
        assert!(check_import_path("missing.zip").is_err());
    }

    /// 从现有隧道里选一台跳板机：保存时把源隧道的凭据密文复制过来，
    /// 落盘后跳板机自包含——删掉源隧道也不会把已有跳板链弄断。
    #[tokio::test]
    async fn a_jump_picked_from_a_tunnel_keeps_its_own_copy_of_the_secret() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let bastion = build(
            tunnel_input("堡垒机", "bastion.example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();
        let document = snapshot(&[bastion.clone()]);

        let mut input = tunnel_input("内网", "inner.example.com");
        input.jumps = vec![JumpInput {
            id: None,
            label: "堡垒机".into(),
            host: "bastion.example.com".into(),
            port: 22,
            username: "ops".into(),
            credentials: CredentialInput::default(),
            credential_from: Some(crate::model::JumpCredentialSource {
                tunnel_id: bastion.id.clone(),
                hop: None,
            }),
        }];
        let target = build_in(&document, input, None, &vault, &local_ssh()).unwrap();

        assert_eq!(target.jumps.len(), 1);
        let cipher = target.jumps[0]
            .credentials
            .password_cipher
            .clone()
            .expect("跳板机应复制到密码密文");
        assert_eq!(vault.decrypt(&cipher).unwrap(), "s3cret");
        // 密文复制而不是引用：源隧道从快照里消失后，跳板机仍然可用。
        let without_source = DockSnapshot {
            tunnels: vec![target.clone()],
            ..DockSnapshot::default()
        };
        assert!(without_source.with_inherited_jump_credentials(&target).jumps[0]
            .credentials
            .has_secret());
    }

    #[tokio::test]
    async fn picking_an_unknown_tunnel_or_a_hop_out_of_range_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let bastion = build(
            tunnel_input("堡垒机", "bastion.example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();
        let document = snapshot(&[bastion.clone()]);

        let with_source = |source: crate::model::JumpCredentialSource| {
            let mut input = tunnel_input("内网", "inner.example.com");
            input.jumps = vec![JumpInput {
                id: None,
                label: "堡垒机".into(),
                host: "bastion.example.com".into(),
                port: 22,
                username: "ops".into(),
                credentials: CredentialInput::default(),
                credential_from: Some(source),
            }];
            input
        };

        let missing = with_source(crate::model::JumpCredentialSource {
            tunnel_id: "gone".into(),
            hop: None,
        });
        let error = build_in(&document, missing, None, &vault, &local_ssh()).unwrap_err();
        assert!(error.contains("已被删除"), "unexpected: {error}");

        let bad_hop = with_source(crate::model::JumpCredentialSource {
            tunnel_id: bastion.id.clone(),
            hop: Some(3),
        });
        let error = build_in(&document, bad_hop, None, &vault, &local_ssh()).unwrap_err();
        assert!(error.contains("跳板链"), "unexpected: {error}");
    }

    /// 手动填写的路径必须保持原样：不传 `credentialFrom` 时不会去读别的隧道。
    #[tokio::test]
    async fn a_manual_jump_still_uses_its_own_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let mut input = tunnel_input("内网", "inner.example.com");
        input.jumps = vec![JumpInput {
            id: None,
            label: "手填堡垒机".into(),
            host: "manual.example.com".into(),
            port: 2222,
            username: "ops".into(),
            credentials: CredentialInput {
                password: Some("manual-secret".into()),
                ..CredentialInput::default()
            },
            credential_from: None,
        }];
        let config = build(
            input,
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();
        let cipher = config.jumps[0].credentials.password_cipher.clone().unwrap();
        assert_eq!(vault.decrypt(&cipher).unwrap(), "manual-secret");
    }

    #[tokio::test]
    async fn app_keeps_desired_and_created_at_on_edit() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Secrets::load(dir.path()).unwrap();
        let tunnel = build(
            tunnel_input("NAS", "example.com"),
            None,
            &vault,
            &local_ssh(),
        )
        .unwrap();
        let document = snapshot(&[tunnel.clone()]);
        let existing = ForwardApp {
            name: "Web".into(),
            tunnel_ids: vec![tunnel.id.clone()],
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
        let updated = build_app(edit, Some(&existing), &document).unwrap();
        assert_eq!(updated.name, "Web 2");
        assert!(updated.desired_on);
        assert_eq!(updated.created_at, 42);
    }
}
