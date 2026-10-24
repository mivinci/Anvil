# Anvil Shell 执行清单

状态：第 1–9 项已完成。第 10 项已恢复推进：跨平台 Desktop 发行工作流和 stable/test 签名更新 feed 已加入，尚未完成本地发行构建、真实 GitHub 测试发布和各平台 WebView/回滚 smoke。设计决策见 [Anvil Shell 设计草案](shell-design.md)。

## 范围与既定约定

- Shell 是通用 Tauri 宿主，只负责 App 加载、生命周期、更新和自身的原生能力；Shell 不依赖 `anvil-core`，不实现 Minecraft 业务。
- Shell 原生能力通过受限、类型化的 Tauri IPC 暴露；桌面 App 业务请求由页面直连 App sidecar 的 loopback HTTP API。
- 桌面页面与 sidecar 属于同一个签名 App 版本包；更新后台下载，在下次 Shell 启动时激活，页面或 Runtime 未健康时回滚。
- 当前 App 目录为 `apps/shell` 和 `apps/desktop`；Rust crate 名 `anvil-shell`、npm 包名 `@anvil/desktop` 和 App ID `anvil.desktop` 保持不变。
- 当前不创建 `packages/`。只有多个 App 真实共享代码时，才抽取 npm workspace package。
- 首轮只实现桌面默认 App；移动端、第三方 App、App Store 和多个 App 同时运行不在本清单范围内。

## 执行项

### 1. [x] 移除 Shell 的 Core 依赖

**交付：** `apps/shell/src-tauri` 只保留 Tauri 宿主和通用 Shell 能力，移除 `anvil-core` 依赖及任何领域命令。保留 crate 名 `anvil-shell`。App 业务实现留给后续的 App sidecar。

**验收：** `cargo tree -p anvil-shell --offline` 不包含 `anvil-core`；`cargo check --workspace --offline` 通过。README 与架构文档准确描述 Shell 的边界。

### 2. [x] 定义构建参数与 App 包清单

**交付：** 定义目标平台、默认 App ID 和 App Runtime API 兼容范围的构建配置；定义带 schema 版本的 App manifest，描述 App ID、版本、平台、页面入口、sidecar 入口、HTTP API 版本和 Shell API 兼容范围。明确构建产物如何把默认 App 包嵌入 Shell 安装包。

**验收：** Shell 构建会解析并校验示例 manifest；临时配置验证了缺少必需字段、App ID 不匹配和目标平台不符都会以明确诊断失败。`cargo check --workspace --offline` 通过。构建参数使用 Cargo target triple、默认 App ID 和 Shell API 版本，不把 `desktop` 或 `mobile` 写成 Shell 内部的业务分支。

### 3. [x] 实现本地 App 存储与版本状态

**交付：** `anvil-shell::app_storage` 使用系统应用数据目录，并可按 App ID 隔离版本目录与 schema 2 状态文件；可读取并迁移 schema 1 状态。状态原子记录活动版本、待启用版本、上一个健康版本、启动确认标记和失败版本。版本先写入同一 App 目录下的 staging，再原子发布到版本目录；发布后的版本不可由存储 API 覆盖，并通过只读文件句柄解析包内文件。

**验收：** 当前 `apps/shell/tests/app_storage.rs` 的 13 项本地集成测试覆盖初次无状态与 App 隔离、活动/待启用/上一个健康版本解析、schema 1 迁移、损坏和缺失版本状态拒绝、原子写入失败、版本不可覆盖、路径穿越/绝对路径拒绝，以及发布与读取期间的符号链接和越界路径拒绝。第 4 项的本地协议现已读取活动版本；首次安装内置包仍属于第 7 项。

### 4. [x] 实现本地页面加载与恢复入口

