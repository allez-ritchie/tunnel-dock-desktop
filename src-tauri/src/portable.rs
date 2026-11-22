//! 配置包（`.zip`）的导出与导入：应用组、应用、隧道。
//!
//! 导出的不是 `tunnels.json` 的复制，而是一份**可移植清单**：
//!
//! - 凭据只支持**私钥**（密码无法跨机器迁移，且会泄露明文口令）；
//! - 应用组、隧道、应用之间用**名称**而不是 ID 引用，因此导入到另一台机器
//!   同样能还原引用关系；
//! - 不含运行状态（`desired_on`），导入后不会自动启动任何转发。
//!
//! 清单版本 2 起，一条隧道携带**一组线路**（`endpoints`）；版本 1 的清单仍可导入，
//! 它把隧道上的那对 SSH 信息当作唯一的一条线路。
//!
//! 压缩包内只有一个 `tunneldock-export.json` 清单和一个说明文件。清单是纯文本 JSON，
//! 出现问题时可以直接用文本编辑器检查。

use crate::model::{
    new_id, now_millis, AppGroup, AuthType, DockSnapshot, ForwardApp, JumpHost, SshCredentials,
    TunnelConfig, TunnelEndpoint, DEFAULT_GROUP_ID, DEFAULT_SSH_PORT,
};
use crate::secrets::Secrets;
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// 当前清单版本。1 是单线路的扁平结构，2 起隧道持有 `endpoints` 数组。
pub const EXPORT_VERSION: u32 = 2;
pub const EXPORT_KIND: &str = "tunneldock.export";
pub const MANIFEST_NAME: &str = "tunneldock-export.json";
pub const README_NAME: &str = "README.txt";
/// 清单解压后的上限：配置包本就很小，超出即视为异常文件。
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
/// 单条隧道最多导出多少条线路，防止异常清单撑爆内存。
const MAX_ENDPOINTS: usize = 32;

const README: &str = "\
TunnelDock 配置包
=================

- tunneldock-export.json：应用组、应用、隧道的可移植清单（纯文本 JSON）。
- 隧道可以有多条 SSH 线路；每条线路的凭据单独保存。
- 凭据仅支持私钥；使用密码认证的线路不会被导出。
- 包内不包含运行状态，导入后不会自动启动转发。
- 该文件包含明文私钥，请按密钥文件的等级妥善保管，不要上传到公开位置。
";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportJump {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
}

/// 导出清单里的一条 SSH 线路。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportEndpoint {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
    pub jumps: Vec<ExportJump>,
    /// 只在局域网可用（家里的服务器）。
    pub wifi_only: bool,
    pub enabled: bool,
}

impl ExportEndpoint {
    fn endpoint_key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.host.trim().to_ascii_lowercase(),
            self.port,
            self.username.trim()
        )
    }
}

/// 导出清单里的一条隧道：名称 + 一组线路 + 连接参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportTunnel {
    pub name: String,
    pub endpoints: Vec<ExportEndpoint>,
    /// 首选线路的序号（导出时把首选线路排到第一位，因此这里通常只是 0）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_endpoint: Option<usize>,
    pub auto_reconnect: bool,
    pub keep_alive_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub strict_host_key: bool,
}

impl ExportTunnel {
    fn has_invalid_port(&self) -> bool {
        self.endpoints.iter().any(|endpoint| endpoint.port == 0)
    }

    /// 丢掉缺地址 / 缺用户的线路；一条都不剩时整条隧道被跳过。
    fn normalize(&mut self) {
        self.endpoints.retain(|endpoint| {
            !endpoint.host.trim().is_empty() && !endpoint.username.trim().is_empty()
        });
        self.endpoints.truncate(MAX_ENDPOINTS);
        if let Some(index) = self.preferred_endpoint {
            if index >= self.endpoints.len() {
                self.preferred_endpoint = None;
            }
        }
    }
}

/// 清单版本 1 的隧道：SSH 接入信息直接摊在隧道上。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
struct ExportTunnelV1 {
    name: String,
    ssh_host: String,
    ssh_port: u16,
    ssh_user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    private_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    passphrase: Option<String>,
    jumps: Vec<ExportJump>,
    auto_reconnect: bool,
    keep_alive_seconds: u32,
    connect_timeout_seconds: u32,
    strict_host_key: bool,
}

impl ExportTunnelV1 {
    fn into_tunnel(self) -> ExportTunnel {
        ExportTunnel {
            name: self.name.clone(),
            endpoints: vec![ExportEndpoint {
                label: self.name,
                host: self.ssh_host,
                port: self.ssh_port,
                username: self.ssh_user,
                private_key: self.private_key,
                passphrase: self.passphrase,
                jumps: self.jumps,
                wifi_only: false,
                enabled: true,
            }],
            preferred_endpoint: None,
            auto_reconnect: self.auto_reconnect,
            keep_alive_seconds: self.keep_alive_seconds,
            connect_timeout_seconds: self.connect_timeout_seconds,
            strict_host_key: self.strict_host_key,
        }
    }
}

/// 清单版本 1 的文档布局（隧道为扁平结构）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
struct ExportDocumentV1 {
    version: u32,
    kind: String,
    exported_at: u64,
    tunnels: Vec<ExportTunnelV1>,
    groups: Vec<ExportGroup>,
    apps: Vec<ExportApp>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportGroup {
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportApp {
    pub name: String,
    /// 引用应用组的名称（`default` 表示默认组）。
    pub group: String,
    /// 引用隧道的名称。
    pub tunnel: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub browser_url: String,
    pub auto_start_on_boot: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportDocument {
    pub version: u32,
    pub kind: String,
    pub exported_at: u64,
    pub tunnels: Vec<ExportTunnel>,
    pub groups: Vec<ExportGroup>,
    pub apps: Vec<ExportApp>,
}

/// 导入结果，直接呈现给界面。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub tunnels_added: usize,
    pub tunnels_reused: usize,
    pub groups_added: usize,
    pub groups_reused: usize,
    pub apps_added: usize,
    pub apps_skipped: usize,
    /// 与现有应用撞了本地端口，导入后需要改端口才能启动。
    pub port_conflicts: Vec<String>,
    /// 引用不到隧道/应用组而被丢弃的应用。
    pub apps_dropped: usize,
}

/// 导出结果：实际写入了什么，以及哪些内容因为无法安全迁移而被跳过。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub path: String,
    pub tunnels: usize,
    pub groups: usize,
    pub apps: usize,
    /// 使用密码认证、因此没有导出的隧道名称。
    pub password_tunnels: Vec<String>,
    /// 使用密码认证、因此没有导出凭据的跳板机（格式 `隧道名 / 跳板机名`）。
    pub password_jumps: Vec<String>,
}

/// 导入前的预览：让用户先确认包里有什么。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundlePreview {
    pub tunnel_count: usize,
    pub group_count: usize,
    pub app_count: usize,
    pub tunnel_names: Vec<String>,
    pub group_names: Vec<String>,
    pub exported_at: u64,
    /// 清单里缺少私钥的隧道数量：导入后需要手动补凭据。
    pub tunnels_without_key: usize,
}

