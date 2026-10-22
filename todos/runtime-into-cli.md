# Runtime 合入 CLI 执行项（serve 子命令）

## 背景与决策

2026-10-22 与维护者确认：`anvil-desktop-runtime` 的 Runtime 服务器合入 `anvil` CLI，作为 `serve` 子命令，删除独立 crate 与独立二进制。动机：

- workspace 少一个 crate 和一个 bin 产物，打包脚本与 CI target 各少一份。
- 终端用户安装的 CLI 与桌面 App 运行的业务逻辑天然同源，口径一致。
- App 包继续把 `anvil` 二进制作为 sidecar 打入，Shell 现有签名更新通道与启动监管不变。

**暂缓**：CLI 发布流水线与 Runtime 自更新（见下文 Phase 2–4）。待出现"Runtime 频繁迭代、UI 不动"的真实发布痛点再立项；届时优先考虑在 `stable.json` 更新 manifest 内把 sidecar 做成独立版本化的 artifact（同一条 Ed25519 签名通道、独立版本与回滚），而不是让 Shell 直连 GitHub release 第二下载通道。

## Phase 划分

1. [x] 合代码：Runtime 模块搬入 `anvil`（`serve` 子命令），删除 `anvil-desktop-runtime`，迁移集成测试，打包脚本与 App manifest 指向 `anvil` 二进制。
2. [ ] CLI 三平台发布流水线 + SHA-256 清单 + 最新版索引（暂缓；独立于桌面端，CLI 用户亦受益）。
3. [ ] Shell 侧数据目录覆盖机制（包内种子 + 版本目录优先）与 manifest schema 调整（暂缓）。
4. [ ] `serve` 自更新器：检查/下载/校验/发布/回滚 + `http_api_version` 兼容策略（暂缓）。

每项独立可合可回滚；Phase 2–4 开工前需维护者确认。

## Phase 1 接口约定

- Shell 以环境变量传 `ANVIL_HTTP_API_VERSION`、`ANVIL_LOG_STDERR`、`ANVIL_LOG_DIRECTORY`，并透传 `RUST_LOG` 与 `ANVIL_MICROSOFT_CLIENT_ID`；凭据仍经 stdin 单行传入，ready JSON 仍写 stdout，`shutdown` 行或 stdin EOF 优雅退出。`serve` 不新增 CLI 参数。
- 命名：`anvil serve`。帮助文本注明这是"桌面 App 的 Runtime 进程，由桌面 Shell 启动"；与 `server`（Minecraft 服务端管理）语义区分。人类直接执行会阻塞等待 stdin，属预期行为。
- App manifest `sidecar_entry` 由 `runtime/anvil-desktop(.exe)` 改为 `runtime/anvil(.exe)`；`package-desktop-app.mjs` 改为构建并复制 `anvil` 二进制。
- Shell 启动逻辑经 manifest 间接引用 sidecar 路径，不需要改动。

## Phase 3/4 风险与开放问题（开工前复核）

- **api_version 握手**：manifest 现为单值 `http_api_version`，Shell 启动时强校验、不匹配拒启。外部 CLI 独立演进后需要 range 化（参照 `shell_api_compatibility` 的 VersionReq 语义）、兼容窗口或下载期过滤策略，属于真实设计工作。
- **校验强度**：现有通道为 Ed25519 签名 manifest + SHA-256 指纹 + 归档路径校验；GitHub release asset 仅 TLS，sha256 与产物同源。新通道必须拉平到现有强度才可上线。
- **Windows**：采用"下次启动生效"模式，不做运行中替换，绕开 rename-swap。
- **回滚**：复用 app_storage 的 staging/publish 与版本保留机制，启动失败回退包内种子。

## Phase 1 验收记录

2026-10-22：

- 实现：`crates/anvil-desktop-runtime/src/{lib,accounts,api,operations,protocol,versions}.rs` 迁移为 `crates/anvil/src/serve.rs` 与 `crates/anvil/src/serve/`；`anvil serve` 子命令经 `main` 早分发，绕开 CLI tracing 与 JSON envelope，改用 `anvil-logging` 初始化；`crates/anvil-desktop-runtime` 删除；集成测试迁至 `crates/anvil/tests/runtime_server.rs`，改经 `CARGO_BIN_EXE_anvil` + `serve` 参数拉起。
- 顺带修复两处既有问题：`anvil-logging` 两处 `if let` 嵌套不合 clippy `collapsible_if`（此前未被 CI clippy 覆盖，`anvil` 依赖它后暴露），已折叠为 let 链；迁移后的 `runtime_server` 集成测试首次真实运行，发现 CORS 断言写死了 Title-Case 头名而 Hyper 实际输出小写头，已改为大小写不敏感断言。该测试此前从未被执行过（见 client-milestone 与 desktop-account-login 的"未运行自动化测试"记录），非本次合并引入的行为变化。
- Shell 接口零改动：`ANVIL_HTTP_API_VERSION` 等环境变量、stdin 凭据、ready JSON 协议均保持；`apps/desktop/manifests/*.json` 的 `sidecar_entry` 改为 `runtime/anvil(.exe)`；`package-desktop-app.mjs` 改为构建并复制 `anvil` 二进制。
- 检查：`cargo fmt --all --check`、`cargo clippy --locked -p anvil --all-targets -- -D warnings` 通过；`cargo test --locked -p anvil -p anvil-core -- --test-threads=1`（20 + 63 项，1 项忽略）与 `cargo test --locked -p anvil-shell`（27 项）通过。未重新执行真实下载 smoke 与 WebView 联调，维持既有验收边界；`desktop:package` 脚本未在本机实际执行打包。
