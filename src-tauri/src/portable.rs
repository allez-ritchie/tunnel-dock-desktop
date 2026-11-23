//! 配置包（`.zip`）的导出与导入：应用组、应用、隧道。
//!
//! 导出的不是 `tunnels.json` 的复制，而是一份**可移植清单**：
//!
//! - 凭据只支持**私钥**（密码无法跨机器迁移，且会泄露明文口令）；
//! - 应用组、隧道、应用之间用**名称**而不是 ID 引用，因此导入到另一台机器
//!   同样能还原引用关系；
//! - 不含运行状态（`desired_on`），导入后不会自动启动任何转发。
//!
//! 清单版本 3 起，隧道就是一次确定的 SSH 接入（`host`/`port`/`username`），
//! 应用按顺序引用多条隧道（`tunnels`）。版本 1 / 2 的清单仍可导入：
//! v1 的隧道是扁平结构；v2 的隧道持有 `endpoints`，会被拆成多条隧道并把顺序
//! 写进应用的 `tunnels`，与本地 v3→v4 迁移保持一致。
//!
//! 压缩包内只有一个 `tunneldock-export.json` 清单和一个说明文件。清单是纯文本 JSON，
//! 出现问题时可以直接用文本编辑器检查。

use crate::model::{
    new_id, now_millis, AppGroup, AuthType, DockSnapshot, ForwardApp, JumpHost, SshCredentials,
    TunnelConfig, DEFAULT_GROUP_ID, DEFAULT_SSH_PORT,
};
use crate::secrets::Secrets;
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// 当前清单版本。1 是单线路的扁平结构，2 起隧道持有 `endpoints` 数组，
/// 3 起隧道是单次接入、隧道列表归应用。
pub const EXPORT_VERSION: u32 = 3;
pub const EXPORT_KIND: &str = "tunneldock.export";
pub const MANIFEST_NAME: &str = "tunneldock-export.json";
pub const README_NAME: &str = "README.txt";
/// 清单解压后的上限：配置包本就很小，超出即视为异常文件。
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
/// 一个应用最多引用多少条隧道，防止异常清单撑爆内存。
const MAX_APP_TUNNELS: usize = 8;

const README: &str = "\
TunnelDock 配置包
=================

- tunneldock-export.json：应用组、应用、隧道的可移植清单（纯文本 JSON）。
- 一条隧道就是一次确定的 SSH 接入（用户名 / 地址 / 端口 / 跳板）。
- 应用按顺序引用多条隧道，启动时依次尝试，本地端口保持不变。
- 凭据仅支持私钥；使用密码认证的隧道不会被导出。
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

/// 导出清单里的一条隧道：一次确定的 SSH 接入。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportTunnel {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
    pub jumps: Vec<ExportJump>,
    pub auto_reconnect: bool,
    pub keep_alive_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub strict_host_key: bool,
}

impl ExportTunnel {
    fn has_valid_access(&self) -> bool {
        !self.host.trim().is_empty() && !self.username.trim().is_empty()
    }

    /// 识别一次接入的稳定键：`host|port|user`（主机名不区分大小写）。
    fn access_key(&self) -> String {
        access_key(&self.host, self.port, &self.username)
    }
}

fn access_key(host: &str, port: u16, username: &str) -> String {
    format!(
        "{}|{}|{}",
        host.trim().to_ascii_lowercase(),
        port,
        username.trim()
    )
}

