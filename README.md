# TunnelDock Desktop

一个纯本地优先的 **SSH 本地端口转发（`ssh -L`）** 桌面客户端，是 Android 版
[TunnelDock](../tunnel-dock) 的等价实现。把远程服务器上的 `ip:port` 通过 SSH 转发到本机
`127.0.0.1:port`，本地浏览器或任意程序即可像访问本地服务一样访问远程服务。

```
浏览器 / 其他程序
        │  http://127.0.0.1:18080
        ▼
   TunnelDock（本地监听）
        │  SSH 隧道（可多级跳板）
        ▼
   跳板机 A → 跳板机 B → SSH 服务器 → 远程服务 10.0.0.5:5000
```

配置分为三层：

- **隧道**：可复用的 SSH 接入信息，包含**一组有序的 SSH 线路**（每条线路自己的
  用户名 / 地址 / 端口 / 跳板 / 凭据），可被多个应用引用；
- **应用**：一条转发规则（本地端口 → 远程 IP:端口），引用一个隧道、归属一个应用组；
- **应用组**：应用的分组，用于收起/展开与一键启动、停止整组应用。

隧道是**配置复用**，不是运行时共享：每个启动的应用都会用所选隧道的配置独立建立
自己的 SSH 会话与本地转发，互不影响、互不拖累。

**多线路**解决的是“同一个服务、不同场景、不同 SSH 入口”：例如家里走局域网直连、
外出走 frpc 反向代理，而依赖该服务的第三方程序只认 `127.0.0.1:18321` 这个固定端口。
端口归属于**服务**（应用），SSH 入口归属于**线路**，因此换网络时本地端口保持不变，
第三方完全无感。启动应用时按「首选 → 上次成功 → 声明顺序」逐条拨号，第一条通了就不再试
后面的；只有全部失败才进入指数退避重连。

技术栈：**Rust**（Tokio + `russh`）+ **Tauri 2** + **TypeScript**（React + Vite）。

## 功能

| 能力 | 说明 |
| --- | --- |
| 隧道复用 | 隧道保存 SSH 用户名 / 地址 / 端口 / 跳板，多个应用引用同一份配置，无需重复输入 |
| 多线路入口 | 一条隧道可配多条 SSH 线路，按场景自动切换，应用本地端口不变 |
| 仅局域网线路 | 家里那台服务器可标记为“仅局域网”，不在局域网时自动跳过，外出时直接走外网入口 |
| 线路自动回退 | 首选线路连不上时自动按顺序尝试下一条，卡片显示当前线路并提示“已切换” |
| 独立会话 | 每个运行中的应用各自建立 SSH 会话与本地转发，单个应用故障不影响其他应用 |
| 应用与应用组 | 应用 = 一条 `本地端口 → 远程 IP:端口` 转发；应用组支持一键启动 / 停止整组 |
| 左侧导航 | 「应用」「隧道」「设置」三页竖排在左侧；应用页按组折叠展示，收起时只显示转发中数量 |
| 隧道连接测试 | 隧道卡片可单独发起连通性测试，逐条线路验证认证、跳板与主机密钥并报告每条线路的结果，不影响运行中的应用 |
| 不可用提示 | 隧道被删除或连接失败时，应用显示红色叹号，并可打开错误详情页查看原因与技术细节 |
| SSH 接力跳转 | ProxyJump 式多级跳板（每条线路最多 8 级），每级独立凭据 |
| 跳板凭据复用 | 跳板机与某条已有隧道是同一 `用户@主机:端口` 时，自动复用该隧道的密码，无需逐条重输 |
| 隧道信息遮蔽 | 应用卡片上的 `用户@主机:端口` 默认隐藏，点击整体显示 / 隐藏 |
| 配置持久化 | 本地 JSON 原子写入，应用重启后自动恢复上一次的运行意图；旧版单层配置自动迁移 |
| 凭据加密 | 密码 / 私钥 / 口令经 AES-256-GCM 加密后落盘，主密钥文件权限 `0600` |
| 主机密钥校验 | TOFU 策略：首次信任并固定指纹，指纹变更时**阻止连接**并提示 |
| 托盘常驻 | 关闭窗口隐藏到托盘，托盘菜单提供显示窗口、全部断开、退出 |
| 登录自启动 | 应用启用“开机自动启动”后，随系统登录自动运行应用并启动这条转发 |
| 隧道保活 | 可配置 keepalive 间隔，SSH 层探测 + 本地监听存活检查 |
| 自动重连 | 指数退避重连（1s→30s），网络恢复时立即重试 |
| 一键跳转浏览器 | 连通后直接打开 `http://127.0.0.1:端口`，支持自定义 URL 与 `{port}` 占位符 |
| 配置增删改 | 隧道、应用、应用组均可增删改；线路与跳板机可增删、上下移动排序，线路可设为首选 |
| 变更保护 | 隧道被应用引用时，删除或修改需要二次确认；确认后会先停止相关应用（应用本身保留） |
| 复用本机私钥 | 隧道与跳板机可直接选用 `~/.ssh` 下的私钥；前端只传文件名，密钥由后端读取并加密落盘 |
| 导入 ssh config | 一键读取 `~/.ssh/config`，勾选主机批量导入为隧道，自动还原 `HostName`/`User`/`Port`/`ProxyJump` |
| 配置包导出导入 | 应用组、应用、隧道导出为 `.zip`；导入时按名称合并到现有配置，可跨设备迁移 |