**交付：** Shell 的 `frontendDist` 只包含轻量恢复页面；生产窗口从 `anvil-app://localhost/` 启动，并通过异步 Tauri 本地协议读取活动 App 版本中的 `web/` 页面资源。协议校验活动版本、manifest、目标平台、Shell API 兼容范围和页面入口，再处理 MIME、缺失文件和资源路径；没有有效活动版本时显示恢复入口。桌面 SvelteKit 页面不再嵌入 Shell 二进制。开发模式仍使用 Vite 页面。

**验收：** `apps/shell/tests/app_protocol.rs` 的 7 项本地集成测试调用 Shell 使用的协议处理器，覆盖 fixture 页面/脚本/CSS、本地 MIME、无活动版本、损坏 manifest、缺失入口、manifest 版本不匹配、缺失资源、GET/HEAD、错误方法、路径穿越/绝对路径/错误编码和符号链接拒绝。SvelteKit 生产构建成功，生成页面资源引用均为本地路径。2026-10-22 `cargo test --locked --offline -p anvil-shell`、`cargo check --workspace --locked --offline`、Clippy、`cargo fmt --all -- --check`、`npm run build --workspace @anvil/desktop` 和 `npm run check --workspace @anvil/desktop` 通过。跨平台 Tauri WebView smoke 留待第 10 项平台验收。

### 5. [x] 固化并实现 Shell 原生 IPC

**交付：** 实现 `shell_get_app_info`、`shell_get_update_status`、`shell_check_for_updates` 和 `shell_get_runtime_status` 四个窄命令，并在 `apps/desktop/src/lib/shell.ts` 提供 typed client。App ID 固定为构建配置中的默认 App ID；身份结果包含构建 manifest 的基线版本和本地存储中的活动版本。Tauri build 仅为这四个命令生成权限；`default` capability 只授予 `main` 窗口，且不配置远程来源。命令另校验窗口 label 与 Shell/dev origin，不接受前端指定 App ID、路径、URL 或系统操作。更新进度通过状态查询暴露；更新器与 sidecar 尚未实现时明确返回 `not_configured`。

**验收：** 第 5 项交付时，IPC 测试覆盖允许的 Shell/dev origin、错误窗口与来源拒绝，以及 capability 仅绑定 `main`、无远程 URL 且权限恰好对应四个 Shell 命令；Tauri build 实际生成四个命令 ACL。`apps/desktop/src/lib/shell.ts` 提供身份、更新和 Runtime 查询及更新检查请求的前端类型。第 6 项增加 Runtime 连接引导命令后，当前 capability 仅绑定 `main` 且恰好授予五个 Shell 命令。Shell IPC 中不出现账户、客户端、服务端等业务方法。2026-10-22 `cargo clippy --locked --offline --all-targets -p anvil-shell -- -D warnings`、`cargo check --locked --offline --workspace`、`cargo fmt --all -- --check`、`npm run check --workspace @anvil/desktop` 和 `npm run build --workspace @anvil/desktop` 通过。

### 6. [x] 实现 sidecar 监管与 HTTP 连接引导

**交付：** 将桌面 App 的 Rust sidecar crate `crates/anvil-desktop-runtime` 纳入 Cargo workspace，并让它自己依赖 `anvil-core`。Shell 从活动 App 包启动、监管和停止 sidecar。sidecar 只监听 loopback 的动态端口；每次运行使用短期凭据。Shell 通过窄 IPC 将当前 Runtime 地址和凭据交给受信任页面，页面直接请求 sidecar 的健康端点。生产与开发 CSP 只开放 Tauri IPC、当前 loopback API 和开发服务器所需连接。

**验收：** `cargo test --locked --offline -p anvil-shell -p anvil-desktop-runtime` 通过。sidecar 单元与真实本地 HTTP 集成测试覆盖动态 loopback 端口、授权健康请求、无效凭据、错误 Origin、预检和关闭；Shell 测试 stub 覆盖 ready 握手、启动失败回收、连接状态和优雅停止。凭据经 stdin 一次性传入 sidecar，Shell 只在内存保留且不写入 URL、日志或持久化文件。桌面 typed client 通过 `shell_get_runtime_connection` 获取本次连接并直连 `/health`。当前 sidecar 仅实现健康端点；完整 App 包首次安装仍属于第 7 项，真实页面/sidecar 联动与各平台 WebView smoke 留待第 7、10 项验收。