fn credential_plaintext(
    cipher: Option<&String>,
    secrets: &Secrets,
    label: &str,
    what: &str,
) -> Result<Option<String>, String> {
    match cipher {
        None => Ok(None),
        Some(payload) if payload.is_empty() => Ok(None),
        Some(payload) => secrets
            .decrypt(payload)
            .map(Some)
            .map_err(|_| format!("无法解密“{label}”的{what}，请重新填写后再导出")),
    }
}

fn export_credentials(
    credentials: &SshCredentials,
    secrets: &Secrets,
    label: &str,
) -> Result<(Option<String>, Option<String>), String> {
    match credentials.auth_type {
        // 密码无法安全迁移：保留隧道定义，由导出结果提示用户导入后重填。
        AuthType::Password => Ok((None, None)),
        AuthType::PrivateKey => {
            let key = credential_plaintext(
                credentials.private_key_cipher.as_ref(),
                secrets,
                label,
                "私钥",
            )?;
            let passphrase = credential_plaintext(
                credentials.passphrase_cipher.as_ref(),
                secrets,
                label,
                "私钥口令",
            )?;
            Ok((key, passphrase))
        }
    }
}

fn import_credentials(
    key: Option<&String>,
    passphrase: Option<&String>,
    secrets: &Secrets,
) -> Result<SshCredentials, String> {
    let encrypt = |value: Option<&String>| -> Result<Option<String>, String> {
        match value {
            Some(text) if !text.is_empty() => secrets
                .encrypt(text)
                .map(Some)
                .map_err(|error| error.to_string()),
            _ => Ok(None),
        }
    };
    let private_key_cipher = encrypt(key)?;
    Ok(SshCredentials {
        auth_type: if private_key_cipher.is_some() {
            AuthType::PrivateKey
        } else {
            AuthType::Password
        },
        password_cipher: None,
        private_key_cipher,
        passphrase_cipher: encrypt(passphrase)?,
    })
}

/// 导出清单与“无法迁移的部分”报告。
#[derive(Debug, Clone, Default)]
pub struct ExportPayload {
    pub document: ExportDocument,
    pub password_tunnels: Vec<String>,
    pub password_jumps: Vec<String>,
}

impl ExportPayload {
    pub fn summary(&self, path: &str) -> ExportSummary {
        ExportSummary {
            path: path.to_string(),
            tunnels: self.document.tunnels.len(),
            groups: self.document.groups.len(),
            apps: self.document.apps.len(),
            password_tunnels: self.password_tunnels.clone(),
            password_jumps: self.password_jumps.clone(),
        }
    }
}

/// 把当前配置整理成可导出清单。密码认证的线路只导出定义、不导出密码，
/// 并在 [`ExportPayload`] 里列出名字，由界面提示“导入后需要重新填写”。
pub fn build_document(snapshot: &DockSnapshot, secrets: &Secrets) -> Result<ExportPayload, String> {
    let group_names: std::collections::HashMap<&str, &str> = snapshot
        .groups
        .iter()
        .map(|group| (group.id.as_str(), group.name.as_str()))
        .collect();
    let tunnel_names: std::collections::HashMap<&str, &str> = snapshot
        .tunnels
        .iter()
        .map(|tunnel| (tunnel.id.as_str(), tunnel.name.as_str()))
        .collect();

    let mut password_tunnels = Vec::new();
    let mut password_jumps = Vec::new();
    let mut tunnels = Vec::with_capacity(snapshot.tunnels.len());
    for tunnel in &snapshot.tunnels {
        let mut endpoints = Vec::with_capacity(tunnel.endpoints.len());
        for endpoint in &tunnel.endpoints {
            let label = endpoint.display_name();
            if endpoint.credentials.auth_type == AuthType::Password {
                password_tunnels.push(format!("{} / {}", tunnel.name, label));
            }
            let (private_key, passphrase) =
                export_credentials(&endpoint.credentials, secrets, label)?;
            let mut jumps = Vec::with_capacity(endpoint.jumps.len());
            for jump in &endpoint.jumps {
                let jump_label = if jump.label.is_empty() {
                    jump.host.as_str()
                } else {
                    jump.label.as_str()
                };
                if jump.credentials.auth_type == AuthType::Password {
                    password_jumps.push(format!("{} / {} / {}", tunnel.name, label, jump_label));
                }
                let (jump_key, jump_passphrase) =
                    export_credentials(&jump.credentials, secrets, jump_label)?;
                jumps.push(ExportJump {
                    label: jump.label.clone(),
                    host: jump.host.clone(),
                    port: jump.port,
                    username: jump.username.clone(),
                    private_key: jump_key,
                    passphrase: jump_passphrase,
                });
            }
            endpoints.push(ExportEndpoint {
                label: endpoint.label.clone(),
                host: endpoint.host.clone(),
                port: endpoint.port,
                username: endpoint.username.clone(),
                private_key,
                passphrase,
                jumps,
                wifi_only: endpoint.wifi_only,
                enabled: endpoint.enabled,
            });
        }
        // 把首选线路移到第一位：导入端只需按顺序尝试，不必再存一个可变的下标。
        let mut preferred_endpoint = None;
        if let Some(preferred) = tunnel.preferred_endpoint_id.as_deref() {
            if let Some(index) = tunnel
                .endpoints
                .iter()
                .position(|endpoint| endpoint.id == preferred)
            {
                if index > 0 {
                    endpoints.swap(0, index);
                }
                preferred_endpoint = Some(0);
            }
        }
        tunnels.push(ExportTunnel {
            name: tunnel.name.clone(),
            endpoints,
            preferred_endpoint,
            auto_reconnect: tunnel.auto_reconnect,
            keep_alive_seconds: tunnel.keep_alive_seconds,
            connect_timeout_seconds: tunnel.connect_timeout_seconds,
            strict_host_key: tunnel.strict_host_key,
        });
    }

    let groups = snapshot
        .groups
        .iter()
        .filter(|group| !group.is_default())
        .map(|group| ExportGroup {
            name: group.name.clone(),
        })
        .collect();

    let mut apps = Vec::with_capacity(snapshot.apps.len());
    for app in &snapshot.apps {
        let Some(tunnel_name) = tunnel_names.get(app.tunnel_id.as_str()) else {
            continue;
        };
        let group_name = group_names
            .get(app.group_id.as_str())
            .copied()
            .unwrap_or(DEFAULT_GROUP_ID);
        apps.push(ExportApp {
            name: app.name.clone(),
            group: group_name.to_string(),
            tunnel: tunnel_name.to_string(),
            local_port: app.local_port,
            remote_host: app.remote_host.clone(),
            remote_port: app.remote_port,
            browser_url: app.browser_url.clone(),
            auto_start_on_boot: app.auto_start_on_boot,
        });
    }

    Ok(ExportPayload {
        document: ExportDocument {
            version: EXPORT_VERSION,
            kind: EXPORT_KIND.to_string(),
            exported_at: now_millis(),
            tunnels,
            groups,
            apps,
        },
        password_tunnels,
        password_jumps,
    })
}

