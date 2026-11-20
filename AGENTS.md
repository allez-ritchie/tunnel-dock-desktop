# AGENTS.md

本文件面向在本仓库中工作的 AI 编程助手（以及人类协作者）。**开始任何工作前必须先读本文件**；
修改代码、文档或配置后，必须按本文件的 Git 约定及时提交。

---

## 1. 仓库作用

本仓库是 **TunnelDock Desktop** 的桌面客户端源码，是 Android 版
`tunnel-dock` 的等价实现。TunnelDock 是一个纯本地优先的 **SSH 本地端口转发
（`ssh -L`）** 工具，核心能力：

- 把远程服务器上的 `host:port` 通过 SSH 转发到本机 `127.0.0.1:port`，让本地浏览器或
  其他程序直接以 `localhost` 访问远程服务；
- 配置分三层：**隧道**（可复用的 SSH 接入信息：用户名/地址/端口/跳板/凭据）、
  **应用**（一条 `本地端口 → 远程 IP:端口` 转发规则，引用一个隧道并归属一个应用组）、
  **应用组**（应用分组，支持一键启停整组）；
- 支持**SSH 接力跳转（ProxyJump 式多级跳板，最多 8 级）**、单个/整组开关、隧道连接测试；
- 配置持久化（本地 JSON + 原子写入）、凭据加密（AES-256-GCM 主密钥，文件权限 `0600`）、
  主机密钥固定（TOFU）；
- 托盘常驻、登录自启动、隧道保活、指数退避自动重连；
- 隧道连通后一键跳转浏览器，自定义打开地址支持 `{port}` 占位符。

技术栈：**Rust**（Tokio + `russh`）+ **Tauri 2** + **TypeScript**（React + Vite）。
桌面端没有 Android 前台服务的概念，运行时监督器直接运行在 Tauri 的 Tokio 运行时中；
托盘与窗口隐藏承担“后台常驻”的职责。

---

## 2. 目录结构

| 路径 | 作用 |
| --- | --- |
| `src/` | 前端（React + TypeScript）。`App.tsx` 负责页面栈与底部导航，`api.ts` 是唯一的 Tauri 命令封装入口。 |
| `src/screens/` | `AppsScreen`（应用组折叠）、`TunnelsScreen`、`AppEditorScreen`、`TunnelEditorScreen`、`ErrorDetailScreen`、`SettingsScreen`。 |
| `src/components/` | 可复用的展示组件（状态点、徽标、警示框、表单字段等）。 |
| `src/strings.ts` | **所有用户可见文案**集中定义，禁止在组件里散落硬编码中文/英文。 |
| `src/styles.css` | 设计令牌（颜色、圆角、字体）与布局样式，配色统一在此维护。 |
| `src-tauri/src/main.rs` | Tauri 可执行入口，只做 `tunnel_dock_lib::run()` 调用。 |
| `src-tauri/src/lib.rs` | 应用装配：注册命令、托盘、窗口事件、启动时恢复运行意图。 |
| `src-tauri/src/model.rs` | `TunnelConfig` / `ForwardApp` / `AppGroup` / `JumpHost` / `SshCredentials` / `AppStatus` 等模型与视图。 |
| `src-tauri/src/store.rs` | 隧道 / 应用 / 应用组配置文档的 JSON 持久化（原子写入）、归一化与旧版数据迁移。 |
| `src-tauri/src/secrets.rs` | AES-256-GCM 凭据加解密，主密钥落盘为 `0600` 文件。 |
| `src-tauri/src/known_hosts.rs` | TOFU 主机密钥仓库，指纹首次固定、变更即拒绝。 |
| `src-tauri/src/ssh.rs` | `russh` 连接、跳板链、按应用的 `direct-tcpip` 本地监听与双向转发、无转发的连通性探测。 |
| `src-tauri/src/runtime.rs` | 按应用监督：连接生命周期、保活、重连、组启停、隧道连接测试、状态广播。 |
| `src-tauri/src/commands.rs` | 暴露给前端的 `#[tauri::command]`，只做参数校验与转调。 |
| `src-tauri/tauri.conf.json` | 窗口、打包、安全策略配置（共享文件，见 4.1）。 |
| `src-tauri/capabilities/` | Tauri 2 权限清单，新增插件能力时在此声明。 |

日志通过 `tracing` 输出，敏感字段一律脱敏。

---

## 3. 构建、测试与验证