## 构建

环境要求：Node.js 20+、Rust 1.85+，Linux 需要 Tauri 运行依赖：
`libwebkit2gtk-4.1-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`libssl-dev`、
`build-essential`、`pkg-config` 等。

Windows 还需要 **MSVC 生成工具**（Visual Studio Build Tools，含 C++ 工作负载）。
`russh` 通过 `aws-lc-rs` 引入 `aws-lc-sys`，后者在 Windows x86-64 上编译汇编文件需要
NASM 汇编器；本项目已在 `src-tauri/Cargo.toml` 中为 `aws-lc-rs` 打开 `prebuilt-nasm`
特性，未安装 NASM 时会自动回退到 crate 内置的预编译汇编对象，因此**无需单独安装 NASM**。
如果环境里装了 NASM，构建仍会优先使用它。

Windows 打包/构建还需要 `src-tauri/icons/icon.ico`（用于生成 Windows 资源文件）。
该文件由 `npm run tauri icon src-tauri/icons/icon.png` 从现有 PNG 生成，已随仓库提供；
更换应用图标后请重新执行该命令，不要手写 `.ico`。

```bash
npm install

# 前端类型检查 + 生产构建
npm run build

# 开发运行（需要图形桌面环境）
npm run tauri dev

# 打包安装包
npm run tauri build
```

后端校验：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

## 快速上手

1. 打开应用，左侧切到「隧道」，点“新建隧道”，填写：
   - 隧道名称；
   - 点“添加线路”，在对话框里填写 SSH 主机、端口、用户名，以及密码或私钥；
   - 需要跳板机时在该线路对话框里展开“SSH 接力跳转”，按链路顺序添加（第 1 跳最靠近本机）；
   - 同一台服务器有多个入口时再点一次“添加线路”，用 ⭐ 把常用的一条设为首选；
   - 保存后可在隧道卡片上点“测试连接”，逐条线路验证认证、跳板与主机密钥。
2. 底部切到「应用」，点“新建应用”，选择：
   - 应用组（默认 `default`，也可先新建自己的组）；
   - 隧道；
   - 本地端口，例如 `18080`；
   - 远程 IP 与端口，例如 `10.0.0.5` / `5000`（相对 SSH 服务器解析，本机服务写 `127.0.0.1`）。
3. 在应用卡片上点“启动”，状态变为“转发中”即可用浏览器打开 `http://127.0.0.1:18080`。
4. 应用按应用组展示：点组标题可收起/展开，收起时只显示该组“N 个转发中”；
   组标题上的 ▶ / ■ 按钮可一键启动或停止整组。