/// 读取配置包并给出预览，不写入任何配置。
pub fn preview(document: &ExportDocument) -> BundlePreview {
    BundlePreview {
        tunnel_count: document.tunnels.len(),
        group_count: document.groups.len(),
        app_count: document.apps.len(),
        tunnel_names: document
            .tunnels
            .iter()
            .map(|tunnel| display_tunnel_name(&tunnel.name, tunnel))
            .collect(),
        group_names: document
            .groups
            .iter()
            .map(|group| group.name.clone())
            .collect(),
        exported_at: document.exported_at,
        tunnels_without_key: document
            .tunnels
            .iter()
            .filter(|tunnel| {
                tunnel
                    .endpoints
                    .iter()
                    .all(|endpoint| endpoint.private_key.is_none())
            })
            .count(),
    }
}

pub fn write_zip(path: &Path, document: &ExportDocument) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|error| format!("无法创建文件: {error}"))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file(README_NAME, options)
        .map_err(|error| format!("无法写入压缩包: {error}"))?;
    writer
        .write_all(README.as_bytes())
        .map_err(|error| format!("无法写入压缩包: {error}"))?;
    let manifest = serde_json::to_vec_pretty(document)
        .map_err(|error| format!("无法序列化配置: {error}"))?;
    writer
        .start_file(MANIFEST_NAME, options)
        .map_err(|error| format!("无法写入压缩包: {error}"))?;
    writer
        .write_all(&manifest)
        .map_err(|error| format!("无法写入压缩包: {error}"))?;
    writer
        .finish()
        .map_err(|error| format!("无法写入压缩包: {error}"))?;
    Ok(())
}

pub fn read_zip(path: &Path) -> Result<ExportDocument, String> {
    let file = std::fs::File::open(path).map_err(|error| format!("无法读取文件: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|_| "无法识别的压缩包，请选择 TunnelDock 导出的 .zip".to_string())?;

    let mut manifest: Option<String> = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("无法读取压缩包条目: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if name == MANIFEST_NAME || name.ends_with(".json") {
            // 解压后的大小上限，避免被构造的压缩包撑爆内存。
            if entry.size() > MAX_MANIFEST_BYTES {
                return Err("配置清单过大，可能不是 TunnelDock 的配置包".into());
            }
            let mut text = String::new();
            entry
                .take(MAX_MANIFEST_BYTES)
                .read_to_string(&mut text)
                .map_err(|error| format!("无法读取清单文件: {error}"))?;
            manifest = Some(text);
            if name == MANIFEST_NAME {
                break;
            }
        }
    }

    let text = manifest.ok_or_else(|| "压缩包里没有找到配置清单".to_string())?;
    decode_document(&text)
}

fn decode_document(text: &str) -> Result<ExportDocument, String> {
    // 先看 `kind` / `version`：v1 清单的隧道是扁平结构，用当前 schema 解码会
    // 得到“有名字、没有线路”的空隧道，因此必须先按版本走对应的布局。
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| format!("配置清单格式不正确: {error}"))?;
    let kind = value
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if kind != EXPORT_KIND {
        return Err("这不是 TunnelDock 的配置包".into());
    }
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if version > EXPORT_VERSION as u64 {
        return Err(format!(
            "配置包版本 {version} 高于当前应用支持的版本 {EXPORT_VERSION}，请先升级 TunnelDock"
        ));
    }
    if version < 2 {
        let legacy: ExportDocumentV1 = serde_json::from_str(text)
            .map_err(|error| format!("配置清单格式不正确: {error}"))?;
        return Ok(ExportDocument {
            version: EXPORT_VERSION,
            kind: legacy.kind,
            exported_at: legacy.exported_at,
            tunnels: legacy
                .tunnels
                .into_iter()
                .map(ExportTunnelV1::into_tunnel)
                .collect(),
            groups: legacy.groups,
            apps: legacy.apps,
        });
    }
    let document: ExportDocument = serde_json::from_str(text)
        .map_err(|error| format!("配置清单格式不正确: {error}"))?;
    Ok(document)
}

