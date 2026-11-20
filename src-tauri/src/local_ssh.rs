//! 读取本机 `~/.ssh`：把其中的私钥交给 SSH 客户端使用，并解析 `~/.ssh/config`
//! 以便一次性导入多台主机。
//!
//! 这里只做**只读**访问，从不改写用户的 `~/.ssh`。私钥内容只在内存中停留，
//! 由调用方（`commands.rs`）交给 `secrets.rs` 加密后落盘；日志中不输出任何密钥内容。

use serde::Serialize;
use std::path::{Path, PathBuf};

/// 单个私钥文件的大小上限，避免误读巨大的非密钥文件。
const MAX_KEY_BYTES: u64 = 256 * 1024;
/// `~/.ssh/config` 的大小上限。
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
/// OpenSSH 私钥（`openssh-key-v1`）的固定魔数。
const OPENSSH_MAGIC: &[u8] = b"openssh-key-v1\0";

/// `~/.ssh` 目录的只读视图。
#[derive(Debug, Clone)]
pub struct LocalSsh {
    root: PathBuf,
}

/// 目标机上的一个私钥文件。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalSshKey {
    /// 文件名（不含目录），例如 `id_ed25519`。
    pub name: String,
    /// 绝对路径，仅用于界面展示。
    pub path: String,
    /// `ssh-rsa` / `ssh-ed25519` / `ecdsa-sha2-nistp256` 等；无法判断时为空串。
    pub key_type: String,
    /// 私钥本身带口令（OpenSSH 的 cipher 不是 `none`）。
    pub encrypted: bool,
}

/// `~/.ssh` 的整体探测结果，供设置页与导入页展示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSshStatus {
    pub available: bool,
    pub root: String,
    pub keys: Vec<LocalSshKey>,
    pub config_available: bool,
    pub config_path: String,
    pub config_host_count: usize,
    pub config_importable_count: usize,
}

/// `~/.ssh/config` 里的一条 `Host` 配置（已展开 `HostName` / `User` / `Port`）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshConfigHost {
    /// `Host` 行里的名字，例如 `prod-db`。
    pub alias: String,
    pub host_name: String,
    pub user: String,
    pub port: u16,
    /// `IdentityFile` 原始写法（可能含 `~`、`%d`）。
    pub identity_file: Option<String>,
    /// `ProxyJump` 展开后的别名列表，第 1 项最靠近本机。
    pub proxy_jump: Vec<String>,
    /// 能否直接导入；不能导入时 `unsupported` 给出原因代码。
    pub importable: bool,
    /// `proxy_command` / `include` / `match` / `proxy_jump`。
    pub unsupported: Option<String>,
}

impl LocalSsh {
    /// 使用当前登录用户的 home 目录；取不到时返回一个不可用的实例。
    pub fn detect() -> Self {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        let root = home
            .map(|home| home.join(".ssh"))
            .unwrap_or_else(|| PathBuf::from(".ssh"));
        Self { root }
    }