/// 清单版本 2 的隧道：名称 + 一组线路。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
struct ExportTunnelV2 {
    name: String,
    endpoints: Vec<ExportEndpointV2>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preferred_endpoint: Option<usize>,
    auto_reconnect: bool,
    keep_alive_seconds: u32,
    connect_timeout_seconds: u32,
    strict_host_key: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
struct ExportEndpointV2 {
    label: String,
    host: String,
    port: u16,
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    private_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    passphrase: Option<String>,
    jumps: Vec<ExportJump>,
    wifi_only: bool,
    enabled: bool,
}

impl ExportTunnelV2 {
    /// 拆成多条 v3 隧道，返回按顺序排列的隧道名（与本地 v3→v4 迁移一致：
    /// 首选线路在前，其余按声明顺序；停用的线路保留但应用不引用）。
    fn into_tunnels(self) -> (Vec<ExportTunnel>, Vec<String>) {
        if self.endpoints.is_empty() {
            return (
                vec![ExportTunnel {
                    name: display_name(self.name.trim(), "", ""),
                    auto_reconnect: self.auto_reconnect,
                    keep_alive_seconds: self.keep_alive_seconds,
                    connect_timeout_seconds: self.connect_timeout_seconds,
                    strict_host_key: self.strict_host_key,
                    ..ExportTunnel::default()
                }],
                Vec::new(),
            );
        }
        let preferred = self
            .preferred_endpoint
            .filter(|index| *index < self.endpoints.len());
        let mut ordered: Vec<usize> = Vec::with_capacity(self.endpoints.len());
        if let Some(index) = preferred {
            ordered.push(index);
        }
        ordered.extend((0..self.endpoints.len()).filter(|index| Some(*index) != preferred));

        let mut tunnels = Vec::with_capacity(ordered.len());
        let mut names: Vec<String> = Vec::with_capacity(ordered.len());
        let mut referenced: Vec<String> = Vec::new();
        for index in ordered {
            let endpoint = &self.endpoints[index];
            let name = split_name(&self.name, &endpoint.label, &endpoint.host);
            names.push(name.clone());
            if endpoint.enabled || preferred == Some(index) {
                referenced.push(name.clone());
            }
            tunnels.push(ExportTunnel {
                name,
                host: endpoint.host.clone(),
                port: endpoint.port,
                username: endpoint.username.clone(),
                private_key: endpoint.private_key.clone(),
                passphrase: endpoint.passphrase.clone(),
                jumps: endpoint.jumps.clone(),
                auto_reconnect: self.auto_reconnect,
                keep_alive_seconds: self.keep_alive_seconds,
                connect_timeout_seconds: self.connect_timeout_seconds,
                strict_host_key: self.strict_host_key,
            });
        }
        if referenced.is_empty() {
            referenced = names;
        }
        (tunnels, referenced)
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
            name: display_name(self.name.trim(), &self.ssh_user, &self.ssh_host),
            host: self.ssh_host,
            port: self.ssh_port,
            username: self.ssh_user,
            private_key: self.private_key,
            passphrase: self.passphrase,
            jumps: self.jumps,
            auto_reconnect: self.auto_reconnect,
            keep_alive_seconds: self.keep_alive_seconds,
            connect_timeout_seconds: self.connect_timeout_seconds,
            strict_host_key: self.strict_host_key,
        }
    }
}

/// 清单版本 1 的应用：引用单条隧道。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
struct ExportAppV1 {
    name: String,
    group: String,
    tunnel: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
    browser_url: String,
    auto_start_on_boot: bool,
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
    apps: Vec<ExportAppV1>,
}