5. 不使用应用组的场景下，所有应用默认放在名为 `default` 的组里，该组不可删除。
6. 关闭窗口默认隐藏到托盘；需要退出时使用托盘菜单的“退出”。

### 复用本机 `~/.ssh`

- 隧道编辑页的「私钥」认证方式下，可直接从「本机私钥」下拉框选择 `~/.ssh` 中的私钥
  （例如 `id_ed25519`），不需要把密钥内容粘贴到界面。前端只把**文件名**交给后端，
  后端读取文件后立即用主密钥加密落盘，密钥内容不会进入前端状态、日志或导出的配置包。
- 「隧道」页右上角的「导入 ~/.ssh/config」会解析本机 OpenSSH 配置，列出所有具体
  `Host` 条目，勾选后批量创建隧道：`HostName`、`User`、`Port`、`ProxyJump`（按链路
  展开为多级跳板）都会被还原；`IdentityFile` 指向 `~/.ssh` 内存在的私钥时会自动带入。
- 同名隧道不会被重复创建；`Host *` 之类的通配块只作为默认值补全，不会单独导入。
- 以下写法暂不支持导入（列表中会标黄并说明原因），需要手动创建隧道：
  `ProxyCommand`、`Include`、`Match`，以及 `ProxyJump` 里的主机没有在 config 中定义。
- 私钥带口令时，`~/.ssh` 里的文件本身是加密的，导入后请在隧道编辑页填写「私钥口令」。
- 该功能只读取 `~/.ssh`，从不修改其中的任何文件。

### 配置包导出 / 导入

「设置」页的「配置包」卡片可以把**应用组 + 应用 + 隧道**导出为一个 `.zip`：

- 压缩包内是 `tunneldock-export.json`（UTF-8 明文清单，方便排错）和 `README.txt`；
- 实体之间按**名称**引用，因此换一台设备导入后引用关系仍然成立；
- 私钥以明文写入清单（便于跨设备还原），请像保管私钥一样保管该文件；
  **密码认证的隧道只导出定义、不导出密码**，界面会提示导入后需要重新填写；
- 不含运行状态（`desiredOn`），导入后不会自动启动任何转发。

导入时会先弹出预览确认，再按名称合并到现有配置：

- 隧道按 `主机 + 端口 + 用户名` 去重，已存在时复用；应用按「同组同名」去重；
- 引用不到隧道或应用组的应用会被丢弃，并在结果里报告数量；
- 与现有应用撞了本地端口的应用仍会导入，但会列出需要改端口的名字。

## 数据位置

配置与密钥保存在 Tauri 应用数据目录：

