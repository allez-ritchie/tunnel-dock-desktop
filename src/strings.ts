export type ThemeMode = "system" | "dark" | "light";

export const strings = {
  appName: "TunnelDock",
  appTagline: "SSH 本地隧道控制台",

  homeKicker: "SSH TUNNEL CONSOLE",
  homeOnline: "在线",
  homeStatOnline: "运行中",
  homeStatReconnect: "重连中",
  homeStatJumps: "跳板链路",
  homeConnectAll: "全部开启",
  homeDisconnectAll: "全部断开",
  homeSectionTunnels: "隧道",
  homeSectionHint: "本地端口 → 远程服务",
  homeAddTunnel: "新建隧道",
  homeSettings: "设置",
  homeEmptyTitle: "还没有隧道",
  homeEmptyBody: "创建第一条 SSH 本地转发，让远程服务像本地服务一样访问。",
  homeTunnelsCount: (count: number) => `${count} 条`,
  homeHeroEmpty: "创建隧道，从 localhost 访问远程服务",
  homeHeroIdle: "全部隧道已就绪，按需开启",
  homeHeroActive: (count: number) => `${count} 条隧道正在接管本地端口`,
  homeLocalLabel: "本地监听",
  homeRemoteLabel: "远程目标",
  homeReadyBadge: "本地优先 · 凭据加密",
  homeReadyHint: "关闭窗口后仍驻留托盘，隧道不受影响",
  homeBackgroundHint: "关闭窗口会隐藏到托盘；有隧道运行时请使用托盘菜单退出",

  cardOpenBrowser: "打开浏览器",
  cardConnect: "连接",
  cardDisconnect: "断开",
  cardRetry: "重试",
  cardEdit: "编辑",
  cardMore: "更多操作",
  cardCopyUrl: "复制本地地址",
  cardDelete: "删除隧道",
  cardCopied: "地址已复制",
  cardDirect: "直连",
  cardJumps: (count: number) => `${count} 级跳板`,
  cardAutoReconnect: "自动重连",
  cardKeepalive: (seconds: number) => `保活 ${seconds} 秒`,
  cardPhaseConnected: "运行中",
  cardPhaseConnecting: "连接中",
  cardPhaseReconnecting: "重连中",
  cardPhaseError: "连接失败",
  cardPhaseStopped: "已停止",
  cardRetryCount: (attempt: number) => `第 ${attempt} 次`,
  cardNextRetry: (seconds: number) => `${seconds} 秒后重试`,

  bulkDisconnectTitle: "断开全部隧道？",
  bulkDisconnectBody: (count: number) => `将断开 ${count} 条运行中或重连中的隧道。配置不会丢失。`,
  bulkDisconnectConfirm: "全部断开",
  deleteTitle: "删除隧道？",
  deleteBody: (name: string) => `“${name}”的配置将被永久删除，正在运行的连接会立即断开。`,
  deleteConfirm: "删除",
  dialogCancel: "取消",

  editorNew: "新建隧道",
  editorEdit: "编辑隧道",
  editorSave: "保存配置",
  editorSaving: "保存中…",
  editorBasic: "基础信息",
  editorName: "隧道名称",
  editorNameHint: "例如：家庭 NAS",
  editorLocalPort: "本地端口",
  editorLocalPortHint: "浏览器通过 127.0.0.1:端口 访问",
  editorRemoteHost: "远程主机",
  editorRemoteHostHint: "相对 SSH 服务器解析，可填 127.0.0.1",
  editorRemotePort: "远程端口",
  editorSshSection: "SSH 服务器",
  editorSshHost: "主机地址",
  editorSshHostHint: "域名或 IP",
  editorSshPort: "SSH 端口",
  editorUsername: "用户名",
  editorAuthPassword: "密码",
  editorAuthKey: "私钥",
  editorPassword: "登录密码",
  editorPasswordSaved: "密码已加密保存",
  editorSecretKeepHint: "留空则保持原值",
  editorClearSecret: "清除已保存值",
  editorPrivateKey: "私钥内容",
  editorPrivateKeyHint: "-----BEGIN OPENSSH PRIVATE KEY-----",
  editorPrivateKeySaved: "私钥已加密保存",
  editorPassphrase: "私钥口令（可选）",
  editorSecretNotice: "凭据使用 AES-256-GCM 加密，仅保存在本机",
  editorBrowserSection: "浏览器",
  editorBrowserUrl: "打开地址（可选）",
  editorBrowserUrlHint: "https://127.0.0.1:{port}/admin",
  editorBrowserUrlHelp:
    "留空默认打开 http://127.0.0.1:本地端口/；可用 {port} 引用本地端口。",
  editorJumpSection: "SSH 接力跳转",
  editorJumpHelp:
    "连接按顺序经过每一台跳板机，最后到达 SSH 服务器。最多支持 8 级。",
  editorDirect: "直连，无跳板机",
  editorAddJump: "添加跳板机",
  editorJumpMoveUp: "上移",
  editorJumpMoveDown: "下移",
  editorJumpEdit: "编辑跳板机",
  editorJumpDelete: "移除跳板机",
  editorAdvanced: "高级设置",
  editorAutoReconnect: "自动重连",
  editorAutoReconnectDesc: "连接中断后按指数退避自动恢复",
  editorAutoStart: "开机自动启动",
  editorAutoStartDesc: "登录系统后自动启动应用并恢复这条隧道",
  editorStrictHost: "校验主机密钥",
  editorStrictHostDesc: "首次连接记住指纹，后续变更时阻止连接",
  editorKeepalive: "保活间隔（秒）",
  editorKeepaliveDesc: "5–300 秒，发送 SSH keepalive 探测",
  editorTimeout: "连接超时（秒）",
  editorTimeoutDesc: "5–120 秒",
  editorDelete: "删除这条隧道",
  editorDiscardTitle: "放弃修改？",
  editorDiscardBody: "当前编辑内容尚未保存，返回后将丢失。",
  editorDiscardConfirm: "放弃",
  editorSaved: "配置已保存",
  editorRestarting: "配置已保存，正在按新配置重连",
  editorSaveFailed: (message: string) => `保存失败：${message}`,
  editorChainPreview: "链路",
  editorBack: "返回",

  jumpAddTitle: "添加跳板机",
  jumpEditTitle: "编辑跳板机",
  jumpLabel: "备注名（可选）",
  jumpLabelHint: "例如：公司堡垒机",
  jumpHost: "主机地址",
  jumpPort: "端口",
  jumpUsername: "用户名",
  jumpPassword: "密码",
  jumpPasswordSaved: "密码已加密保存",
  jumpPrivateKey: "私钥内容",
  jumpPrivateKeySaved: "私钥已加密保存",
  jumpPassphrase: "私钥口令（可选）",
  jumpSave: "保存跳板机",
  jumpHelp: "跳板机按链路顺序连接，凭据同样使用 AES-256-GCM 加密。",

  settingsTitle: "设置",
  settingsBackground: "后台常驻",
  settingsBackgroundDesc: "托盘 + 窗口隐藏 + 登录自启动",
  settingsTray: "关闭窗口时隐藏到托盘",
  settingsTrayBody:
    "窗口关闭后应用继续在托盘运行，隧道保持连接。需要退出时请使用托盘菜单的“退出”。",
  settingsAutostart: "登录自启动",
  settingsAutostartOn: "已启用（存在开机自动启动的隧道）",
  settingsAutostartOff: "未启用",
  settingsAutostartHint:
    "在隧道编辑页打开“开机自动启动”后，系统登录时会自动运行应用并恢复该隧道。",
  settingsAppearance: "外观",
  settingsAppearanceDesc: "跟随系统或强制深色 / 浅色",
  settingsThemeSystem: "跟随系统",
  settingsThemeDark: "深色",
  settingsThemeLight: "浅色",
  settingsHostKeys: "主机密钥",
  settingsHostKeysCount: (count: number) => `已固定 ${count} 个主机密钥`,
  settingsHostKeysDesc: "采用 TOFU 策略：首次连接自动信任，指纹变化时阻止连接。",
  settingsClearHosts: "清除已知主机",
  settingsClearHostsTitle: "清除所有已知主机？",
  settingsClearHostsBody:
    "下次连接时会重新信任服务器的主机密钥。请仅在确认服务器指纹变更时这样做。",
  settingsClearedHosts: "已清除已知主机密钥",
  settingsAbout: "关于",
  settingsVersion: (version: string) => `版本 ${version}`,
  settingsAboutBody:
    "TunnelDock 是一个本地优先的 SSH 端口转发工具。所有配置保存在设备上，凭据由本机主密钥加密。",
  settingsSecurity: "安全与隐私",
  settingsSecurityBody:
    "应用不收集遥测，不上传配置。SSH 主机密钥变化会被阻止。请仅转发你信任的远程服务。",
  settingsOss: "SSH 传输层由 russh 提供支持。",
  settingsDataHint: "配置与密钥保存在应用数据目录中，主密钥文件权限为 0600。",

  errorNameRequired: "请填写隧道名称",
  errorNameTooLong: "名称不能超过 40 个字符",
  errorLocalPort: "本地端口需在 1024–65535 之间",
  errorLocalPortInUse: (name: string) => `本地端口与“${name}”冲突`,
  errorRemoteHostRequired: "请填写远程主机",
  errorRemotePort: "远程端口需在 1–65535 之间",
  errorSshHostRequired: "请填写 SSH 主机地址",
  errorSshPort: "SSH 端口需在 1–65535 之间",
  errorUsernameRequired: "请填写 SSH 用户名",
  errorPasswordRequired: "请填写登录密码",
  errorPrivateKeyRequired: "请粘贴私钥内容",
  errorBrowserUrl: "地址需以 http:// 或 https:// 开头",
  errorKeepalive: "保活间隔需在 5–300 秒之间",
  errorTimeout: "连接超时需在 5–120 秒之间",
  errorJumpInvalid: "有跳板机配置不完整，请重新编辑",
  errorJumpHostRequired: "请填写跳板机地址",
  errorJumpPort: "跳板机端口需在 1–65535 之间",
  errorJumpUsernameRequired: "请填写跳板机用户名",
  errorJumpPasswordRequired: "请填写跳板机密码",
  errorJumpPrivateKeyRequired: "请粘贴跳板机私钥",
  errorMaxJumps: "最多支持 8 级跳板机",

  statusWaitingRestore: "等待恢复…",
  statusManualStop: "已手动断开",
  statusServiceStopped: "应用已退出",
  errorHostKeyChanged:
    "主机密钥已变更，连接被安全阻止。请确认服务器指纹后，在设置中清除已知主机再重试。",
  errorSecretDecrypt: "无法读取加密凭据，请重新编辑隧道并填写密码或私钥。",
  errorPortInUse: (port: number) => `本地端口 ${port} 被占用，请更换端口。`,
  errorAuthFailed: "SSH 认证失败，请检查用户名、密码或私钥。",
  errorUnknownHost: "无法解析 SSH 服务器地址。",
  errorRuntimeTimeout: "连接超时，请检查网络或服务器地址。",
  errorConnectionRefused: "服务器拒绝连接，请检查端口与服务状态。",
  errorLocalForwardLost: "本地转发已中断，正在尝试恢复。",
  errorConnectionClosed: "SSH 连接已断开。",
  errorGeneric: (detail: string) => `连接失败：${detail}`,
  errorNoBrowser: "没有找到可处理该地址的浏览器",

  toastDeleted: "配置已删除",
  toastNoBrowser: "无法打开浏览器",
} as const;

export function describeError(
  code: string | null | undefined,
  detail: string | null | undefined,
  localPort?: number | null,
): string {
  switch (code) {
    case "host_key_changed":
      return strings.errorHostKeyChanged;
    case "secret_decrypt":
      return strings.errorSecretDecrypt;
    case "port_in_use":
      return strings.errorPortInUse(localPort ?? 0);
    case "auth_failed":
      return strings.errorAuthFailed;
    case "unknown_host":
      return strings.errorUnknownHost;
    case "timeout":
      return strings.errorRuntimeTimeout;
    case "connection_refused":
      return strings.errorConnectionRefused;
    case "local_forward_lost":
      return strings.errorLocalForwardLost;
    case "connection_closed":
      return strings.errorConnectionClosed;
    default:
      return strings.errorGeneric(detail ?? "未知错误");
  }
}