环境要求：Node.js 20+、Rust 1.85+、Linux 需要 `libwebkit2gtk-4.1-dev`、
`libayatana-appindicator3-dev`、`librsvg2-dev`、`libssl-dev` 等 Tauri 系统依赖。
Windows 需要 MSVC 生成工具；`aws-lc-sys` 在 Windows x86-64 上原本要求 NASM，已在
`src-tauri/Cargo.toml` 为 `aws-lc-rs` 开启 `prebuilt-nasm` 特性，未装 NASM 时回退到
crate 内置的预编译汇编对象，不要为绕过构建失败而安装/依赖本机 NASM。Windows 构建还依赖
`src-tauri/icons/icon.ico`（由 `npm run tauri icon` 从 PNG 生成，需随仓库提交）。

```bash
# 安装前端依赖
npm install

# 前端类型检查 + 生产构建（最快的 UI 回归）
npm run build

# 前端逻辑单元测试（隧道/应用表单校验、凭据语义、错误码与文案映射）
npm test

# Rust 编译检查（最快的后端语法/类型检查）
cargo check --manifest-path src-tauri/Cargo.toml

# Rust 单元 + 集成测试（模型归一化、旧数据迁移、重连退避、密文格式、命令入参校验）
cargo test --manifest-path src-tauri/Cargo.toml

# 端到端隧道测试（带 #[ignore]，需要本机 sshd 与 HTTP 服务，见 README「测试」）
TD_E2E_KEY=/path/to/key cargo test --manifest-path src-tauri/Cargo.toml \
  --lib -- --ignored

# 开发运行（需要图形桌面环境）
npm run tauri dev

# 打包安装包
npm run tauri build
```

补充说明：

- 不能提交无法编译的代码；提交前至少保证 `npm run build` 与 `cargo check` 通过。
- 不能把调试代码、临时文件、密钥/令牌、构建产物提交到仓库。
- 首次 `npm install` / `cargo build` 需要联网下载依赖。若 crates 源被镜像替换导致
  版本锁定异常，使用本机配置的镜像即可，**不要把本机镜像配置提交到仓库**。
- Linux 打包依赖窗口系统库；无图形环境时以 `npm run build` + `cargo test` 作为验收基线。

### 3.1 手工端到端验证

隧道是网络功能，改完 `ssh.rs`、`runtime.rs`、凭据相关代码后建议做一次端到端验证：

1. 本机启动 sshd 与 HTTP 服务（例如 `sshd -f <临时配置> -D -p 2222`、
   `python3 -m http.server 8080`）。
2. 启动 `npm run tauri dev`，先在「隧道」页新建隧道：SSH `127.0.0.1:2222`、用户名与密码，
   点“测试连接”应显示成功；再到「应用」页新建应用：选择该隧道、本地端口 `18080`、
   远程 `127.0.0.1:8080`，应用组用 `default`。
3. 验证点：`ss -ltn` 能看到 `127.0.0.1:18080`；`curl http://127.0.0.1:18080/`
   返回远程内容；停掉 sshd 后应用卡片进入“重连中”并按 1s→30s 退避重试，恢复后自动回到
   “转发中”；停止应用后本地监听端口消失。
4. 分组验证：新建一个应用组并把应用移入，点组标题收起后只显示“N 个转发中”；
   组标题上的 ▶ / ■ 可一键启动/停止整组。
5. 变更保护验证：删除或修改仍被应用引用的隧道，应先弹出二次确认；确认后相关应用停止，
   删除隧道时应用标记为不可用并出现红色叹号，点开可查看错误详情。
6. 跳板链验证：至少两级 sshd（`GatewayPorts` 无需开启），确认连接按链路顺序建立，
   主机密钥各自固定在自己的条目下。
7. 主机密钥变更验证：换掉服务器主机密钥后重连，应进入“连接失败”并带安全提示，
   且**停止自动重连**，不被无限重试。

---

## 4. 多 AI 会话协作与交接方式

同一工作目录下可能有多个 AI 会话（Lead 与若干 teammate）并行工作，**共享同一个文件系统**，
彼此的修改立即可见。遵循以下约定可避免互相覆盖和重复劳动：

### 4.1 分工与写入边界

