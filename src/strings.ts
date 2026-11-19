export type ThemeMode = "system" | "dark" | "light";

export const strings = {
  appName: "TunnelDock",
  dialogCancel: "取消",
  appTagline: "SSH 本地隧道控制台",

  tabApps: "应用",
  tabTunnels: "隧道",
  tabSettings: "设置",

  // ---------------------------------------------------------------- apps tab
  appsKicker: "FORWARDED APPS",
  appsTitle: "应用",
  appsSubtitle: "按应用组管理本地端口转发，点组标题可收起整组",
  appsStatForwarding: "转发中",
  appsStatGroups: "应用组",
  appsStatUnavailable: "不可用",
  appsAdd: "新建应用",
  appsEmptyTitle: "还没有应用",
  appsEmptyBody:
    "应用把本地端口转发到远程 IP:端口。先创建一条隧道，再添加应用即可。",
  appsEmptyCreateTunnel: "先创建隧道",
  appsGroupEmpty: "这个应用组还没有应用",
  appsGroupAdd: "添加到该组",
  appsGroupsSection: "应用组",
  appsNewGroup: "新建应用组",
  appsGroupForwarding: (count: number) => `${count} 个转发中`,
  appsGroupCount: (count: number) => `${count} 个应用`,
  appsGroupUnavailable: (count: number) => `${count} 个不可用`,
  appsGroupStart: "启动整组",
  appsGroupStop: "停止整组",
  appsGroupRename: "重命名应用组",
  appsGroupDelete: "删除应用组",
  appsGroupDefaultBadge: "默认",

  groupNameTitleNew: "新建应用组",
  groupNameTitleEdit: "重命名应用组",
  groupNameLabel: "应用组名称",
  groupNameHint: "例如：工作、家庭",
  groupNameSave: "保存",
  groupNameRequired: "请填写应用组名称",
  groupDeleteTitle: "删除应用组？",
  groupDeleteBody: (name: string, count: number) =>
    `“${name}”及其中的 ${count} 个应用将被删除，正在运行的转发会立即停止。`,
  groupDeleteEmptyBody: (name: string) => `“${name}”将被删除。`,
  groupDeleteConfirm: "删除应用组",

  appCardLocal: "本地",
  appCardRemote: "远程",
  appCardUnavailable: "不可用",
  appCardViewError: "查看错误详情",
  appCardAutoBoot: "开机启动",
  appCardStart: "启动",
  appCardStop: "停止",
  appCardEdit: "编辑",
  appCardDelete: "删除应用",
  appCardMore: "更多操作",
  appCardOpenBrowser: "打开浏览器",
  appCardCopyUrl: "复制地址",
  appCopied: "地址已复制",
  appDeleteTitle: "删除应用？",
  appDeleteBody: (name: string) =>
    `“${name}”的转发配置将被删除，正在运行的连接会立即断开。`,
  appDeleteConfirm: "删除应用",
  appPhaseIdle: "已停止",
  appPhaseConnecting: "连接中",
  appPhaseConnected: "转发中",
  appPhaseReconnecting: "重连中",
  appPhaseError: "不可用",
  appRetryCount: (attempt: number) => `第 ${attempt} 次`,
  appNextRetry: (seconds: number) => `${seconds} 秒后重试`,

  // ------------------------------------------------------------- tunnels tab
  tunnelsKicker: "SSH ACCESS",
  tunnelsTitle: "隧道",
  tunnelsSubtitle: "可复用的 SSH 接入信息，多个应用共用同一份配置",
  tunnelsAdd: "新建隧道",
  tunnelsEmptyTitle: "还没有隧道",
  tunnelsEmptyBody:
    "隧道保存 SSH 用户名、地址、端口与跳板机，应用只需引用它，无需重复输入。",
  tunnelCardEndpoint: "SSH 接入",
  tunnelCardChain: "链路",
  tunnelAppsUsing: (count: number) => `被 ${count} 个应用使用`,
  tunnelRunning: (count: number) => `${count} 个转发中`,
  tunnelNoApps: "还没有应用使用该隧道",
  tunnelJumpCount: (count: number) => `${count} 级跳板`,
  tunnelKeepalive: (seconds: number) => `保活 ${seconds} 秒`,
  tunnelTest: "测试连接",
  tunnelTesting: "测试中…",
  tunnelTestOk: (elapsedMs: number) => `连接正常 · ${elapsedMs} ms`,
  tunnelTestFail: "连接失败",
  tunnelTestSecurity: "安全告警",
  tunnelEdit: "编辑隧道",
  tunnelDelete: "删除隧道",
  tunnelEditConfirmTitle: "隧道正在被应用使用",
  tunnelEditConfirmBody: (name: string, count: number) =>
    `“${name}”正被 ${count} 个应用使用。保存后这些应用会停止（配置保留），需要重新启动。`,
  tunnelEditConfirm: "保存并停止应用",
  tunnelDeleteTitle: "删除隧道？",
  tunnelDeleteBody: (name: string, count: number) =>
    `“${name}”正被 ${count} 个应用使用。删除后这些应用会停止并标记为不可用，应用配置仍然保留。`,
  tunnelDeleteBodyUnused: (name: string) => `“${name}”的 SSH 配置将被删除。`,
  tunnelDeleteConfirm: "删除隧道",

  // --------------------------------------------------------- tunnel editor
  editorNewTunnel: "新建隧道",
  editorEditTunnel: "编辑隧道",
  editorSave: "保存配置",
  editorSaving: "保存中…",
  editorBasic: "基础信息",
  editorName: "隧道名称",
  editorNameHint: "例如：家庭 NAS",
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
  editorStrictHost: "校验主机密钥",
  editorStrictHostDesc: "首次连接记住指纹，后续变更时阻止连接",
  editorKeepalive: "保活间隔（秒）",
  editorKeepaliveDesc: "5–300 秒，发送 SSH keepalive 探测",
  editorTimeout: "连接超时（秒）",
  editorTimeoutDesc: "5–120 秒",
  editorDeleteTunnel: "删除这条隧道",
  editorDiscardTitle: "放弃修改？",
  editorDiscardBody: "当前编辑内容尚未保存，返回后将丢失。",
  editorDiscardConfirm: "放弃",
  editorTunnelSaved: "隧道已保存",
  editorTunnelSavedStopped: (count: number) =>
    `隧道已保存，${count} 个使用它的应用已停止`,
  editorSaveFailed: (message: string) => `保存失败：${message}`,
  editorChainPreview: "链路",
  editorBack: "返回",
  editorTunnelUsingHint: (count: number) =>
    `该隧道被 ${count} 个应用使用，保存后它们会停止并等待重新启动。`,

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

  // ------------------------------------------------------------ app editor
  appEditorNew: "新建应用",
  appEditorEdit: "编辑应用",
  appEditorBasic: "基本信息",
  appEditorName: "应用名称",
  appEditorNameHint: "例如：公司后台",
  appEditorGroup: "应用组",
  appEditorGroupHint: "用于分组管理与一键启停",
  appEditorTunnel: "隧道",
  appEditorTunnelHint: "选择保存好的 SSH 接入信息",
  appEditorNoTunnel: "还没有可用隧道，请先创建隧道。",
  appEditorCreateTunnel: "新建隧道",
  appEditorTunnelMissing: "原隧道已被删除，请重新选择。",
  appEditorRouteSection: "转发规则",
  appEditorLocalPort: "本地端口",
  appEditorLocalPortHint: "浏览器通过 127.0.0.1:端口 访问",
  appEditorRemoteHost: "远程主机",
  appEditorRemoteHostHint: "相对 SSH 服务器解析，可填 127.0.0.1",
  appEditorRemotePort: "远程端口",
  appEditorBrowserSection: "浏览器",
  appEditorBrowserUrl: "打开地址（可选）",
  appEditorBrowserUrlHint: "https://127.0.0.1:{port}/admin",
  appEditorBrowserUrlHelp:
    "留空默认打开 http://127.0.0.1:本地端口/；可用 {port} 引用本地端口。",
  appEditorAdvanced: "高级设置",
  appEditorAutoBoot: "开机自动启动",
  appEditorAutoBootDesc: "登录系统后自动启动这条转发",
  appEditorDelete: "删除这个应用",
  appEditorDiscardTitle: "放弃修改？",
  appEditorDiscardBody: "当前编辑内容尚未保存，返回后将丢失。",
  appEditorDiscardConfirm: "放弃",
  appEditorSaved: "应用已保存",
  appEditorSaveFailed: (message: string) => `保存失败：${message}`,
  appEditorPortInUse: (port: number) =>
    `本地端口 ${port} 已被其他应用使用，若同时启动会提示冲突。`,

  // ------------------------------------------------------------ error detail
  errorDetailTitle: "错误详情",
  errorDetailUnavailable: "应用不可用",
  errorDetailReason: "原因",
  errorDetailCode: "错误码",
  errorDetailTechnical: "技术细节",
  errorDetailTime: "发生时间",
  errorDetailTunnel: "使用隧道",
  errorDetailRoute: "转发规则",
  errorDetailRetry: "重试",
  errorDetailEdit: "编辑应用",
  errorDetailBack: "返回应用列表",
  errorDetailNoDetail: "没有更多技术细节。",
  errorDetailTunnelMissing: "隧道已删除",
  errorDetailMissingApp: "应用不存在或已被删除。",
  errorDetailBlockedHint:
    "该应用当前不可用，错误原因会保留在这里，直到应用重新启动成功。",

  // ---------------------------------------------------------------- settings
  settingsTitle: "设置",
  settingsBackground: "后台常驻",
  settingsBackgroundDesc: "托盘 + 窗口隐藏 + 登录自启动",
  settingsTray: "关闭窗口时隐藏到托盘",
  settingsTrayBody:
    "窗口关闭后应用继续在托盘运行，转发保持连接。需要退出时请使用托盘菜单的“退出”。",
  settingsAutostart: "登录自启动",
  settingsAutostartOn: "已启用（存在开机自动启动的应用）",
  settingsAutostartOff: "未启用",
  settingsAutostartHint:
    "在应用编辑页打开“开机自动启动”后，系统登录时会自动运行应用并启动该转发。",
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
  settingsData: "数据与配置",
  settingsDataDesc: "所有配置只保存在本机，不会上传",
  settingsDataCounts: (tunnels: number, apps: number, groups: number) =>
    `隧道 ${tunnels} 个 · 应用 ${apps} 个 · 应用组 ${groups} 个`,
  settingsAbout: "关于",
  settingsVersion: (version: string) => `版本 ${version}`,
  settingsAboutBody:
    "TunnelDock 是一个本地优先的 SSH 端口转发工具。所有配置保存在设备上，凭据由本机主密钥加密。",
  settingsSecurity: "安全与隐私",
  settingsSecurityBody:
    "应用不收集遥测，不上传配置。SSH 主机密钥变化会被阻止。请仅转发你信任的远程服务。",
  settingsOss: "SSH 传输层由 russh 提供支持。",
  settingsDataHint: "配置与密钥保存在应用数据目录中，主密钥文件权限为 0600。",

  // ------------------------------------------------------------ validation
  errorNameRequired: "请填写隧道名称",
  errorNameTooLong: "名称不能超过 40 个字符",
  errorSshHostRequired: "请填写 SSH 主机地址",
  errorSshPort: "SSH 端口需在 1–65535 之间",
  errorUsernameRequired: "请填写 SSH 用户名",
  errorPasswordRequired: "请填写登录密码",
  errorPrivateKeyRequired: "请粘贴私钥内容",
  errorKeepalive: "保活间隔需在 5–300 秒之间",
  errorTimeout: "连接超时需在 5–120 秒之间",
  errorJumpInvalid: "有跳板机配置不完整，请重新编辑",
  errorJumpHostRequired: "请填写跳板机地址",
  errorJumpPort: "跳板机端口需在 1–65535 之间",
  errorJumpUsernameRequired: "请填写跳板机用户名",
  errorJumpPasswordRequired: "请填写跳板机密码",
  errorJumpPrivateKeyRequired: "请粘贴跳板机私钥",
  errorMaxJumps: "最多支持 8 级跳板机",
  errorAppNameRequired: "请填写应用名称",
  errorAppNameTooLong: "应用名称不能超过 40 个字符",
  errorGroupRequired: "请选择应用组",
  errorTunnelRequired: "请选择隧道",
  errorLocalPort: "本地端口需在 1024–65535 之间",
  errorRemoteHostRequired: "请填写远程主机",
  errorRemotePort: "远程端口需在 1–65535 之间",
  errorBrowserUrl: "地址需以 http:// 或 https:// 开头",

  // ----------------------------------------------------------------- errors
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
  errorTunnelDeleted: "应用使用的隧道已被删除，请编辑应用并重新选择隧道。",
  errorGroupMissing: "应用所属的应用组不存在，请编辑应用重新选择分组。",
  errorTunnelDown: "隧道连接失败，请检查网络、SSH 地址与凭据。",
  errorForwardFailed: "本地转发建立失败，请检查远程目标与本机端口。",
  errorPortConflict: "本地端口已被其他正在运行的应用占用，请更换端口或先停止冲突应用。",
  errorGeneric: (detail: string) => `连接失败：${detail}`,

  toastDeleted: "已删除",
  toastTunnelDeleted: (stoppedApps: number) =>
    stoppedApps > 0
      ? `隧道已删除，${stoppedApps} 个应用已停止`
      : "隧道已删除",
  toastGroupDeleted: "应用组及组内应用已删除",
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
    case "port_conflict":
      return strings.errorPortConflict;
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
    case "tunnel_deleted":
      return strings.errorTunnelDeleted;
    case "group_missing":
      return strings.errorGroupMissing;
    case "tunnel_down":
      return strings.errorTunnelDown;
    case "forward_failed":
      return strings.errorForwardFailed;
    default:
      return strings.errorGeneric(detail ?? "未知错误");
  }
}