### 7. [x] 打通内置桌面 App 的首次启动

**交付：** `npm run desktop:package` 按 Tauri target triple 构建 SvelteKit 静态页面、对应平台的 `anvil-desktop` sidecar 和 manifest，生成完整的 `anvil.desktop` 基线包；Tauri `bundle.resources` 将它装入 Shell 安装包。首次运行安全发布到本地 App 存储，再从该版本加载页面并启动 sidecar。前端 typed client 位于 `apps/desktop/src/lib`，不创建尚无第二消费者的共享 npm 包。

**验收：** `cargo test --locked --offline -p anvil-shell` 的 26 项测试通过，首次安装测试覆盖包校验、staging 原子发布、状态恢复、已安装版本保护、符号链接拒绝和 sidecar 执行权限。2026-10-22 `npm run desktop:package` 和 `npm run desktop:build -- --bundles app` 成功；生成的 `Anvil.app/Contents/Resources/baseline` 含 manifest、静态页面与 aarch64 sidecar。使用该 bundle 资源和隔离 HOME 完成离线安装、实际 sidecar ready、带凭据的 `/health` 200 响应及有序停止。桌面 typed client 将 Shell 状态请求发往 Tauri IPC，并直接连接 sidecar 健康端点；构建脚本从同一 manifest 生成页面包和 sidecar，Shell 启动时校验 App 版本与 API 兼容性。当前系统的 LaunchServices 无法打开测试 `.app`（`kLSNoExecutableErr`），完整 `desktop:build` 的 DMG 阶段也因环境中的 `hdiutil`“设备未配置”失败；WebView 与安装器 smoke 留待第 10 项平台验收。

### 8. [x] 实现 App 更新下载与验证

**交付：** Shell 启动时检查更新，并每小时调度一次检查；持久时间戳保证每 24 小时最多访问一次。发行构建通过 `ANVIL_UPDATE_MANIFEST_URL` 和 `ANVIL_UPDATE_PUBLIC_KEY` 配置 HTTPS 更新源与 Ed25519 公钥，两者必须同时设置；未配置时如实返回 `not_configured`。签名发布 JSON 固定为 stable 通道，包含目标 triple、版本、tar.gz URL、大小与 SHA-256。后台通过锁定版本的 `q2q-http` 下载到按版本固定的 staging 路径，支持恢复与取消；检查发布签名、文件摘要、压缩/解包大小、tar 路径、manifest 与 API 兼容性后，发布不可变版本并原子标记待启用。页面能查看状态和进度并取消下载。

**验收：** 2026-10-22 `cargo test --locked --offline -p anvil-shell -p anvil-desktop-runtime` 通过（Shell 39 项、Runtime 3 项）。更新测试覆盖 Ed25519 签名、错误密钥与篡改、App/target/channel/URL 拒绝、版本和归档路径校验、真实本机 HTTP 下载、SHA-256 错误不改变活动版本、取消不改变活动版本、兼容包发布并设置 pending，以及 staging 与 24 小时记录持久化。`cargo check --locked --offline --workspace`、Clippy `-D warnings`、Rust 格式检查、SvelteKit 生产构建和 `npm run check --workspace @anvil/desktop` 通过。页面不能指定更新 URL 或发行者密钥。正式更新源和公钥尚未配置，因此默认构建仍显示 `not_configured`；发布密钥轮换/撤销和 beta 通道不属于本项交付。

### 9. [x] 实现下次启动激活、健康确认与回滚