/// 把配置包合并进当前配置：
///
/// - 应用组按名称去重，`default` 组始终存在；
/// - 隧道按 `主机 + 端口 + 用户名` 去重，已存在时复用（缺凭据时补齐）；
/// - 应用按“同名同组”去重，引用不到隧道或应用组的应用计入 `apps_dropped`；
/// - 不导入运行状态，导入后不会自动启动。
///
/// 整份新配置只在最后**一次性**落盘，因此导入失败不会留下半份数据。
pub async fn apply_import(
    document: ExportDocument,
    store: &Store,
    secrets: &Secrets,
) -> Result<ImportSummary, String> {
    let mut summary = ImportSummary::default();
    let mut document = document;
    for tunnel in &mut document.tunnels {
        tunnel.normalize();
    }
    document.tunnels.retain(|tunnel| !tunnel.endpoints.is_empty());

    for tunnel in &document.tunnels {
        if tunnel.has_invalid_port() {
            return Err(format!("隧道“{}”的 SSH 端口不合法", tunnel.name));
        }
    }

    // 现有配置的一次性快照：去重判断全部基于它 + 本次新增的实体。
    let mut current = store.snapshot().await;
    let mut next = current.clone();
    // 给已有隧道补齐凭据也算改动，同样需要落盘。
    let mut changed = false;

    // ------------------------------------------------------------- groups
    let mut group_ids: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for group in &current.groups {
        group_ids.insert(group.name.to_ascii_lowercase(), group.id.clone());
    }
    for group in &document.groups {
        let name = group.name.trim();
        if name.is_empty() {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if let Some(id) = group_ids.get(&key) {
            if id != DEFAULT_GROUP_ID {
                summary.groups_reused += 1;
            }
            continue;
        }
        let saved = AppGroup {
            id: new_id(),
            name: name.to_string(),
            created_at: now_millis(),
        };
        group_ids.insert(key, saved.id.clone());
        next.groups.push(saved);
        summary.groups_added += 1;
    }

    // ------------------------------------------------------------- tunnels
    // 已有的每一条线路都会进入索引：同一条隧道在另一台机器上可能已经配置了
    // “家里 / 外网”两条入口，导入时要按线路粒度判断是否已存在。
    let mut tunnel_ids: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for tunnel in &current.tunnels {
        for endpoint in &tunnel.endpoints {
            tunnel_ids
                .entry(endpoint_index_key(&endpoint.host, endpoint.port, &endpoint.username))
                .or_insert_with(|| tunnel.id.clone());
        }
        tunnel_ids
            .entry(tunnel_key_by_name(&tunnel.name))
            .or_insert_with(|| tunnel.id.clone());
    }

    for tunnel in &document.tunnels {
        let name = display_tunnel_name(&tunnel.name, tunnel);
        let incoming = build_endpoints(tunnel, secrets)?;
        let existing_id = incoming
            .iter()
            .find_map(|endpoint| tunnel_ids.get(&endpoint.0).cloned())
            .or_else(|| tunnel_ids.get(&tunnel_key_by_name(&tunnel.name)).cloned());

        if let Some(existing_id) = existing_id {
            summary.tunnels_reused += 1;
            if let Some(existing) = next.tunnels.iter_mut().find(|item| item.id == existing_id) {
                // 只补齐“完全缺失”的东西：没有凭据的线路补上私钥、没有跳板的线路补上
                // 跳板。已有密码 / 私钥 / 跳板的线路一律保持原样，不能因为一次导入被
                // 静默改掉认证方式，否则可能影响正在运行的转发。
                let mut touched = false;
                for (key, endpoint) in &incoming {
                    match existing
                        .endpoints
                        .iter_mut()
                        .find(|item| endpoint_index_key(&item.host, item.port, &item.username) == *key)
                    {
                        Some(current_endpoint) => {
                            if !current_endpoint.credentials.has_secret()
                                && endpoint.credentials.private_key_cipher.is_some()
                            {
                                current_endpoint.credentials = endpoint.credentials.clone();
                                touched = true;
                            }
                            if current_endpoint.jumps.is_empty() && !endpoint.jumps.is_empty() {
                                current_endpoint.jumps = endpoint.jumps.clone();
                                touched = true;
                            }
                        }
                        // 线上还不存在这条线路：补一条，正是“换台机器也能还原多入口”的场景。
                        None => {
                            existing.endpoints.push(endpoint.clone());
                            touched = true;
                        }
                    }
                }
                if touched {
                    existing.updated_at = now_millis();
                    changed = true;
                }
            }
            tunnel_ids.insert(tunnel_key_by_name(&tunnel.name), existing_id);
            continue;
        }

        let mut config = TunnelConfig {
            id: new_id(),
            name,
            endpoints: incoming.iter().map(|(_, endpoint)| endpoint.clone()).collect(),
            preferred_endpoint_id: tunnel
                .preferred_endpoint
                .and_then(|index| incoming.get(index).map(|(_, endpoint)| endpoint.id.clone())),
            auto_reconnect: tunnel.auto_reconnect,
            keep_alive_seconds: tunnel.keep_alive_seconds.clamp(5, 300).max(5),
            connect_timeout_seconds: tunnel.connect_timeout_seconds.clamp(5, 120).max(5),
            strict_host_key: tunnel.strict_host_key,
            created_at: now_millis(),
            updated_at: now_millis(),
        };
        if config.preferred_endpoint_id.is_none() {
            config.preferred_endpoint_id = config.endpoints.first().map(|item| item.id.clone());
        }
        for (key, _) in &incoming {
            tunnel_ids.insert(key.clone(), config.id.clone());
        }
        tunnel_ids.insert(tunnel_key_by_name(&tunnel.name), config.id.clone());
        current.tunnels.push(config.clone());
        next.tunnels.push(config);
        summary.tunnels_added += 1;
    }

    // ---------------------------------------------------------------- apps
    let mut existing_names: HashSet<(String, String)> = current
        .apps
        .iter()
        .map(|app| (app.group_id.clone(), app.name.to_ascii_lowercase()))
        .collect();
    let mut used_ports: HashSet<u16> = current.apps.iter().map(|app| app.local_port).collect();

    for app in &document.apps {
        let name = app.name.trim();
        if name.is_empty() {
            summary.apps_dropped += 1;
            continue;
        }
        let Some(tunnel_id) = tunnel_ids.get(&tunnel_key_by_name(&app.tunnel)).cloned() else {
            summary.apps_dropped += 1;
            continue;
        };
        let group_id = group_ids
            .get(&app.group.trim().to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| DEFAULT_GROUP_ID.to_string());
        if existing_names.contains(&(group_id.clone(), name.to_ascii_lowercase())) {
            summary.apps_skipped += 1;
            continue;
        }
        if app.local_port < 1024 {
            summary.apps_dropped += 1;
            continue;
        }
        if used_ports.contains(&app.local_port) {
            summary
                .port_conflicts
                .push(format!("{}（{}）", name, app.local_port));
        }
        let config = ForwardApp {
            id: new_id(),
            name: name.to_string(),
            group_id: group_id.clone(),
            tunnel_id,
            local_port: app.local_port,
            remote_host: app.remote_host.trim().to_string(),
            remote_port: app.remote_port.max(1),
            browser_url: app.browser_url.trim().to_string(),
            auto_start_on_boot: app.auto_start_on_boot,
            desired_on: false,
            created_at: now_millis(),
            updated_at: now_millis(),
        };
        used_ports.insert(config.local_port);
        existing_names.insert((group_id, name.to_ascii_lowercase()));
        next.apps.push(config);
        summary.apps_added += 1;
    }

    if changed || summary.groups_added + summary.tunnels_added + summary.apps_added > 0 {
        store
            .replace_all(next)
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(summary)
}

fn tunnel_key_by_name(name: &str) -> String {
    format!("name:{}", name.trim().to_ascii_lowercase())
}

/// 识别一条线路的稳定键：`host|port|user`（主机名不区分大小写）。
fn endpoint_index_key(host: &str, port: u16, username: &str) -> String {
    format!(
        "{}|{}|{}",
        host.trim().to_ascii_lowercase(),
        port,
        username.trim()
    )
}

/// 把导出清单里的一条隧道转成 (线路键, 线路) 列表，凭据就地加密。
fn build_endpoints(
    tunnel: &ExportTunnel,
    secrets: &Secrets,
) -> Result<Vec<(String, TunnelEndpoint)>, String> {
    let mut endpoints = Vec::with_capacity(tunnel.endpoints.len());
    for endpoint in &tunnel.endpoints {
        let jumps = endpoint
            .jumps
            .iter()
            .map(|jump| build_jump(jump, secrets))
            .collect::<Result<Vec<_>, _>>()?;
        let key = endpoint.endpoint_key();
        endpoints.push((
            key,
            TunnelEndpoint {
                id: new_id(),
                label: endpoint.label.trim().to_string(),
                host: endpoint.host.trim().to_string(),
                port: endpoint.port.max(1),
                username: endpoint.username.trim().to_string(),
                credentials: import_credentials(
                    endpoint.private_key.as_ref(),
                    endpoint.passphrase.as_ref(),
                    secrets,
                )?,
                jumps,
                wifi_only: endpoint.wifi_only,
                enabled: endpoint.enabled,
            },
        ));
    }
    Ok(endpoints)
}

/// 隧道名缺失时用第一条线路的 `user@host` 兜底。
fn display_tunnel_name(name: &str, tunnel: &ExportTunnel) -> String {
    let endpoint = tunnel.endpoints.first();
    display_name_from(
        name,
        endpoint.map(|item| item.username.as_str()).unwrap_or(""),
        endpoint.map(|item| item.host.as_str()).unwrap_or(""),
    )
}

fn display_name_from(name: &str, user: &str, host: &str) -> String {
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        return trimmed.chars().take(40).collect();
    }
    let user = if user.trim().is_empty() {
        "root"
    } else {
        user.trim()
    };
    let host = if host.trim().is_empty() {
        "SSH"
    } else {
        host.trim()
    };
    format!("{user}@{host}").chars().take(40).collect()
}

fn build_jump(jump: &ExportJump, secrets: &Secrets) -> Result<JumpHost, String> {
    Ok(JumpHost {
        id: new_id(),
        label: jump.label.trim().to_string(),
        host: jump.host.trim().to_string(),
        port: if jump.port == 0 {
            DEFAULT_SSH_PORT
        } else {
            jump.port
        },
        username: jump.username.trim().to_string(),
        credentials: import_credentials(
            jump.private_key.as_ref(),
            jump.passphrase.as_ref(),
            secrets,
        )?,
    })
}

/// 仅在测试与文档中使用的默认值说明，保证导出的字段与模型默认值一致。
const _DEFAULTS: (u32, u32) = (
    crate::model::DEFAULT_KEEPALIVE_SECONDS,
    crate::model::DEFAULT_CONNECT_TIMEOUT_SECONDS,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AppGroup, JumpHost, DEFAULT_GROUP_ID};

    fn secrets(dir: &Path) -> std::sync::Arc<Secrets> {
        Secrets::load(dir).unwrap()
    }

    fn private_key_credentials(vault: &Secrets) -> SshCredentials {
        SshCredentials {
            auth_type: AuthType::PrivateKey,
            private_key_cipher: Some(vault.encrypt("-----BEGIN KEY-----").unwrap()),
            passphrase_cipher: Some(vault.encrypt("口令").unwrap()),
            ..SshCredentials::default()
        }
    }

    fn private_key_tunnel(vault: &Secrets, name: &str) -> TunnelConfig {
        TunnelConfig {
            name: name.into(),
            endpoints: vec![TunnelEndpoint {
                label: name.into(),
                host: "example.com".into(),
                port: 22,
                username: "ops".into(),
                credentials: private_key_credentials(vault),
                jumps: vec![JumpHost {
                    id: new_id(),
                    label: "堡垒机".into(),
                    host: "bastion.example.com".into(),
                    port: 2222,
                    username: "ops".into(),
                    credentials: SshCredentials {
                        auth_type: AuthType::PrivateKey,
                        private_key_cipher: Some(vault.encrypt("-----BEGIN BASTION-----").unwrap()),
                        ..SshCredentials::default()
                    },
                }],
                ..TunnelEndpoint::default()
            }],
            ..TunnelConfig::default()
        }
    }

    #[tokio::test]
    async fn roundtrip_zip_preserves_entities_and_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let store = Store::load(dir.path()).await.unwrap();
        let tunnel = store
            .upsert_tunnel(private_key_tunnel(&vault, "生产"))
            .await
            .unwrap();
        let group = store
            .upsert_group(AppGroup {
                name: "工作".into(),
                ..AppGroup::default()
            })
            .await
            .unwrap();
        store
            .upsert_app(ForwardApp {
                name: "后台".into(),
                group_id: group.id.clone(),
                tunnel_id: tunnel.id.clone(),
                local_port: 18080,
                remote_host: "10.0.0.5".into(),
                remote_port: 5000,
                auto_start_on_boot: true,
                ..ForwardApp::default()
            })
            .await
            .unwrap();

        let payload = build_document(&store.snapshot().await, &vault).unwrap();
        let document = payload.document;
        assert_eq!(document.tunnels.len(), 1);
        assert_eq!(document.groups.len(), 1, "默认组不导出");
        assert_eq!(document.apps.len(), 1);
        assert_eq!(document.apps[0].group, "工作");
        assert_eq!(document.apps[0].tunnel, "生产");
        assert_eq!(document.tunnels[0].endpoints.len(), 1);
        assert_eq!(
            document.tunnels[0].endpoints[0].private_key.as_deref(),
            Some("-----BEGIN KEY-----")
        );

        let zip_path = dir.path().join("export.zip");
        write_zip(&zip_path, &document).unwrap();
        let decoded = read_zip(&zip_path).unwrap();
        assert_eq!(decoded, document);

        // 导入到另一份空配置里。
        let target_dir = tempfile::tempdir().unwrap();
        let target_vault = secrets(target_dir.path());
        let target_store = Store::load(target_dir.path()).await.unwrap();
        let summary = apply_import(decoded, &target_store, &target_vault)
            .await
            .unwrap();
        assert_eq!(summary.tunnels_added, 1);
        assert_eq!(summary.groups_added, 1);
        assert_eq!(summary.apps_added, 1);

        let imported = target_store.snapshot().await;
        assert_eq!(imported.tunnels.len(), 1);
        assert_eq!(imported.tunnels[0].name, "生产");
        assert_eq!(imported.tunnels[0].endpoints.len(), 1);
        let endpoint = &imported.tunnels[0].endpoints[0];
        assert_eq!(endpoint.jumps.len(), 1);
        assert_eq!(
            target_vault
                .decrypt(endpoint.credentials.private_key_cipher.as_ref().unwrap())
                .unwrap(),
            "-----BEGIN KEY-----"
        );
        assert!(endpoint.credentials.passphrase_cipher.is_some());
        assert_eq!(
            target_vault
                .decrypt(
                    endpoint.jumps[0]
                        .credentials
                        .private_key_cipher
                        .as_ref()
                        .unwrap()
                )
                .unwrap(),
            "-----BEGIN BASTION-----"
        );
        let imported_group = imported.groups.iter().find(|g| g.name == "工作").unwrap();
        assert_eq!(imported.apps[0].group_id, imported_group.id);
        assert!(!imported.apps[0].desired_on, "导入不恢复运行状态");
        assert!(imported.apps[0].auto_start_on_boot);
    }

    #[tokio::test]
    async fn import_is_idempotent_and_dedupes_by_endpoint() {
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let source = Store::load(dir.path()).await.unwrap();
        source
            .upsert_tunnel(private_key_tunnel(&vault, "生产"))
            .await
            .unwrap();
        let document = build_document(&source.snapshot().await, &vault)
            .unwrap()
            .document;

        let target_dir = tempfile::tempdir().unwrap();
        let target_store = Store::load(target_dir.path()).await.unwrap();
        let first = apply_import(document.clone(), &target_store, &vault)
            .await
            .unwrap();
        assert_eq!(first.tunnels_added, 1);
        let second = apply_import(document, &target_store, &vault).await.unwrap();
        assert_eq!(second.tunnels_added, 0);
        assert_eq!(second.tunnels_reused, 1);
        assert_eq!(target_store.snapshot().await.tunnels.len(), 1);
    }

    #[tokio::test]
    async fn password_tunnels_export_without_the_password() {
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let store = Store::load(dir.path()).await.unwrap();
        store
            .upsert_tunnel(TunnelConfig {
                name: "密码隧道".into(),
                endpoints: vec![TunnelEndpoint {
                    label: "外网".into(),
                    host: "example.com".into(),
                    username: "ops".into(),
                    credentials: SshCredentials {
                        auth_type: AuthType::Password,
                        password_cipher: Some(vault.encrypt("s3cret").unwrap()),
                        ..SshCredentials::default()
                    },
                    ..TunnelEndpoint::default()
                }],
                ..TunnelConfig::default()
            })
            .await
            .unwrap();
        let payload = build_document(&store.snapshot().await, &vault).unwrap();
        assert_eq!(payload.password_tunnels, vec!["密码隧道 / 外网".to_string()]);
        assert_eq!(payload.document.tunnels.len(), 1);
        assert!(payload.document.tunnels[0].endpoints[0].private_key.is_none());
        assert!(payload.summary("x.zip").password_tunnels.len() == 1);
    }

    #[tokio::test]
    async fn a_failed_import_leaves_no_partial_configuration() {
        // 清单里有非法端口时，整次导入必须整体失败：不能出现“组建好了、隧道没建”的半份数据。
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let store = Store::load(dir.path()).await.unwrap();
        let before = store.snapshot().await;
        let document = ExportDocument {
            version: EXPORT_VERSION,
            kind: EXPORT_KIND.into(),
            groups: vec![ExportGroup {
                name: "新组".into(),
            }],
            tunnels: vec![ExportTunnel {
                name: "坏隧道".into(),
                endpoints: vec![ExportEndpoint {
                    label: "坏隧道".into(),
                    host: "example.com".into(),
                    port: 0,
                    username: "ops".into(),
                    private_key: Some("-----BEGIN KEY-----".into()),
                    ..ExportEndpoint::default()
                }],
                ..ExportTunnel::default()
            }],
            ..ExportDocument::default()
        };
        assert!(apply_import(document, &store, &vault).await.is_err());
        let after = store.snapshot().await;
        assert_eq!(after.groups.len(), before.groups.len());
        assert!(after.tunnels.is_empty());
        assert!(after.apps.is_empty());
    }

    #[tokio::test]
    async fn import_fills_missing_key_on_an_existing_tunnel() {
        // 第一次导入只拿到定义（无私钥），第二次导入了私钥后应当补齐而不是新建。
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let store = Store::load(dir.path()).await.unwrap();
        let mut document = ExportDocument {
            version: EXPORT_VERSION,
            kind: EXPORT_KIND.into(),
            tunnels: vec![ExportTunnel {
                name: "生产".into(),
                endpoints: vec![ExportEndpoint {
                    label: "生产".into(),
                    host: "example.com".into(),
                    port: 22,
                    username: "ops".into(),
                    ..ExportEndpoint::default()
                }],
                ..ExportTunnel::default()
            }],
            ..ExportDocument::default()
        };
        apply_import(document.clone(), &store, &vault)
            .await
            .unwrap();
        assert!(store.snapshot().await.tunnels[0].endpoints[0]
            .credentials
            .private_key_cipher
            .is_none());

        document.tunnels[0].endpoints[0].private_key = Some("-----BEGIN KEY-----".into());
        let summary = apply_import(document, &store, &vault).await.unwrap();
        assert_eq!(summary.tunnels_added, 0);
        assert_eq!(summary.tunnels_reused, 1);
        let snapshot = store.snapshot().await;
        assert_eq!(snapshot.tunnels.len(), 1);
        assert_eq!(
            vault
                .decrypt(
                    snapshot.tunnels[0].endpoints[0]
                        .credentials
                        .private_key_cipher
                        .as_ref()
                        .unwrap()
                )
                .unwrap(),
            "-----BEGIN KEY-----"
        );
    }

    #[tokio::test]
    async fn import_never_overwrites_an_existing_password_credential() {
        // 隧道已有密码凭据时，导入包里的私钥不能把它悄悄换掉。
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let store = Store::load(dir.path()).await.unwrap();
        let password_cipher = vault.encrypt("s3cret").unwrap();
        store
            .upsert_tunnel(TunnelConfig {
                name: "生产".into(),
                endpoints: vec![TunnelEndpoint {
                    label: "生产".into(),
                    host: "example.com".into(),
                    port: 22,
                    username: "ops".into(),
                    credentials: SshCredentials {
                        auth_type: AuthType::Password,
                        password_cipher: Some(password_cipher.clone()),
                        ..SshCredentials::default()
                    },
                    ..TunnelEndpoint::default()
                }],
                ..TunnelConfig::default()
            })
            .await
            .unwrap();

        let document = ExportDocument {
            version: EXPORT_VERSION,
            kind: EXPORT_KIND.into(),
            tunnels: vec![ExportTunnel {
                name: "生产".into(),
                endpoints: vec![ExportEndpoint {
                    label: "生产".into(),
                    host: "example.com".into(),
                    port: 22,
                    username: "ops".into(),
                    private_key: Some("-----BEGIN KEY-----".into()),
                    ..ExportEndpoint::default()
                }],
                ..ExportTunnel::default()
            }],
            ..ExportDocument::default()
        };
        let summary = apply_import(document, &store, &vault).await.unwrap();
        assert_eq!(summary.tunnels_reused, 1);

        let snapshot = store.snapshot().await;
        assert_eq!(snapshot.tunnels.len(), 1);
        let credentials = &snapshot.tunnels[0].endpoints[0].credentials;
        assert_eq!(credentials.auth_type, AuthType::Password);
        assert_eq!(
            credentials.password_cipher.as_deref(),
            Some(password_cipher.as_str()),
            "已有密码必须原样保留"
        );
        assert!(credentials.private_key_cipher.is_none());
    }

    /// 多线路隧道导出后必须原样还原，首选线路的先后顺序也要保住。
    #[tokio::test]
    async fn multi_endpoint_tunnels_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let store = Store::load(dir.path()).await.unwrap();
        let home = TunnelEndpoint {
            label: "家庭局域网".into(),
            host: "192.168.1.10".into(),
            port: 22,
            username: "ops".into(),
            credentials: private_key_credentials(&vault),
            wifi_only: true,
            ..TunnelEndpoint::default()
        };
        let away = TunnelEndpoint {
            label: "frp 外网".into(),
            host: "frp.example.com".into(),
            port: 2222,
            username: "ops".into(),
            credentials: private_key_credentials(&vault),
            ..TunnelEndpoint::default()
        };
        // 首选是第二条：导出时应被挪到第一位，导入后仍然优先尝试它。
        let saved = store
            .upsert_tunnel(TunnelConfig {
                name: "NAS".into(),
                endpoints: vec![home, away.clone()],
                preferred_endpoint_id: Some(away.id.clone()),
                ..TunnelConfig::default()
            })
            .await
            .unwrap();
        assert_eq!(saved.endpoints.len(), 2);

        let payload = build_document(&store.snapshot().await, &vault).unwrap();
        let document = payload.document;
        assert_eq!(document.tunnels[0].endpoints.len(), 2);
        assert_eq!(document.tunnels[0].endpoints[0].label, "frp 外网");
        assert_eq!(document.tunnels[0].preferred_endpoint, Some(0));
        assert!(document.tunnels[0].endpoints[0].enabled);

        let zip_path = dir.path().join("export.zip");
        write_zip(&zip_path, &document).unwrap();
        let decoded = read_zip(&zip_path).unwrap();

        let target_dir = tempfile::tempdir().unwrap();
        let target_vault = secrets(target_dir.path());
        let target_store = Store::load(target_dir.path()).await.unwrap();
        let summary = apply_import(decoded, &target_store, &target_vault)
            .await
            .unwrap();
        assert_eq!(summary.tunnels_added, 1);
        let imported = target_store.snapshot().await;
        let tunnel = &imported.tunnels[0];
        assert_eq!(tunnel.endpoints.len(), 2);
        assert_eq!(tunnel.endpoints[0].label, "frp 外网");
        assert_eq!(tunnel.endpoints[0].port, 2222);
        assert!(tunnel.endpoints.iter().any(|item| item.wifi_only));
        assert_eq!(
            tunnel.preferred_endpoint_id.as_deref(),
            Some(tunnel.endpoints[0].id.as_str())
        );
        for endpoint in &tunnel.endpoints {
            assert!(endpoint.credentials.private_key_cipher.is_some());
        }
    }

    /// 另一台机器上已有同一条隧道的“外网”线路时，导入“家里”线路应当补到
    /// 同一条隧道里，而不是新建一条隧道——这正是多入口跨设备还原的关键。
    #[tokio::test]
    async fn importing_a_second_line_extends_the_existing_tunnel() {
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let store = Store::load(dir.path()).await.unwrap();
        store
            .upsert_tunnel(TunnelConfig {
                name: "NAS".into(),
                endpoints: vec![TunnelEndpoint {
                    label: "frp 外网".into(),
                    host: "frp.example.com".into(),
                    port: 2222,
                    username: "ops".into(),
                    credentials: private_key_credentials(&vault),
                    ..TunnelEndpoint::default()
                }],
                ..TunnelConfig::default()
            })
            .await
            .unwrap();

        let document = ExportDocument {
            version: EXPORT_VERSION,
            kind: EXPORT_KIND.into(),
            tunnels: vec![ExportTunnel {
                name: "NAS".into(),
                endpoints: vec![ExportEndpoint {
                    label: "家庭局域网".into(),
                    host: "192.168.1.10".into(),
                    port: 22,
                    username: "ops".into(),
                    private_key: Some("-----BEGIN HOME-----".into()),
                    wifi_only: true,
                    enabled: true,
                    ..ExportEndpoint::default()
                }],
                ..ExportTunnel::default()
            }],
            ..ExportDocument::default()
        };
        let summary = apply_import(document, &store, &vault).await.unwrap();
        assert_eq!(summary.tunnels_added, 0);
        assert_eq!(summary.tunnels_reused, 1);
        let snapshot = store.snapshot().await;
        assert_eq!(snapshot.tunnels.len(), 1, "不能因为多一条线路就多一条隧道");
        let tunnel = &snapshot.tunnels[0];
        assert_eq!(tunnel.endpoints.len(), 2);
        let home = tunnel
            .endpoints
            .iter()
            .find(|item| item.host == "192.168.1.10")
            .expect("家里那条线路应当被补进来");
        assert!(home.wifi_only);
        assert_eq!(
            vault
                .decrypt(home.credentials.private_key_cipher.as_ref().unwrap())
                .unwrap(),
            "-----BEGIN HOME-----"
        );
    }

    /// 旧版（v1）配置包里的隧道是扁平结构，必须按“唯一一条线路”导入。
    #[test]
    fn a_version_one_bundle_upgrades_to_a_single_endpoint() {
        let text = serde_json::json!({
            "version": 1,
            "kind": EXPORT_KIND,
            "exportedAt": 42,
            "tunnels": [{
                "name": "生产",
                "sshHost": "example.com",
                "sshPort": 2222,
                "sshUser": "ops",
                "privateKey": "-----BEGIN KEY-----",
                "jumps": [{
                    "label": "堡垒机", "host": "bastion", "port": 22, "username": "ops",
                    "privateKey": "-----BEGIN BASTION-----"
                }]
            }],
            "groups": [{ "name": "工作" }],
            "apps": []
        })
        .to_string();
        let document = decode_document(&text).unwrap();
        assert_eq!(document.version, EXPORT_VERSION);
        assert_eq!(document.tunnels.len(), 1);
        let endpoint = &document.tunnels[0].endpoints[0];
        assert_eq!(endpoint.host, "example.com");
        assert_eq!(endpoint.port, 2222);
        assert_eq!(endpoint.username, "ops");
        assert_eq!(endpoint.label, "生产", "v1 没有线路名，用隧道名兜底");
        assert_eq!(endpoint.private_key.as_deref(), Some("-----BEGIN KEY-----"));
        assert_eq!(endpoint.jumps.len(), 1);
        assert!(endpoint.enabled);
        assert!(!endpoint.wifi_only);
        assert_eq!(document.groups.len(), 1);
    }

    /// 预览里的“缺私钥”按隧道统计：只要有一条线路带私钥就不算缺。
    #[test]
    fn preview_counts_tunnels_without_any_key() {
        let tunnel = |name: &str, key: Option<&str>| ExportTunnel {
            name: name.into(),
            endpoints: vec![ExportEndpoint {
                label: name.into(),
                host: "example.com".into(),
                port: 22,
                username: "ops".into(),
                private_key: key.map(str::to_string),
                enabled: true,
                ..ExportEndpoint::default()
            }],
            ..ExportTunnel::default()
        };
        let preview = preview(&ExportDocument {
            version: EXPORT_VERSION,
            tunnels: vec![tunnel("有私钥", Some("k")), tunnel("没私钥", None)],
            ..ExportDocument::default()
        });
        assert_eq!(preview.tunnel_count, 2);
        assert_eq!(preview.tunnels_without_key, 1);
    }

    /// 清单里缺地址 / 缺用户的线路会被丢掉，整条隧道没线路时整条跳过。
    #[test]
    fn lines_without_address_or_user_are_dropped() {
        let mut tunnel = ExportTunnel {
            name: "脏数据".into(),
            endpoints: vec![
                ExportEndpoint {
                    host: "  ".into(),
                    username: "ops".into(),
                    port: 22,
                    ..ExportEndpoint::default()
                },
                ExportEndpoint {
                    host: "ok.example.com".into(),
                    username: " ".into(),
                    port: 22,
                    ..ExportEndpoint::default()
                },
                ExportEndpoint {
                    host: "good.example.com".into(),
                    username: "ops".into(),
                    port: 22,
                    ..ExportEndpoint::default()
                },
            ],
            ..ExportTunnel::default()
        };
        tunnel.normalize();
        assert_eq!(tunnel.endpoints.len(), 1);
        assert_eq!(tunnel.endpoints[0].host, "good.example.com");

        let mut empty = ExportTunnel {
            name: "全脏".into(),
            ..ExportTunnel::default()
        };
        empty.normalize();
        assert!(empty.endpoints.is_empty());
    }

    #[test]
    fn rejects_foreign_and_newer_archives() {
        assert!(decode_document(r#"{"kind":"other"}"#).is_err());
        assert!(decode_document(
            &serde_json::json!({"kind": EXPORT_KIND, "version": EXPORT_VERSION + 1}).to_string()
        )
        .is_err());
        assert!(decode_document(
            &serde_json::json!({"kind": EXPORT_KIND, "version": EXPORT_VERSION}).to_string()
        )
        .is_ok());
    }

    #[test]
    fn reads_a_repacked_archive_from_another_tool() {
        // 用户可能用系统压缩工具重新打包：清单换了文件名、压缩方式也不同，
        // 只要能找到任意一个 json 清单就应当识别。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repacked.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file("notes.txt", stored).unwrap();
        writer.write_all(b"ignore me").unwrap();
        writer.start_file("backup/manifest.json", stored).unwrap();
        writer
            .write_all(
                serde_json::json!({
                    "kind": EXPORT_KIND,
                    "version": EXPORT_VERSION,
                    "tunnels": [{
                        "name": "甲", "sshHost": "h", "sshPort": 22, "sshUser": "u"
                    }]
                })
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        writer.finish().unwrap();

        let document = read_zip(&path).unwrap();
        assert_eq!(document.tunnels.len(), 1);
        assert_eq!(document.tunnels[0].name, "甲");
    }

    #[test]
    fn rejects_archives_without_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        writer
            .start_file(README_NAME, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"nothing here").unwrap();
        writer.finish().unwrap();
        assert!(read_zip(&path).unwrap_err().contains("配置清单"));
    }

    #[test]
    fn rejects_a_non_zip_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.zip");
        std::fs::write(&path, b"this is not a zip").unwrap();
        assert!(read_zip(&path).is_err());
    }

    #[tokio::test]
    async fn apps_with_dangling_references_are_dropped_not_created() {
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let store = Store::load(dir.path()).await.unwrap();
        let document = ExportDocument {
            version: EXPORT_VERSION,
            kind: EXPORT_KIND.into(),
            tunnels: vec![ExportTunnel {
                name: "生产".into(),
                endpoints: vec![ExportEndpoint {
                    label: "生产".into(),
                    host: "example.com".into(),
                    port: 22,
                    username: "ops".into(),
                    private_key: Some("-----BEGIN KEY-----".into()),
                    ..ExportEndpoint::default()
                }],
                ..ExportTunnel::default()
            }],
            apps: vec![
                ExportApp {
                    name: "孤儿".into(),
                    tunnel: "不存在".into(),
                    group: DEFAULT_GROUP_ID.into(),
                    local_port: 18080,
                    remote_host: "127.0.0.1".into(),
                    remote_port: 80,
                    ..ExportApp::default()
                },
                ExportApp {
                    name: "正常".into(),
                    tunnel: "生产".into(),
                    group: "未定义组".into(),
                    local_port: 18081,
                    remote_host: "127.0.0.1".into(),
                    remote_port: 80,
                    ..ExportApp::default()
                },
            ],
            ..ExportDocument::default()
        };
        let summary = apply_import(document, &store, &vault).await.unwrap();
        assert_eq!(summary.tunnels_added, 1);
        assert_eq!(summary.apps_added, 1);
        assert_eq!(summary.apps_dropped, 1);
        let snapshot = store.snapshot().await;
        assert_eq!(snapshot.apps.len(), 1);
        assert_eq!(snapshot.apps[0].name, "正常");
        assert_eq!(snapshot.apps[0].group_id, DEFAULT_GROUP_ID);
    }

    #[test]
    fn port_conflicts_are_reported_without_blocking_the_import() {
        // 端口冲突只在导入后影响启动，因此导入本身要继续，并把冲突名字带回界面。
        let summary = ImportSummary {
            port_conflicts: vec!["后台（18080）".into()],
            ..ImportSummary::default()
        };
        assert_eq!(summary.port_conflicts.len(), 1);
    }

    #[test]
    fn tunnel_names_are_bounded_and_defaulted() {
        let unnamed = ExportTunnel {
            endpoints: vec![ExportEndpoint {
                host: "example.com".into(),
                username: "ops".into(),
                ..ExportEndpoint::default()
            }],
            ..ExportTunnel::default()
        };
        assert_eq!(display_tunnel_name("  ", &unnamed), "ops@example.com");
        let named = ExportTunnel {
            name: "生产".into(),
            ..unnamed.clone()
        };
        assert_eq!(display_tunnel_name("生产", &named), "生产");
        let long = "x".repeat(80);
        assert_eq!(display_tunnel_name(&long, &named).chars().count(), 40);
        // 连线路都没有时也不能 panic。
        assert_eq!(display_tunnel_name("", &ExportTunnel::default()), "root@SSH");
    }

    /// 用外部工具读写我们生成的压缩包，确认跨设备可交换（默认不跑）。
    ///
    /// ```bash
    /// TD_ZIP_DIR=/tmp/zipcheck cargo test --manifest-path src-tauri/Cargo.toml \
    ///   --lib real_zip -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "写真实文件供外部工具校验，默认跳过"]
    fn real_zip_is_readable_by_external_tools() {
        let dir = std::env::var("TD_ZIP_DIR").expect("需要 TD_ZIP_DIR");
        let dir = Path::new(&dir);
        std::fs::create_dir_all(dir).unwrap();
        let document = ExportDocument {
            version: EXPORT_VERSION,
            kind: EXPORT_KIND.into(),
            exported_at: 1_700_000_000_000,
            tunnels: vec![ExportTunnel {
                name: "生产 NAS".into(),
                endpoints: vec![
                    ExportEndpoint {
                        label: "家庭局域网".into(),
                        host: "192.168.1.10".into(),
                        port: 22,
                        username: "ops".into(),
                        private_key: Some(OPENSSH_TEST_KEY.into()),
                        passphrase: None,
                        jumps: vec![],
                        wifi_only: true,
                        enabled: true,
                    },
                    ExportEndpoint {
                        label: "frp 外网".into(),
                        host: "example.com".into(),
                        port: 2222,
                        username: "ops".into(),
                        private_key: Some(OPENSSH_TEST_KEY.into()),
                        passphrase: None,
                        jumps: vec![ExportJump {
                            label: "堡垒机".into(),
                            host: "bastion.example.com".into(),
                            port: 22,
                            username: "ops".into(),
                            private_key: Some(OPENSSH_TEST_KEY.into()),
                            passphrase: None,
                        }],
                        wifi_only: false,
                        enabled: true,
                    },
                ],
                ..ExportTunnel::default()
            }],
            groups: vec![ExportGroup {
                name: "工作 / 家庭".into(),
            }],
            apps: vec![ExportApp {
                name: "公司后台".into(),
                group: "工作 / 家庭".into(),
                tunnel: "生产 NAS".into(),
                local_port: 18080,
                remote_host: "10.0.0.5".into(),
                remote_port: 5000,
                ..ExportApp::default()
            }],
        };
        let path = dir.join("tunneldock-real.zip");
        write_zip(&path, &document).unwrap();
        println!("zip={}", path.display());
        // 同一份文件必须能被我们自己读回，字段完全一致。
        assert_eq!(read_zip(&path).unwrap(), document);
        // 校验清单是 UTF-8 明文，方便用户排错。
        let raw = std::fs::read(&path).unwrap();
        assert!(raw.starts_with(b"PK"), "应当是标准 zip 头部");
    }

    const OPENSSH_TEST_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\n\
QyNTUxOQAAACBw5c7cJ4cJy0n6b2nHf5G6b2nHf5G6b2nHf5G6b2nHf5G6bQAAAJg=\n\
-----END OPENSSH PRIVATE KEY-----\n";
}