- Linux：`~/.local/share/com.tunneldock.desktop/`
- macOS：`~/Library/Application Support/com.tunneldock.desktop/`
- Windows：`%APPDATA%\com.tunneldock.desktop\`

其中 `tunnels.json` 为一份原子写入的配置文档，包含 `tunnels`（隧道）、`apps`（应用）、
`groups`（应用组）三个数组，凭据以密文保存；`known_hosts.json` 为固定的主机密钥，
`master.key` 为 AES-256-GCM 主密钥（Linux/macOS 下权限 `0600`）。

旧版本的 `tunnels.json` 会在首次启动时自动迁移：
- **v1**（单层隧道数组）：SSH 接入信息相同的配置合并为一个隧道，每条旧配置拆成一条应用，
  并全部放入 `default` 组；
- **v2**（单条 SSH 接入信息直接存放在隧道上）：原样升级为“一条线路，名字取隧道名”，
  行为与升级前完全一致，直到你添加第二条线路。

> 迁移靠文档里的 `version` 字段驱动：所有字段都有默认值，因此**不能**直接用当前结构去
> 解码旧文档——那样不会报错，只会静默丢掉 `host`/`port`/`username`，得到一堆空隧道。

## 安全说明

- 所有配置与凭据只保存在本机；应用不收集遥测、不上传数据。
- SSH 密码、私钥、私钥口令使用应用数据目录中的随机主密钥做 AES-256-GCM 加密，
  明文只在建立连接时短暂存在内存中。
- 主机密钥采用 TOFU：首次连接自动信任并固定，后续指纹不匹配会直接拒绝连接，
  需要在设置中“清除已知主机”后才能重新信任。
- 日志不输出密码、私钥与完整主机密钥。
- 本地监听默认绑定 `127.0.0.1`，不会暴露到局域网。
- 删除隧道或应用组、修改被引用的隧道都属于破坏性操作，界面会二次确认；确认后
  先停止相关应用，再落盘配置。
- 读取 `~/.ssh` 是只读操作：导入私钥时前端只传文件名，后端读取后立即加密；
  私钥文件路径做了白名单校验（必须是 `~/.ssh` 下的普通文件，拒绝 `../` 之类的路径）。
- 导入配置包会校验清单的 `kind` 与 `version`，并限制解压后的清单大小，避免被构造的
  压缩包撑爆内存；密码不会进入配置包。

## 目录结构

```
src/                          # React + TypeScript 前端
├── App.tsx                   # 页面栈、左侧导航、状态订阅、全局操作
├── api.ts                    # 唯一的 Tauri 命令封装
├── strings.ts                # 所有用户可见文案
├── draft.ts                  # 隧道 / 应用表单状态与校验
├── components/ui.tsx         # 通用展示组件、图标与左侧导航
├── components/LocalKeyPicker.tsx # 从 ~/.ssh 选择私钥
├── screens/
│   ├── AppsScreen.tsx        # 应用组 + 可折叠应用卡片
│   ├── TunnelsScreen.tsx     # 隧道列表与连接测试
│   ├── AppEditorScreen.tsx   # 应用编辑器
│   ├── TunnelEditorScreen.tsx# 隧道编辑器：线路列表、线路与跳板机对话框
│   ├── SshConfigImportScreen.tsx # 导入 ~/.ssh/config
│   ├── BundleCard.tsx        # 配置包导出 / 导入
│   ├── ErrorDetailScreen.tsx # 应用不可用详情
│   └── SettingsScreen.tsx    # 设置 / 版本 / 数据统计
└── styles.css                # 设计令牌与布局

src-tauri/src/
├── lib.rs                    # 应用装配、托盘、窗口事件
├── model.rs                  # Tunnel / ForwardApp / AppGroup 与状态模型
├── store.rs                  # 配置文档 JSON 持久化（原子写入）与旧数据迁移
├── secrets.rs                # AES-256-GCM 凭据加解密
├── known_hosts.rs            # TOFU 主机密钥仓库
├── local_ssh.rs              # 只读探测 ~/.ssh 私钥并解析 ~/.ssh/config
├── portable.rs               # 配置包（.zip）的导出、预览与合并导入
├── ssh.rs                    # russh 连接、跳板链、按应用转发与连接测试
├── runtime.rs                # 按应用监督、保活、重连、组启停、状态广播
└── commands.rs               # Tauri 命令
```

## 测试

三层测试，全部可离线运行（端到端层需要一个本机 sshd）。

### 1. 前端单元测试

```bash
npm test          # vitest run
```

覆盖 `src/draft.ts` 的隧道 / 应用表单校验、端口推导、凭据“留空保持 / 空串清除 / 新值替换”
语义、跳板序列化，以及 `src/strings.ts` 的错误码与文案映射。纯逻辑、无需浏览器。

### 2. Rust 单元与集成测试

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

包含模型归一化、配置文档持久化、旧版配置迁移、凭据加解密、TOFU 主机密钥、重连退避、
按应用的启动前校验（隧道/应用组存在、本地端口冲突），以及 `commands.rs` 的入参校验
（端口范围、跳板数量上限、应用组与隧道引用、凭据密文保留语义）。

另外还有两组针对真实文件的用例（默认忽略，需要显式开启）：

```bash
# 用本机真实的 ~/.ssh/config 校验解析器，会打印每条 Host 的解析结果
TD_REAL_SSH_CONFIG=~/.ssh/config cargo test --manifest-path src-tauri/Cargo.toml \
  --lib real_ssh_config -- --ignored --nocapture