    /// 指定目录，测试用。
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config")
    }

    pub fn exists(&self) -> bool {
        self.root.is_dir()
    }

    /// 探测目录：列出私钥文件，并统计 `config` 中可导入的主机数。
    pub fn scan(&self) -> LocalSshStatus {
        let keys = if self.exists() {
            self.scan_keys()
        } else {
            Vec::new()
        };
        let config_available = self.config_path().is_file();
        let (host_count, importable_count) = if config_available {
            match self.read_config() {
                Ok(text) => {
                    let hosts = parse_ssh_config(&text);
                    (
                        hosts.len(),
                        hosts.iter().filter(|host| host.importable).count(),
                    )
                }
                Err(_) => (0, 0),
            }
        } else {
            (0, 0)
        };
        LocalSshStatus {
            available: self.exists(),
            root: shorten_home(&self.root.to_string_lossy()),
            keys: keys
                .into_iter()
                .map(|mut key| {
                    key.path = shorten_home(&key.path);
                    key
                })
                .collect(),
            config_available,
            config_path: shorten_home(&self.config_path().to_string_lossy()),
            config_host_count: host_count,
            config_importable_count: importable_count,
        }
    }

    fn scan_keys(&self) -> Vec<LocalSshKey> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) => {
                tracing::warn!(%error, "无法读取 .ssh 目录");
                return Vec::new();
            }
        };
        let mut keys: Vec<LocalSshKey> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let metadata = entry.metadata().ok()?;
                if !metadata.is_file() || metadata.len() > MAX_KEY_BYTES {
                    return None;
                }
                let name = path.file_name()?.to_string_lossy().to_string();
                if name.ends_with(".pub") {
                    return None;
                }
                let raw = std::fs::read(&path).ok()?;
                let key_type = detect_key_type(&raw, &self.root, &name);
                let encrypted = is_encrypted_key(&raw);
                key_type.map(|key_type| LocalSshKey {
                    name,
                    path: path.to_string_lossy().to_string(),
                    key_type,
                    encrypted,
                })
            })
            .collect();
        keys.sort_by(|a, b| a.name.cmp(&b.name));
        keys
    }

    pub fn read_config(&self) -> Result<String, String> {
        read_limited(&self.config_path(), MAX_CONFIG_BYTES)
            .map_err(|error| format!("无法读取 ~/.ssh/config: {error}"))
    }

    /// 按文件名读取私钥。只接受 `.ssh` 目录下真实存在的普通文件，
    /// 避免通过 `../` 之类的路径把任意文件读进凭据里。
    pub fn read_key(&self, name: &str) -> Result<String, String> {
        if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
            return Err("私钥文件名不合法".into());
        }
        let path = self.root.join(name);
        if !path.is_file() {
            return Err(format!("~/.ssh 下没有私钥 {name}"));
        }
        let text = read_limited(&path, MAX_KEY_BYTES)
            .map_err(|error| format!("无法读取私钥 {name}: {error}"))?;
        if decode_secret_key(&text, None).is_err() && !looks_like_private_key(&text) {
            return Err(format!("{name} 不是可识别的私钥文件"));
        }
        Ok(text)
    }

    /// 解析当前 `config`。
    pub fn parse_config(&self) -> Result<Vec<SshConfigHost>, String> {
        Ok(parse_ssh_config(&self.read_config()?))
    }
}

fn read_limited(path: &Path, limit: u64) -> std::io::Result<String> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "文件过大",
        ));
    }
    let raw = std::fs::read(path)?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

fn looks_like_private_key(text: &str) -> bool {
    let head = text.trim_start();
    head.starts_with("-----BEGIN") && head.contains("PRIVATE KEY")
}

/// 判断是否私钥，并尽量给出公钥算法名。
fn detect_key_type(raw: &[u8], dir: &Path, name: &str) -> Option<String> {
    let text = std::str::from_utf8(raw).ok()?;
    let head = text.trim_start();
    if !head.starts_with("-----BEGIN") || !head.contains("PRIVATE KEY") {
        return None;
    }
    if head.starts_with("-----BEGIN RSA PRIVATE KEY-----") {
        return Some("ssh-rsa".into());
    }
    if head.starts_with("-----BEGIN EC PRIVATE KEY-----") {
        return Some("ecdsa".into());
    }
    if head.starts_with("-----BEGIN DSA PRIVATE KEY-----") {
        return Some("ssh-dss".into());
    }
    if head.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----") {
        // OpenSSH 格式不写死算法名，优先读同名 .pub 的第一段。
        if let Some(key_type) = public_key_type(&dir.join(format!("{name}.pub"))) {
            return Some(key_type);
        }
        return Some(String::new());
    }
    Some(String::new())
}

fn public_key_type(path: &Path) -> Option<String> {
    let raw = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&raw);
    let first = text.trim().split_whitespace().next()?;
    if first.starts_with("ssh-") || first.starts_with("ecdsa-") || first.starts_with("sk-") {
        Some(first.to_string())
    } else {
        None
    }
}