- 按**垂直切片**拆分任务（一个功能从数据到 UI 端到端交给一个负责人），而不是按“层”拆分。
- 每个任务都要在共享任务板登记**写入范围（write scopes）**；只能修改自己范围内的文件。
- 以下**共享文件**默认只由 Lead（或任务指定的唯一负责人）修改，其他会话不要直接改：
  `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、
  `src-tauri/capabilities/default.json`、`src/main.tsx`、`src/App.tsx`、
  `src/api.ts`、`src/strings.ts`、`src/styles.css`、`src-tauri/src/lib.rs`、
  `src-tauri/src/model.rs`。
  功能模块的文案请先在自己的组件内以常量形式组织，并在评审时合并到 `src/strings.ts`。
- 需要改共享文件时，通过消息/任务说明需求，由负责人统一修改，避免并发编辑冲突。
- 文件操作发生冲突时：重新读取最新内容，在最新内容上重做改动，不要强推旧版本。

### 4.2 交接记录（Handoff）

每次会话结束、切换负责人或遇到阻塞时，在 `.agents/handoff/` 下写交接文件：

```
.agents/handoff/<yyyy-MM-dd>-<任务 slug>.md
```

必须包含：目标与背景、当前状态（已完成 / 进行中 / 未开始）、改动文件清单与关键设计决策、
待办事项（可执行到文件与函数级别）、验证命令与最近一次结果、风险与坑点。

- **`.agents/handoff/` 下的交接文档只保留在本地，不提交到 git**：它们是会话间的临时工作状态，
  不是仓库产物。该目录已在 `.gitignore` 中忽略，仅 `README.md`（模板）保留跟踪。
- 因此**不要为 handoff 文档单独提交**；`git status` / `git add` 时应确认没有混入
  `.agents/handoff/` 下的文件。
- 需要长期留存的结论（设计决策、接口约定、迁移方式）请固化到 `AGENTS.md`、`README.md`、
  其他正式文档或代码注释中，而不是依赖 handoff。

**新会话接续工作时的顺序**：读 `AGENTS.md` → 读相关 handoff 文件（本地文件，不在 git 历史中）
→ `git status` / `git log --oneline -10` → 再开始动手。

### 4.3 验证与提交责任

- 并行开发期间，**各会话不要自行执行 `git commit` / `git push`**，避免共享索引与历史冲突。
- 由 Lead（或任务指定的唯一集成负责人）在功能完成、编译通过、必要测试通过后统一提交。
- 提交前必须 `git status` 检查改动是否都属于当前提交范围，确认没有混入他人未完成的修改。
- Lead 给出最终答复前，必须完成最终 diff 审查并跑通最终验证。

---

## 5. Git 提交规范（强制）

### 5.1 语言要求

- **提交信息必须使用中文**（`type`、`scope` 可以保留英文约定，描述、正文必须中文）。
- 禁止使用 `update`、`fix`、`修改`、`提交` 这类无信息量的提交信息。

### 5.2 提交时机

- **更新文档、修复 bug、添加新功能后必须及时提交**，完成一项提交一项。
- 不要把多个不相关改动塞进同一个提交；也不要攒很久再一次性提交。
- 提交前必须保证改动可编译；不得提交半成品或无法通过编译的代码。

### 5.3 Conventional Commits

格式：

```
<type>(<scope>): <中文简短描述>

<可选中文正文：说明动机、方案、影响面>