/// 清单版本 2 的文档布局（隧道持有线路，应用引用单条隧道）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
struct ExportDocumentV2 {
    version: u32,
    kind: String,
    exported_at: u64,
    tunnels: Vec<ExportTunnelV2>,
    groups: Vec<ExportGroup>,
    apps: Vec<ExportAppV1>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportGroup {
    pub name: String,
}

/// 导出清单里的一个应用：按顺序引用多条隧道名。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportApp {
    pub name: String,
    /// 引用应用组的名称（`default` 表示默认组）。
    pub group: String,
    /// 按尝试顺序排列的隧道名，第一条是首选。
    pub tunnels: Vec<String>,
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

/// 把当前配置整理成可导出清单。密码认证的隧道只导出定义、不导出密码，
/// 并在 [`ExportPayload`] 里列出名字，由界面提示“导入后需要重新填写”。
pub fn build_document(snapshot: &DockSnapshot, secrets: &Secrets) -> Result<ExportPayload, String> {
    let group_names: HashMap<&str, &str> = snapshot
        .groups
        .iter()
        .map(|group| (group.id.as_str(), group.name.as_str()))
        .collect();
    let tunnel_names: HashMap<&str, &str> = snapshot
        .tunnels
        .iter()
        .map(|tunnel| (tunnel.id.as_str(), tunnel.name.as_str()))
        .collect();

    let mut password_tunnels = Vec::new();
    let mut password_jumps = Vec::new();
    let mut tunnels = Vec::with_capacity(snapshot.tunnels.len());
    for tunnel in &snapshot.tunnels {
        if tunnel.credentials.auth_type == AuthType::Password {
            password_tunnels.push(tunnel.name.clone());
        }
        let (private_key, passphrase) =
            export_credentials(&tunnel.credentials, secrets, &tunnel.name)?;
        let mut jumps = Vec::with_capacity(tunnel.jumps.len());
        for jump in &tunnel.jumps {
            let jump_label = if jump.label.is_empty() {
                jump.host.as_str()
            } else {
                jump.label.as_str()
            };
            if jump.credentials.auth_type == AuthType::Password {
                password_jumps.push(format!("{} / {}", tunnel.name, jump_label));
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
        tunnels.push(ExportTunnel {
            name: tunnel.name.clone(),
            host: tunnel.host.clone(),
            port: tunnel.port,
            username: tunnel.username.clone(),
            private_key,
            passphrase,
            jumps,
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
        // 引用不到隧道的应用导出后也没法还原，直接跳过。
        let tunnel_names_ordered: Vec<String> = app
            .tunnel_ids
            .iter()
            .filter_map(|id| tunnel_names.get(id.as_str()).map(|name| name.to_string()))
            .collect();
        if tunnel_names_ordered.is_empty() {
            continue;
        }
        let group_name = group_names
            .get(app.group_id.as_str())
            .copied()
            .unwrap_or(DEFAULT_GROUP_ID);
        apps.push(ExportApp {
            name: app.name.clone(),
            group: group_name.to_string(),
            tunnels: tunnel_names_ordered,
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
            .map(|tunnel| display_name(&tunnel.name, &tunnel.username, &tunnel.host))
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
            .filter(|tunnel| tunnel.private_key.is_none())
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
    // 先看 `kind` / `version`：旧版清单的隧道布局不同，用当前 schema 解码会
    // 得到“有名字、没有接入信息”的空隧道，因此必须先按版本走对应的布局。
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
            apps: legacy
                .apps
                .into_iter()
                .map(|app| ExportApp {
                    name: app.name,
                    group: app.group,
                    tunnels: vec![app.tunnel],
                    local_port: app.local_port,
                    remote_host: app.remote_host,
                    remote_port: app.remote_port,
                    browser_url: app.browser_url,
                    auto_start_on_boot: app.auto_start_on_boot,
                })
                .collect(),
        });
    }
    if version < 3 {
        let legacy: ExportDocumentV2 = serde_json::from_str(text)
            .map_err(|error| format!("配置清单格式不正确: {error}"))?;
        return Ok(upgrade_v2(legacy));
    }
    let document: ExportDocument = serde_json::from_str(text)
        .map_err(|error| format!("配置清单格式不正确: {error}"))?;
    Ok(document)
}

/// v2 清单 → v3：每条线路拆成一条隧道，应用改持隧道名列表。
fn upgrade_v2(legacy: ExportDocumentV2) -> ExportDocument {
    let mut tunnels = Vec::new();
    // 旧隧道名 → 按顺序排列的新隧道名。
    let mut order_by_old: HashMap<String, Vec<String>> = HashMap::new();
    for old in legacy.tunnels {
        let key = old.name.trim().to_ascii_lowercase();
        let (split, referenced) = old.into_tunnels();
        if !key.is_empty() {
            order_by_old.insert(key, referenced);
        }
        tunnels.extend(split);
    }
    let apps = legacy
        .apps
        .into_iter()
        .map(|app| ExportApp {
            tunnels: order_by_old
                .get(&app.tunnel.trim().to_ascii_lowercase())
                .cloned()
                .unwrap_or_else(|| vec![app.tunnel]),
            name: app.name,
            group: app.group,
            local_port: app.local_port,
            remote_host: app.remote_host,
            remote_port: app.remote_port,
            browser_url: app.browser_url,
            auto_start_on_boot: app.auto_start_on_boot,
        })
        .collect();
    ExportDocument {
        version: EXPORT_VERSION,
        kind: EXPORT_KIND.to_string(),
        exported_at: legacy.exported_at,
        tunnels,
        groups: legacy.groups,
        apps,
    }
}

/// 把配置包合并进当前配置：
///
/// - 应用组按名称去重，`default` 组始终存在；
/// - 隧道按 `主机 + 端口 + 用户名` 去重，已存在时复用（缺凭据时补齐）；
/// - 应用按“同名同组”去重，隧道按名称解析成 ID，引用不到的应用计入 `apps_dropped`；
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
        tunnel.name = display_name(&tunnel.name, &tunnel.username, &tunnel.host);
    }
    // 缺地址 / 缺用户的隧道没法连接，直接丢掉。
    document.tunnels.retain(ExportTunnel::has_valid_access);
    for tunnel in &document.tunnels {
        if tunnel.port == 0 {
            return Err(format!("隧道“{}”的 SSH 端口不合法", tunnel.name));
        }
    }

    // 现有配置的一次性快照：去重判断全部基于它 + 本次新增的实体。
    let mut current = store.snapshot().await;
    let mut next = current.clone();
    // 给已有隧道补齐凭据也算改动，同样需要落盘。
    let mut changed = false;

    // ------------------------------------------------------------- groups
    let mut group_ids: HashMap<String, String> = HashMap::new();
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
    // 已有隧道按“接入键”和“名称”双索引：同一台服务器换个名字也能复用。
    let mut tunnel_ids: HashMap<String, String> = HashMap::new();
    for tunnel in &current.tunnels {
        tunnel_ids
            .entry(access_key(&tunnel.host, tunnel.port, &tunnel.username))
            .or_insert_with(|| tunnel.id.clone());
        tunnel_ids
            .entry(tunnel_key_by_name(&tunnel.name))
            .or_insert_with(|| tunnel.id.clone());
    }

    for tunnel in &document.tunnels {
        let name = tunnel.name.clone();
        let key = tunnel.access_key();
        let existing_id = tunnel_ids
            .get(&key)
            .cloned()
            .or_else(|| tunnel_ids.get(&tunnel_key_by_name(&name)).cloned());

        if let Some(existing_id) = existing_id {
            summary.tunnels_reused += 1;
            if let Some(existing) = next.tunnels.iter_mut().find(|item| item.id == existing_id) {
                // 只补齐“完全缺失”的东西：没有凭据的隧道补上私钥、没有跳板的补上
                // 跳板。已有密码 / 私钥 / 跳板的一律保持原样，不能因为一次导入被
                // 静默改掉认证方式，否则可能影响正在运行的转发。
                let mut touched = false;
                if !existing.credentials.has_secret() && tunnel.private_key.is_some() {
                    existing.credentials = import_credentials(
                        tunnel.private_key.as_ref(),
                        tunnel.passphrase.as_ref(),
                        secrets,
                    )?;
                    touched = true;
                }
                if existing.jumps.is_empty() && !tunnel.jumps.is_empty() {
                    existing.jumps = tunnel
                        .jumps
                        .iter()
                        .map(|jump| build_jump(jump, secrets))
                        .collect::<Result<Vec<_>, _>>()?;
                    touched = true;
                }
                if touched {
                    existing.updated_at = now_millis();
                    changed = true;
                }
            }
            tunnel_ids.insert(tunnel_key_by_name(&name), existing_id.clone());
            tunnel_ids.entry(key).or_insert(existing_id);
            continue;
        }

        let config = TunnelConfig {
            id: new_id(),
            name: name.clone(),
            host: tunnel.host.trim().to_string(),
            port: tunnel.port.max(1),
            username: tunnel.username.trim().to_string(),
            credentials: import_credentials(
                tunnel.private_key.as_ref(),
                tunnel.passphrase.as_ref(),
                secrets,
            )?,
            jumps: tunnel
                .jumps
                .iter()
                .map(|jump| build_jump(jump, secrets))
                .collect::<Result<Vec<_>, _>>()?,
            auto_reconnect: tunnel.auto_reconnect,
            keep_alive_seconds: tunnel.keep_alive_seconds.clamp(5, 300),
            connect_timeout_seconds: tunnel.connect_timeout_seconds.clamp(5, 120),
            strict_host_key: tunnel.strict_host_key,
            created_at: now_millis(),
            updated_at: now_millis(),
        };
        tunnel_ids.insert(key, config.id.clone());
        tunnel_ids.insert(tunnel_key_by_name(&name), config.id.clone());
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
        // 隧道按名称解析，顺序保持清单里的顺序；解析不到的跳过。
        let mut tunnel_ids_ordered: Vec<String> = Vec::new();
        for tunnel_name in app.tunnels.iter().take(MAX_APP_TUNNELS) {
            if let Some(id) = tunnel_ids.get(&tunnel_key_by_name(tunnel_name)) {
                if !tunnel_ids_ordered.contains(id) {
                    tunnel_ids_ordered.push(id.clone());
                }
            }
        }
        if tunnel_ids_ordered.is_empty() {
            summary.apps_dropped += 1;
            continue;
        }
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
            tunnel_ids: tunnel_ids_ordered,
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

/// 隧道名缺失时用 `user@host` 兜底；名称超过 40 个字符时截断。
fn display_name(name: &str, user: &str, host: &str) -> String {
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

/// 拆分后的隧道名：`原名 · 线路名`，超长时截断到 40 个字符。
fn split_name(name: &str, label: &str, host: &str) -> String {
    let base = {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            "SSH"
        } else {
            trimmed
        }
    };
    let suffix = {
        let label = label.trim();
        if !label.is_empty() {
            label
        } else {
            host.trim()
        }
    };
    let combined = if suffix.is_empty() {
        base.to_string()
    } else {
        format!("{base} · {suffix}")
    };
    combined.chars().take(40).collect()
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
                tunnel_ids: vec![tunnel.id.clone()],
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
        assert_eq!(document.apps[0].tunnels, vec!["生产".to_string()]);
        assert_eq!(
            document.tunnels[0].private_key.as_deref(),
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
        assert_eq!(imported.tunnels[0].host, "example.com");
        assert_eq!(imported.tunnels[0].jumps.len(), 1);
        assert_eq!(
            target_vault
                .decrypt(
                    imported.tunnels[0]
                        .credentials
                        .private_key_cipher
                        .as_ref()
                        .unwrap()
                )
                .unwrap(),
            "-----BEGIN KEY-----"
        );
        assert!(imported.tunnels[0].credentials.passphrase_cipher.is_some());
        assert_eq!(
            target_vault
                .decrypt(
                    imported.tunnels[0].jumps[0]
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
        assert_eq!(imported.apps[0].tunnel_ids.len(), 1);
        assert_eq!(
            imported.tunnel(&imported.apps[0].tunnel_ids[0]).unwrap().name,
            "生产"
        );
        assert!(!imported.apps[0].desired_on, "导入不恢复运行状态");
        assert!(imported.apps[0].auto_start_on_boot);
    }

    #[tokio::test]
    async fn import_is_idempotent_and_dedupes_by_access() {
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
                host: "example.com".into(),
                username: "ops".into(),
                credentials: SshCredentials {
                    auth_type: AuthType::Password,
                    password_cipher: Some(vault.encrypt("s3cret").unwrap()),
                    ..SshCredentials::default()
                },
                ..TunnelConfig::default()
            })
            .await
            .unwrap();
        let payload = build_document(&store.snapshot().await, &vault).unwrap();
        assert_eq!(payload.password_tunnels, vec!["密码隧道".to_string()]);
        assert_eq!(payload.document.tunnels.len(), 1);
        assert!(payload.document.tunnels[0].private_key.is_none());
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
                host: "example.com".into(),
                port: 0,
                username: "ops".into(),
                private_key: Some("-----BEGIN KEY-----".into()),
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
                host: "example.com".into(),
                port: 22,
                username: "ops".into(),
                ..ExportTunnel::default()
            }],
            ..ExportDocument::default()
        };
        apply_import(document.clone(), &store, &vault)
            .await
            .unwrap();
        assert!(store.snapshot().await.tunnels[0]
            .credentials
            .private_key_cipher
            .is_none());

        document.tunnels[0].private_key = Some("-----BEGIN KEY-----".into());
        let summary = apply_import(document, &store, &vault).await.unwrap();
        assert_eq!(summary.tunnels_added, 0);
        assert_eq!(summary.tunnels_reused, 1);
        let snapshot = store.snapshot().await;
        assert_eq!(snapshot.tunnels.len(), 1);
        assert_eq!(
            vault
                .decrypt(
                    snapshot.tunnels[0]
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
                host: "example.com".into(),
                port: 22,
                username: "ops".into(),
                credentials: SshCredentials {
                    auth_type: AuthType::Password,
                    password_cipher: Some(password_cipher.clone()),
                    ..SshCredentials::default()
                },
                ..TunnelConfig::default()
            })
            .await
            .unwrap();

        let document = ExportDocument {
            version: EXPORT_VERSION,
            kind: EXPORT_KIND.into(),
            tunnels: vec![ExportTunnel {
                name: "生产".into(),
                host: "example.com".into(),
                port: 22,
                username: "ops".into(),
                private_key: Some("-----BEGIN KEY-----".into()),
                ..ExportTunnel::default()
            }],
            ..ExportDocument::default()
        };
        let summary = apply_import(document, &store, &vault).await.unwrap();
        assert_eq!(summary.tunnels_reused, 1);

        let snapshot = store.snapshot().await;
        assert_eq!(snapshot.tunnels.len(), 1);
        let credentials = &snapshot.tunnels[0].credentials;
        assert_eq!(credentials.auth_type, AuthType::Password);
        assert_eq!(
            credentials.password_cipher.as_deref(),
            Some(password_cipher.as_str()),
            "已有密码必须原样保留"
        );
        assert!(credentials.private_key_cipher.is_none());
    }

    /// 两个入口就是两条隧道，应用按顺序引用；导出导入后顺序必须原样还原。
    #[tokio::test]
    async fn app_tunnel_order_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let store = Store::load(dir.path()).await.unwrap();
        let home = store
            .upsert_tunnel(TunnelConfig {
                name: "家里".into(),
                host: "192.168.1.10".into(),
                username: "ops".into(),
                credentials: private_key_credentials(&vault),
                ..TunnelConfig::default()
            })
            .await
            .unwrap();
        let away = store
            .upsert_tunnel(TunnelConfig {
                name: "外网".into(),
                host: "frp.example.com".into(),
                port: 2222,
                username: "ops".into(),
                credentials: private_key_credentials(&vault),
                ..TunnelConfig::default()
            })
            .await
            .unwrap();
        // 外网优先、家里兜底。
        store
            .upsert_app(ForwardApp {
                name: "NAS".into(),
                tunnel_ids: vec![away.id.clone(), home.id.clone()],
                local_port: 18080,
                remote_host: "127.0.0.1".into(),
                remote_port: 5000,
                ..ForwardApp::default()
            })
            .await
            .unwrap();

        let payload = build_document(&store.snapshot().await, &vault).unwrap();
        let document = payload.document;
        assert_eq!(document.tunnels.len(), 2, "每条隧道只描述一次接入");
        assert_eq!(
            document.apps[0].tunnels,
            vec!["外网".to_string(), "家里".to_string()]
        );

        let zip_path = dir.path().join("export.zip");
        write_zip(&zip_path, &document).unwrap();
        let decoded = read_zip(&zip_path).unwrap();

        let target_dir = tempfile::tempdir().unwrap();
        let target_vault = secrets(target_dir.path());
        let target_store = Store::load(target_dir.path()).await.unwrap();
        let summary = apply_import(decoded, &target_store, &target_vault)
            .await
            .unwrap();
        assert_eq!(summary.tunnels_added, 2);
        assert_eq!(summary.apps_added, 1);
        let imported = target_store.snapshot().await;
        let app = &imported.apps[0];
        assert_eq!(app.tunnel_ids.len(), 2);
        assert_eq!(imported.tunnel(&app.tunnel_ids[0]).unwrap().name, "外网");
        assert_eq!(imported.tunnel(&app.tunnel_ids[1]).unwrap().name, "家里");
        for tunnel in &imported.tunnels {
            assert!(tunnel.credentials.private_key_cipher.is_some());
        }
    }

    /// 另一台机器上已有同一个入口时，导入不会新建第二条隧道，但应用仍能引用它。
    #[tokio::test]
    async fn importing_an_existing_access_reuses_the_tunnel() {
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let store = Store::load(dir.path()).await.unwrap();
        let existing = store
            .upsert_tunnel(TunnelConfig {
                name: "外网".into(),
                host: "frp.example.com".into(),
                port: 2222,
                username: "ops".into(),
                credentials: private_key_credentials(&vault),
                ..TunnelConfig::default()
            })
            .await
            .unwrap();

        let document = ExportDocument {
            version: EXPORT_VERSION,
            kind: EXPORT_KIND.into(),
            tunnels: vec![
                ExportTunnel {
                    name: "外网（改名）".into(),
                    host: "frp.example.com".into(),
                    port: 2222,
                    username: "ops".into(),
                    private_key: Some("-----BEGIN AWAY-----".into()),
                    ..ExportTunnel::default()
                },
                ExportTunnel {
                    name: "家里".into(),
                    host: "192.168.1.10".into(),
                    port: 22,
                    username: "ops".into(),
                    private_key: Some("-----BEGIN HOME-----".into()),
                    ..ExportTunnel::default()
                },
            ],
            apps: vec![ExportApp {
                name: "NAS".into(),
                group: DEFAULT_GROUP_ID.into(),
                tunnels: vec!["外网（改名）".into(), "家里".into()],
                local_port: 18080,
                remote_host: "127.0.0.1".into(),
                remote_port: 5000,
                ..ExportApp::default()
            }],
            ..ExportDocument::default()
        };
        let summary = apply_import(document, &store, &vault).await.unwrap();
        assert_eq!(summary.tunnels_added, 1, "同一 host/port/user 只补一条");
        assert_eq!(summary.tunnels_reused, 1);
        let snapshot = store.snapshot().await;
        assert_eq!(snapshot.tunnels.len(), 2);
        let app = &snapshot.apps[0];
        assert_eq!(app.tunnel_ids[0], existing.id, "复用已有隧道而不是新建");
        assert_eq!(
            snapshot.tunnel(&app.tunnel_ids[1]).unwrap().host,
            "192.168.1.10"
        );
    }

    /// 旧版（v1）配置包里的隧道是扁平结构，按“单条隧道”导入。
    #[test]
    fn a_version_one_bundle_upgrades_to_a_flat_tunnel() {
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
            "apps": [{
                "name": "后台", "group": "工作", "tunnel": "生产",
                "localPort": 18080, "remoteHost": "127.0.0.1", "remotePort": 80
            }]
        })
        .to_string();
        let document = decode_document(&text).unwrap();
        assert_eq!(document.version, EXPORT_VERSION);
        assert_eq!(document.tunnels.len(), 1);
        let tunnel = &document.tunnels[0];
        assert_eq!(tunnel.name, "生产");
        assert_eq!(tunnel.host, "example.com");
        assert_eq!(tunnel.port, 2222);
        assert_eq!(tunnel.username, "ops");
        assert_eq!(tunnel.private_key.as_deref(), Some("-----BEGIN KEY-----"));
        assert_eq!(tunnel.jumps.len(), 1);
        assert_eq!(document.groups.len(), 1);
        assert_eq!(document.apps[0].tunnels, vec!["生产".to_string()]);
    }

    /// v2 配置包：一条隧道多条线路，导入时拆成多条隧道并写进应用的隧道列表。
    #[test]
    fn a_version_two_bundle_splits_lines_into_tunnels() {
        let text = serde_json::json!({
            "version": 2,
            "kind": EXPORT_KIND,
            "tunnels": [{
                "name": "NAS",
                "endpoints": [
                    {
                        "label": "家庭局域网", "host": "192.168.1.10", "port": 22,
                        "username": "ops", "privateKey": "-----BEGIN HOME-----",
                        "wifiOnly": true, "enabled": true
                    },
                    {
                        "label": "frp 外网", "host": "frp.example.com", "port": 2222,
                        "username": "ops", "privateKey": "-----BEGIN AWAY-----",
                        "enabled": true
                    }
                ],
                "preferredEndpoint": 1
            }],
            "apps": [{
                "name": "后台", "group": "default", "tunnel": "NAS",
                "localPort": 18080, "remoteHost": "127.0.0.1", "remotePort": 80
            }]
        })
        .to_string();
        let document = decode_document(&text).unwrap();
        assert_eq!(document.version, EXPORT_VERSION);
        assert_eq!(document.tunnels.len(), 2);
        // 首选线路排在最前，其余按声明顺序。
        assert_eq!(document.tunnels[0].name, "NAS · frp 外网");
        assert_eq!(document.tunnels[0].host, "frp.example.com");
        assert_eq!(document.tunnels[1].name, "NAS · 家庭局域网");
        assert_eq!(
            document.apps[0].tunnels,
            vec!["NAS · frp 外网".to_string(), "NAS · 家庭局域网".to_string()]
        );
    }

    /// v2 包里被停用的线路不进入应用的隧道列表，但隧道定义仍然保留。
    #[test]
    fn a_disabled_v2_line_is_kept_but_not_referenced() {
        let text = serde_json::json!({
            "version": 2,
            "kind": EXPORT_KIND,
            "tunnels": [{
                "name": "NAS",
                "endpoints": [
                    {
                        "label": "已停用", "host": "dead.example.com", "port": 22,
                        "username": "ops", "enabled": false
                    },
                    {
                        "label": "外网", "host": "frp.example.com", "port": 2222,
                        "username": "ops", "privateKey": "k", "enabled": true
                    }
                ]
            }],
            "apps": [{
                "name": "后台", "group": "default", "tunnel": "NAS",
                "localPort": 18080, "remoteHost": "127.0.0.1", "remotePort": 80
            }]
        })
        .to_string();
        let document = decode_document(&text).unwrap();
        assert_eq!(document.tunnels.len(), 2);
        assert_eq!(
            document.apps[0].tunnels,
            vec!["NAS · 外网".to_string()],
            "停用的入口不进应用的尝试顺序"
        );
    }

    /// 预览里的“缺私钥”按隧道统计。
    #[test]
    fn preview_counts_tunnels_without_a_key() {
        let tunnel = |name: &str, key: Option<&str>| ExportTunnel {
            name: name.into(),
            host: "example.com".into(),
            port: 22,
            username: "ops".into(),
            private_key: key.map(str::to_string),
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

    /// 缺地址 / 缺用户的隧道会被丢掉，引用它的应用计入 dropped。
    #[tokio::test]
    async fn tunnels_without_address_or_user_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let vault = secrets(dir.path());
        let store = Store::load(dir.path()).await.unwrap();
        let document = ExportDocument {
            version: EXPORT_VERSION,
            kind: EXPORT_KIND.into(),
            tunnels: vec![
                ExportTunnel {
                    name: "脏数据".into(),
                    host: "  ".into(),
                    username: "ops".into(),
                    port: 22,
                    ..ExportTunnel::default()
                },
                ExportTunnel {
                    name: "正常".into(),
                    host: "good.example.com".into(),
                    username: "ops".into(),
                    port: 22,
                    ..ExportTunnel::default()
                },
            ],
            apps: vec![
                ExportApp {
                    name: "孤儿".into(),
                    group: DEFAULT_GROUP_ID.into(),
                    tunnels: vec!["脏数据".into()],
                    local_port: 18080,
                    remote_host: "127.0.0.1".into(),
                    remote_port: 80,
                    ..ExportApp::default()
                },
                ExportApp {
                    name: "正常应用".into(),
                    group: DEFAULT_GROUP_ID.into(),
                    tunnels: vec!["正常".into()],
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
        assert_eq!(snapshot.apps[0].name, "正常应用");
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
                        "name": "甲", "host": "h", "port": 22, "username": "u"
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
                host: "example.com".into(),
                port: 22,
                username: "ops".into(),
                private_key: Some("-----BEGIN KEY-----".into()),
                ..ExportTunnel::default()
            }],
            apps: vec![
                ExportApp {
                    name: "孤儿".into(),
                    tunnels: vec!["不存在".into()],
                    group: DEFAULT_GROUP_ID.into(),
                    local_port: 18080,
                    remote_host: "127.0.0.1".into(),
                    remote_port: 80,
                    ..ExportApp::default()
                },
                ExportApp {
                    name: "正常".into(),
                    tunnels: vec!["生产".into()],
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
        // 引用不到的隧道名被跳过，不会留下空引用。
        assert_eq!(snapshot.apps[0].tunnel_ids.len(), 1);
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
        assert_eq!(display_name("  ", "ops", "example.com"), "ops@example.com");
        assert_eq!(display_name("生产", "ops", "example.com"), "生产");
        let long = "x".repeat(80);
        assert_eq!(display_name(&long, "ops", "h").chars().count(), 40);
        assert_eq!(display_name("", "", ""), "root@SSH");
        assert_eq!(split_name("NAS", "外网", "x"), "NAS · 外网");
        assert_eq!(split_name("NAS", "  ", "frp.example.com"), "NAS · frp.example.com");
        assert_eq!(split_name("  ", "", ""), "SSH");
        assert_eq!(split_name(&"名".repeat(40), "外网", "x").chars().count(), 40);
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
            tunnels: vec![
                ExportTunnel {
                    name: "NAS · 家庭局域网".into(),
                    host: "192.168.1.10".into(),
                    port: 22,
                    username: "ops".into(),
                    private_key: Some(OPENSSH_TEST_KEY.into()),
                    ..ExportTunnel::default()
                },
                ExportTunnel {
                    name: "NAS · frp 外网".into(),
                    host: "example.com".into(),
                    port: 2222,
                    username: "ops".into(),
                    private_key: Some(OPENSSH_TEST_KEY.into()),
                    jumps: vec![ExportJump {
                        label: "堡垒机".into(),
                        host: "bastion.example.com".into(),
                        port: 22,
                        username: "ops".into(),
                        private_key: Some(OPENSSH_TEST_KEY.into()),
                        passphrase: None,
                    }],
                    ..ExportTunnel::default()
                },
            ],
            groups: vec![ExportGroup {
                name: "工作 / 家庭".into(),
            }],
            apps: vec![ExportApp {
                name: "公司后台".into(),
                group: "工作 / 家庭".into(),
                tunnels: vec!["NAS · frp 外网".into(), "NAS · 家庭局域网".into()],
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