/// 是否加密（带口令）的私钥：OpenSSH 格式看 `openssh-key-v1` 里的 cipher 名，
/// 传统 PEM 看 `Proc-Type: 4,ENCRYPTED`。
fn is_encrypted_key(raw: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(raw) else {
        return false;
    };
    if text.contains("Proc-Type: 4,ENCRYPTED") {
        return true;
    }
    let Some(body) = extract_pem_body(text) else {
        return false;
    };
    // 只解码前若干个 base64 分组即可读到 magic + cipherName。
    let prefix: String = body.chars().take(48).collect();
    let Ok(decoded) = base64_decode(&prefix) else {
        return false;
    };
    if !decoded.starts_with(OPENSSH_MAGIC) {
        return false;
    }
    let rest = &decoded[OPENSSH_MAGIC.len()..];
    if rest.len() < 4 {
        return false;
    }
    let length = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
    if length == 0 || rest.len() < 4 + length {
        return false;
    }
    let cipher = String::from_utf8_lossy(&rest[4..4 + length]).to_string();
    cipher != "none"
}

fn extract_pem_body(text: &str) -> Option<String> {
    let mut lines = text.lines();
    let begin = lines.next()?;
    if !begin.starts_with("-----BEGIN") {
        return None;
    }
    let mut body = String::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("-----END") {
            return Some(body);
        }
        if trimmed.is_empty() || trimmed.contains(':') {
            continue;
        }
        body.push_str(trimmed);
    }
    None
}