<可选脚注：BREAKING CHANGE / Closes #123 等>
```

常用 `type`：`feat` / `fix` / `docs` / `style` / `refactor` / `perf` / `test` /
`build` / `ci` / `chore` / `revert`。

示例：

```
feat(tunnel): 支持 SSH 接力跳转的多级跳板
fix(ssh): 修复本地监听断开后未触发重连
docs(agents): 补充桌面端端到端验证步骤
build(tauri): 补充 Linux 打包所需的系统依赖说明
```

其他要求：

- 一个提交只做一件事，提交信息与改动内容必须一致；
- 不提交密钥、账号密码、`master.key`、`tunnels.json`、安装包产物、IDE 临时文件；
- 不提交 `.agents/handoff/` 下的交接文档（仅模板保留跟踪），详见 4.2 节；
- 破坏性变更使用 `!` 标注（如 `feat(api)!: ...`）并在正文说明迁移方式。

---

## 6. 代码与安全约定（摘要）

### 6.1 UI

- 所有用户可见文案必须走 `src/strings.ts`，禁止在组件里硬编码文本。
- 组件拆分为可复用的小组件，避免单个文件堆到上千行；配色与圆角统一走
  `src/styles.css` 的设计令牌，不要散落硬编码颜色值。
- 列表使用稳定的 `key`（隧道 ID、应用 ID、应用组 ID、跳板 ID），状态变化应由后端事件驱动，
  不要在组件内部复制业务可变状态。
- 调用后端统一走 `src/api.ts`，组件里不直接拼 `invoke` 字符串。
- 破坏性操作（删除应用、删除应用组、删除隧道、修改被引用的隧道、清除已知主机）必须二次确认。
- 导航为底部三页（应用 / 隧道 / 设置）。界面内不再提供全局“全部开启 / 全部断开”，
  批量操作以应用组为单位；托盘菜单的“全部断开”保留为紧急停止入口。

### 6.2 凭据与密钥

- SSH 密码、私钥、私钥口令等敏感信息：不得写入日志、不得硬编码；持久化必须经
  `secrets.rs` 用 AES-256-GCM 加密，密文以 `v1.iv.data`（Base64）形式存放。
- 主密钥由随机数生成后保存在应用数据目录的 `master.key`，文件权限 `0600`；
  首次运行创建，不得随仓库分发，也不得把明文密钥复制到配置 JSON。
- 解密失败时按“未配置”处理并提示用户重新输入，不允许回退到明文。
- 私钥在内存中只以解密后的字符串短暂存在；任何新增的临时文件路径都必须
  `0600` 且用完立即删除。
- 主机密钥采用 TOFU：首次连接固定指纹；同一 `host:port` 的指纹变化时**拒绝连接**。
  任何“自动接受变更”的改动都属于安全回退，提交前必须说明理由。
- 日志里不要输出密码、私钥、完整主机密钥；`runtime.rs` 的异常日志已做脱敏，保持这一点。

### 6.3 并发与线程

- 网络、加密、文件 IO 一律使用 Tokio 异步接口，禁止在 Tauri 命令中执行阻塞操作。
- `runtime.rs` 是进程内单例监督器：所有生命周期操作（start/stop/restart/delete）
  必须走它，不要在命令或 UI 里直接构造 SSH 连接。
- **隧道是配置，不是运行时**：`TunnelConfig` 只保存 SSH 用户名/地址/端口/跳板/凭据，
  不持有连接。每个启动的应用各自建立独立 SSH 会话与本地监听，因此多个应用引用同一隧道时
  互不共享 socket、互不拖累。不要把隧道改回“一条共享会话 + 多转发”。
- 用户意图（应用的 `desired_on`）与运行状态（`AppStatus`）分离：前者持久化、后者只在内存。
  只有应用重启后才根据 `desired_on` 恢复，不要用运行状态做持久化判断。
- 每个应用的 start/stop/restart/delete 必须串行化（per-app 锁），并等待旧任务结束后再写入
  新状态，避免快速点击导致“意图”和实际任务不一致、旧任务的清理覆盖新状态。
- 启动应用前必须校验三件事：隧道存在、应用组存在、本地端口未被其他处于 desired 状态的应用
  占用；任一不满足就写入 `AppBlockReason`，让界面显示红色叹号与错误详情。
- 状态变更通过 Tauri 事件 `app-status` 广播，前端只订阅、不轮询。

### 6.4 托盘与保活

- 窗口关闭默认隐藏到托盘而不是退出；托盘菜单至少提供“显示主窗口”“全部断开”“退出”。
- 有应用在转发时，退出必须走显式菜单项，并在退出前关闭所有监听端口与 SSH 会话。
- 状态文案随运行状态变化（转发中 / 重连中 / 准备中），避免出现“UI 说在运行但实际没有连接”。
- 所有应用都停止后，托盘提示与状态计数必须同步归零，不要残留僵尸状态。
- 登录自启动只恢复 `auto_start_on_boot = true` 的应用；普通 `desired_on` 恢复由应用启动路径负责。

### 6.5 重连与保活

- 重连退避策略集中在 `runtime.rs` 的 `ReconnectPolicy`（1s→2s→4s→8s→16s→30s 封顶），
  修改策略必须同步更新单元测试。
- 主机密钥变更等安全类错误必须**停止自动重连**并清理 `desired_on`，否则会出现无限重试风暴。
- 保活既要依赖 SSH 层的 keepalive 配置，也要在本地监听任务异常结束时触发重连；
  两者缺一不可。
- 本地端口被占用、监听器意外退出、SSH 通道读写失败，都必须映射为可读的错误信息。
- 隧道被应用引用时的删除/修改：界面先二次确认，确认后由 `runtime.rs` 的 `delete_tunnel`
  / `save_tunnel` 先停止相关应用再落盘。删除会把这些应用标记为 `tunnel_deleted`
  （应用保留、可改选隧道），修改只把它们置为已停止。
- 删除应用组必须先确认，确认后停止并删除组内应用，再删除分组本身；`default` 组不可删除、
  不可重命名，缺失的分组引用在归一化时回落到 `default`。
- 隧道连接测试（`test_tunnel` / `ssh::probe`）只走认证与跳板链，不绑定本地端口，
  不得影响正在运行的应用。

### 6.6 依赖与构建

- Rust 依赖统一在 `src-tauri/Cargo.toml` 维护；前端依赖统一在 `package.json` 维护。
- 新增 Tauri 插件时必须同步更新 `src-tauri/capabilities/default.json` 的权限声明。
- 不得在仓库中提交 `dist/`、`src-tauri/target/`、`node_modules/`、`*.log`、`master.key`、
  `tunnels.json`、`known_hosts.json` 等本地产物。