# 用本机真实的 ~/.ssh 校验私钥探测与读取
TD_REAL_SSH_DIR=~/.ssh cargo test --manifest-path src-tauri/Cargo.toml \
  --lib real_ssh_keys -- --ignored --nocapture

# 生成真实配置包，供 unzip / python zipfile 等外部工具校验
TD_ZIP_DIR=/tmp/zipcheck cargo test --manifest-path src-tauri/Cargo.toml \
  --lib real_zip -- --ignored --nocapture
```

配置包相关的常规用例覆盖：zip 往返后实体与私钥口令完整、重复导入按端点去重、
密码型隧道只导出定义、悬空引用被丢弃、外来/更高版本压缩包被拒绝、
外部工具重新打包后仍能识别，以及缺少清单或非 zip 文件的报错。

### 3. 端到端隧道测试（忽略标记，需真实 sshd）

这些用例在 `ssh.rs` 里带 `#[ignore]`，默认不跑。先准备一台测试 sshd：

```bash
# 公钥登录的 sshd（端口 2222）与 HTTP 服务（端口 8080）
TD_E2E_KEY=~/.ssh/id_ed25519 \
  cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_forward -- --ignored

# 多级跳板
TD_E2E_KEY=~/.ssh/id_ed25519 TD_E2E_WITH_JUMP=1 \
  cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_forward -- --ignored

# 只认证、不建本地监听的隧道连接测试
TD_E2E_KEY=~/.ssh/id_ed25519 \
  cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_probe_authenticates_without_forward -- --ignored

# 主机密钥变更必须被拒（会临时改写临时目录里的 known_hosts.json）
TD_E2E_KEY=~/.ssh/id_ed25519 \
  cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_rejects_changed_host_key -- --ignored

# 认证失败 / 本地端口被占用
TD_E2E_KEY=~/.ssh/id_ed25519 TD_E2E_BAD_KEY=~/.ssh/unauthorized_key \
  cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored \
  --skip e2e_password_auth --test-threads=1
```

密码认证需要一台允许 `PasswordAuthentication` 且 `AllowTcpForwarding yes` 的 sshd：

```bash
TD_E2E_HOST=127.0.0.1 TD_E2E_PORT=2222 TD_E2E_USER=test \
TD_E2E_PASSWORD=... TD_E2E_REMOTE_HOST=127.0.0.1 TD_E2E_REMOTE_PORT=8080 \
  cargo test --manifest-path src-tauri/Cargo.toml --lib e2e_password_auth -- --ignored
```

可用环境变量：`TD_E2E_HOST`、`TD_E2E_PORT`、`TD_E2E_USER`、`TD_E2E_KEY`、
`TD_E2E_BAD_KEY`、`TD_E2E_PASSWORD`、`TD_E2E_REMOTE_HOST`、`TD_E2E_REMOTE_PORT`、
`TD_E2E_EXPECT`（默认 `TUNNELDOCK_E2E_MARKER_42`）、`TD_E2E_WITH_JUMP`。

## 与 Android 版的差异

- 常驻能力由**托盘 + 窗口隐藏**承担，而不是前台服务与常驻通知。
- “登录自启动”通过 Tauri autostart 插件注册系统登录项，登录后恢复启用了
  `autoStartOnBoot` 的应用。
- 凭据加密使用应用数据目录中的随机主密钥文件（`0600`），而不是 Android Keystore；
  主密钥不会离开本机。
- 桌面端保留托盘菜单的“全部断开”作为紧急停止入口；界面内的批量操作按应用组进行。

## 许可

SSH 传输层由 [russh](https://github.com/Eugeny/russh) 提供。