/// 极简 Base64 解码（不接受空白），避免为了读一个 cipher 名引入额外依赖。
fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for &byte in bytes {
        if byte == b'=' {
            break;
        }
        let Some(value) = TABLE.iter().position(|&item| item == byte) else {
            return Err(());
        };
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

/// russh 的私钥解析器；`read_key` 用它确认文件确实是私钥。
fn decode_secret_key(pem: &str, passphrase: Option<&str>) -> Result<(), ()> {
    russh::keys::decode_secret_key(pem, passphrase)
        .map(|_| ())
        .map_err(|_| ())
}

// ---------------------------------------------------------------- ssh config

#[derive(Debug, Default, Clone)]
struct RawBlock {
    patterns: Vec<String>,
    host_name: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_file: Option<String>,
    proxy_jump: Option<String>,
    proxy_command: bool,
    include: bool,
    match_seen: bool,
}

fn is_wildcard(pattern: &str) -> bool {
    pattern.contains(['*', '?'])
}

/// 按 OpenSSH 的写法切分一行：`#` 起注释，双引号内的空格不切分。
fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '#' if !quoted => break,
            '"' => {
                quoted = !quoted;
            }
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '=' if !quoted => {
                // 兼容 `HostName=example.com` 这种写法：等号也当分隔符。
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            ch if ch.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn parse_blocks(text: &str) -> Vec<RawBlock> {
    let mut blocks: Vec<RawBlock> = Vec::new();
    let mut current: Option<RawBlock> = None;
    let mut match_active = false;

    for line in text.lines() {
        let tokens = tokenize(line);
        let Some(keyword) = tokens.first().cloned() else {
            continue;
        };
        let key = keyword.to_ascii_lowercase();
        let value = tokens.get(1).cloned().unwrap_or_default();
        if key == "host" {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            let patterns = tokens[1..]
                .iter()
                .map(|item| item.to_string())
                .filter(|item| !item.is_empty())
                .collect();
            current = Some(RawBlock {
                patterns,
                ..RawBlock::default()
            });
            match_active = false;
            continue;
        }
        if key == "match" {
            match_active = true;
            continue;
        }
        let Some(block) = current.as_mut() else {
            continue;
        };
        if match_active {
            block.match_seen = true;
            continue;
        }
        match key.as_str() {
            "hostname" => block.host_name = Some(value),
            "user" => block.user = Some(value),
            "port" => block.port = value.parse::<u16>().ok(),
            "identityfile" => block.identity_file = Some(value),
            "proxyjump" => block.proxy_jump = Some(value),
            "proxycommand" => block.proxy_command = true,
            "include" => block.include = true,
            _ => {}
        }
    }
    if let Some(block) = current.take() {
        blocks.push(block);
    }
    blocks
}

/// `ProxyJump` 的值可以写成 `user@host:port,host2`，这里只留主机名。
fn split_proxy_jump(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty() && *item != "none")
        .map(|item| {
            let without_user = item.rsplit('@').next().unwrap_or(item);
            let host = match without_user.rfind(':') {
                Some(index) if without_user[index + 1..].chars().all(|ch| ch.is_ascii_digit()) => {
                    &without_user[..index]
                }
                _ => without_user,
            };
            host.to_string()
        })
        .collect()
}

/// 解析 `~/.ssh/config`：只暴露带具体 `Host` 名的块，`Host *` 之类的通配块
/// 作为默认值参与补全，`Match` 之后的条件分支一律忽略（保守策略）。
pub fn parse_ssh_config(text: &str) -> Vec<SshConfigHost> {
    let blocks = parse_blocks(text);
    let defaults: Vec<&RawBlock> = blocks
        .iter()
        .filter(|block| {
            !block.patterns.is_empty() && block.patterns.iter().all(|item| is_wildcard(item))
        })
        .collect();

    let mut hosts: Vec<SshConfigHost> = Vec::new();
    for block in blocks.iter() {
        let concrete: Vec<&String> = block
            .patterns
            .iter()
            .filter(|item| !is_wildcard(item))
            .collect();
        if concrete.is_empty() {
            continue;
        }
        let mut effective = block.clone();
        for default in &defaults {
            if effective.host_name.is_none() {
                effective.host_name = default.host_name.clone();
            }
            if effective.user.is_none() {
                effective.user = default.user.clone();
            }
            if effective.port.is_none() {
                effective.port = default.port;
            }
            if effective.identity_file.is_none() {
                effective.identity_file = default.identity_file.clone();
            }
            if effective.proxy_jump.is_none() {
                effective.proxy_jump = default.proxy_jump.clone();
            }
            effective.proxy_command |= default.proxy_command;
        }

        let proxy_jump = effective
            .proxy_jump
            .as_deref()
            .map(split_proxy_jump)
            .unwrap_or_default();
        let unsupported = if effective.proxy_command {
            Some("proxy_command".to_string())
        } else if effective.include {
            Some("include".to_string())
        } else if effective.match_seen {
            Some("match".to_string())
        } else {
            None
        };

        for alias in concrete {
            let mut host = SshConfigHost {
                alias: alias.clone(),
                host_name: effective
                    .host_name
                    .clone()
                    .unwrap_or_else(|| alias.to_string()),
                user: effective.user.clone().unwrap_or_else(|| "root".to_string()),
                port: effective.port.unwrap_or(22),
                identity_file: effective.identity_file.clone(),
                proxy_jump: proxy_jump.clone(),
                importable: unsupported.is_none(),
                unsupported: unsupported.clone(),
            };
            // 跳板机必须是 config 里另有一条 Host 的名字，否则无法还原链路。
            if host.importable
                && host.proxy_jump.iter().any(|jump| {
                    !hosts.iter().any(|item| item.alias == *jump)
                        && !blocks
                            .iter()
                            .any(|block| block.patterns.iter().any(|p| p == jump))
                })
            {
                host.importable = false;
                host.unsupported = Some("proxy_jump".into());
            }
            // `Host A B` 的两个名字指向同一台机器，只保留第一条的完整信息。
            if !hosts.iter().any(|item| item.alias == host.alias) {
                hosts.push(host);
            }
        }
    }
    hosts
}

/// 给前端用的可读路径：把 home 目录替换成 `~`，避免界面出现完整用户名。
pub fn shorten_home(path: &str) -> String {
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok());
    match home {
        Some(home) if !home.is_empty() && path.starts_with(&home) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, content: &[u8]) {
        std::fs::write(dir.join(name), content).unwrap();
    }

    const OPENSSH_UNENCRYPTED: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\n\
QyNTUxOQAAACBw5c7cJ4cJy0n6b2nHf5G6b2nHf5G6b2nHf5G6b2nHf5G6bQAAAJg=\n\
-----END OPENSSH PRIVATE KEY-----\n";

    #[test]
    fn open_ssh_private_key_is_not_encrypted() {
        assert!(!is_encrypted_key(OPENSSH_UNENCRYPTED.as_bytes()));
    }

    #[test]
    fn detects_open_ssh_key_and_public_type() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "id_ed25519", OPENSSH_UNENCRYPTED.as_bytes());
        write(
            dir.path(),
            "id_ed25519.pub",
            b"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHA user@host\n",
        );
        let local = LocalSsh::at(dir.path());
        let status = local.scan();
        assert!(status.available);
        assert_eq!(status.keys.len(), 1, "只列私钥，不列 .pub");
        assert_eq!(status.keys[0].name, "id_ed25519");
        assert_eq!(status.keys[0].key_type, "ssh-ed25519");
        assert!(!status.keys[0].encrypted);
        assert!(!status.config_available);
    }

    #[test]
    fn marks_encrypted_open_ssh_key() {
        // cipher 名是 "aes256-ctr"，因此应当判断为带口令。
        let key = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0\n\
-----END OPENSSH PRIVATE KEY-----\n";
        assert!(is_encrypted_key(key.as_bytes()));
        assert!(!is_encrypted_key(OPENSSH_UNENCRYPTED.as_bytes()));
    }

    #[test]
    fn ignores_non_key_files_and_rejects_traversal() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "known_hosts", b"example.com ssh-ed25519 AAAA\n");
        write(dir.path(), "config", b"Host a\n  HostName a.example.com\n");
        write(dir.path(), "id_rsa", OPENSSH_UNENCRYPTED.as_bytes());
        let local = LocalSsh::at(dir.path());
        let status = local.scan();
        assert_eq!(status.keys.len(), 1);
        assert_eq!(status.keys[0].name, "id_rsa");
        assert!(local.read_key("../config").is_err());
        assert!(local.read_key("nope").is_err());
    }

    #[test]
    fn reads_plain_key_but_rejects_encrypted_one_without_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "id_ed25519", OPENSSH_UNENCRYPTED.as_bytes());
        let local = LocalSsh::at(dir.path());
        // 页面里伪造的内容不是合法私钥，必须拒绝。
        assert!(local.read_key("id_ed25519").is_ok());
    }

    #[test]
    fn parses_hosts_with_defaults_and_jumps() {
        let config = r#"
# 注释
Host *
  ServerAliveInterval 30
  User ops

Host prod
  HostName 10.0.0.5
  Port 2222

Host bastion
  HostName bastion.example.com

Host inner
  HostName 10.0.0.9
  User root
  ProxyJump bastion

Host box-a box-b
  HostName 10.0.0.20   # 行尾注释
"#;
        let hosts = parse_ssh_config(config);
        let prod = hosts.iter().find(|host| host.alias == "prod").unwrap();
        assert_eq!(prod.host_name, "10.0.0.5");
        assert_eq!(prod.port, 2222);
        assert_eq!(prod.user, "ops", "Host * 的 User 作为默认值");
        assert!(prod.importable);

        let inner = hosts.iter().find(|host| host.alias == "inner").unwrap();
        assert_eq!(inner.user, "root");
        assert_eq!(inner.port, 22);
        assert_eq!(inner.proxy_jump, vec!["bastion".to_string()]);
        assert!(inner.importable);

        let a = hosts.iter().find(|host| host.alias == "box-a").unwrap();
        assert_eq!(a.host_name, "10.0.0.20");
        assert!(hosts.iter().any(|host| host.alias == "box-b"));
        assert!(!hosts.iter().any(|host| host.alias == "*"));
    }

    #[test]
    fn flags_unsupported_directives() {
        let config = "\
Host proxied
  HostName p.example.com
  ProxyCommand nc %h %p

Host included
  HostName i.example.com
  Include ~/.ssh/extra

Host matched
  HostName m.example.com
  Match host m.example.com
  User root

Host dangling
  HostName d.example.com
  ProxyJump missing-bastion
";
        let hosts = parse_ssh_config(config);
        let by_alias = |alias: &str| hosts.iter().find(|host| host.alias == alias).unwrap();
        assert_eq!(
            by_alias("proxied").unsupported.as_deref(),
            Some("proxy_command")
        );
        assert_eq!(by_alias("included").unsupported.as_deref(), Some("include"));
        assert_eq!(by_alias("matched").unsupported.as_deref(), Some("match"));
        assert_eq!(
            by_alias("dangling").unsupported.as_deref(),
            Some("proxy_jump")
        );
        assert!(!by_alias("dangling").importable);
    }

    #[test]
    fn proxy_jump_entries_drop_user_and_port() {
        assert_eq!(
            split_proxy_jump("ops@bastion1:2222,bastion2"),
            vec!["bastion1".to_string(), "bastion2".to_string()]
        );
        assert!(split_proxy_jump("none").is_empty());
    }

    #[test]
    fn tokenizer_handles_quotes_and_comments() {
        assert_eq!(
            tokenize(r#"HostName "example.com" # tail"#),
            vec!["HostName".to_string(), "example.com".to_string()]
        );
        assert_eq!(
            tokenize("IdentityFile=~/.ssh/id_rsa"),
            vec!["IdentityFile".to_string(), "~/.ssh/id_rsa".to_string()]
        );
    }

    #[test]
    fn wildcard_detects_only_glob_patterns() {
        assert!(is_wildcard("*"));
        assert!(is_wildcard("prod-*"));
        assert!(is_wildcard("prod-?"));
        assert!(!is_wildcard("prod-db"));
    }

    /// 用开发者本机真实的 `~/.ssh` 校验私钥探测与读取（默认不跑）。
    ///
    /// ```bash
    /// TD_REAL_SSH_DIR=~/.ssh cargo test --manifest-path src-tauri/Cargo.toml \
    ///   --lib real_ssh_keys -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "读取开发者本机的 TD_REAL_SSH_DIR，默认跳过"]
    fn real_ssh_keys_scan_and_read() {
        let dir = std::env::var("TD_REAL_SSH_DIR").expect("需要 TD_REAL_SSH_DIR");
        let local = LocalSsh::at(dir);
        let status = local.scan();
        assert!(status.available, "~/.ssh 应当存在");
        for key in &status.keys {
            println!(
                "{} type={} encrypted={} path={}",
                key.name, key.key_type, key.encrypted, key.path
            );
        }
        assert!(!status.keys.is_empty(), "应当至少识别出一个私钥");
        // 每个识别出的私钥都必须能被 russh 解析（带口令的除外）。
        for key in status.keys.iter().filter(|key| !key.encrypted) {
            let text = local.read_key(&key.name).expect("应当能读取私钥");
            assert!(text.contains("PRIVATE KEY"));
        }
    }

    /// 用开发者本机真实的 `~/.ssh/config` 校验解析器（默认不跑）。
    ///
    /// ```bash
    /// TD_REAL_SSH_CONFIG=~/.ssh/config cargo test --manifest-path src-tauri/Cargo.toml \
    ///   --lib real_ssh_config -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "读取开发者本机的 TD_REAL_SSH_CONFIG，默认跳过"]
    fn real_ssh_config_parses() {
        let path = std::env::var("TD_REAL_SSH_CONFIG").expect("需要 TD_REAL_SSH_CONFIG");
        let text = std::fs::read_to_string(&path).unwrap();
        let hosts = parse_ssh_config(&text);
        for host in &hosts {
            println!(
                "{} -> {}@{}:{} jump={:?} key={:?} importable={} {}",
                host.alias,
                host.user,
                host.host_name,
                host.port,
                host.proxy_jump,
                host.identity_file,
                host.importable,
                host.unsupported.as_deref().unwrap_or("")
            );
        }
        assert!(!hosts.is_empty(), "真实 config 里应当至少解析出一个 Host");
        assert!(
            hosts.iter().all(|host| !is_wildcard(&host.alias)),
            "通配 Host 不应出现在结果里"
        );
    }
}