**交付：** Shell 在启动时把待启用版本原子提升为启动候选版，确保候选页面与 sidecar 都从同一 App 版本加载。sidecar ready、页面 `/health` 请求及 API 版本核对完成后，页面通过受限 Shell IPC 确认健康；确认总时限为 30 秒。Runtime 启动失败、页面报告健康检查失败、或 30 秒内没有确认时，Shell 结束候选 sidecar、恢复上一个健康版本并重新加载页面。状态 schema 2 持久记录未完成启动，进程崩溃后下一次启动回滚；旧 schema 1 状态可读取并在下次写入时升级。失败版本及原因写入状态并阻止同一发行版自动再次下载/启用。

**验收：** 2026-10-22 `cargo test --locked --offline -p anvil-shell -p anvil-desktop-runtime` 通过（Shell 46 项、Runtime 3 项）。新增验收覆盖候选版本待确认状态、健康确认、超时回滚、进程崩溃后的恢复、候选包缺失时的恢复、较新待启用版本保留、失败版本不重复下载，以及 schema 1 状态迁移。现有 Runtime 测试覆盖 ready 握手失败时回收 sidecar。`cargo check --locked --offline --workspace`、Clippy `-D warnings`、`cargo fmt --all -- --check`、SvelteKit 生产构建和 `npm run check --workspace @anvil/desktop` 通过。尚未在 Tauri WebView/发行安装器中执行页面握手和回滚 smoke；这属于第 10 项平台验收。

### 10. [ ] 完成桌面发行与平台验收

**交付：** `.github/workflows/desktop-release.yml` 以 `desktop/v*` 和 `desktop-test/v*` 触发 Windows x64、macOS Intel/Apple Silicon 与 Linux x64 构建，打包 Shell、默认 App 和 signed update metadata；stable 与 test 使用不同的固定更新 feed。Runtime `cli/v*` 发布同步维护 `runtime-updates` 签名索引。发行前检查 Tauri/App manifest 版本与 tag 一致，并验证签名私钥/公钥配对。

**验收：** 2026-10-24 本机 Windows `npm run desktop:check` 通过（0 errors/0 warnings），`npm run desktop:package` 成功生成 `x86_64-pc-windows-msvc` 基线包；打包脚本重复生成得到相同 SHA-256，tar.gz 含 74 个文件及 `manifest.json`、静态页面和 sidecar。为验证从 0.1.0 升级，Tauri 与全部 6 份 App manifest 已同步准备为 Desktop 0.1.1。`npm run desktop:build` 成功生成 0.1.1 MSI（11.28 MiB）和 NSIS 安装器（7.68 MiB）；安装器收集脚本已识别两个产物。完整 `cargo test --locked --offline -p anvil-shell -p anvil` 通过（58 项；测试期间有 4 条既有 unused-import 警告）。真实 GitHub 测试发布 [Actions run 40080512492](https://github.com/mivinci/Anvil/actions/runs/40080512492) 成功构建并发布 Windows x64、macOS Intel/Apple Silicon 与 Linux x64 产物；四个平台更新清单的 Ed25519 签名均通过验证，且通过清单中的发布 URL 下载包后，大小与 SHA-256 均匹配。测试发布只更新 `desktop-updates-test` feed。仍待在隔离数据目录执行 Windows 安装升级、待启用/重启激活与失败回滚 smoke；未实测的平台只记录为待验证，不标记为已支持。Desktop stable 更新使用 Secret `ANVIL_DESKTOP_UPDATE_SIGNING_KEY` 与 Variable `ANVIL_DESKTOP_UPDATE_PUBLIC_KEY`；测试版使用隔离的 `ANVIL_DESKTOP_TEST_UPDATE_SIGNING_KEY` / `ANVIL_DESKTOP_TEST_UPDATE_PUBLIC_KEY`；Runtime 更新使用 Secret `ANVIL_RUNTIME_SIGNING_KEY` 与 Variable `ANVIL_RUNTIME_UPDATE_PUBLIC_KEY`。

## 不纳入本轮

移动端 App、第三方 App、多个 App 同时运行、Shell 自身的自动更新、账户与 Minecraft 业务接口、远程页面加载。桌面 sidecar 的产品业务实现按客户端/服务端自己的执行清单推进；本清单只要求 Shell 能托管和连接它。
