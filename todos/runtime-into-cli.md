# Runtime 合入 CLI 执行项（serve 子命令）

## 背景与决策

2026-10-22 与维护者确认：`anvil-desktop-runtime` 的 Runtime 服务器合入 `anvil` CLI，作为 `serve` 子命令，删除独立 crate 与独立二进制。动机：

- workspace 少一个 crate 和一个 bin 产物，打包脚本与 CI target 各少一份。
- 终端用户安装的 CLI 与桌面 App 运行的业务逻辑天然同源，口径一致。
- App 包继续把 `anvil` 二进制作为 sidecar 打入，Shell 现有签名更新通道与启动监管不变。

**进度**：Phase 1–4 已全部完成（2026-10-23）。runtime 自更新走 CLI release 的签名 `runtime-index.json` 通道；App 包继续以包内种子兜底，override 失败自动回退并拉黑。后续演进（如 sidecar 独立版本化的 `stable.json` artifact）在出现真实需求时另立方案。

## Phase 划分

1. [x] 合代码：Runtime 模块搬入 `anvil`（`serve` 子命令），删除 `anvil-desktop-runtime`，迁移集成测试，打包脚本与 App manifest 指向 `anvil` 二进制。
2. [x] CLI 发布流水线 + SHA-256 清单 + `latest.json` 最新版索引（2026-10-23 完成；4 个 target 与 Q2Q release 矩阵一致，linux/windows arm64 待 Phase 3/4 需要时扩展）。
3. [x] Shell 侧数据目录覆盖机制（包内种子 + 版本目录优先）与 manifest schema v2（2026-10-23 完成）。
4. [x] `serve` 自更新器：检查/下载/校验/发布/回滚 + `http_api_version` 兼容策略（2026-10-23 完成，消费 Phase 2 的签名 `runtime-index.json`）。

每项独立可合可回滚；Phase 2–4 开工前需维护者确认。

## Phase 1 接口约定

- Shell 以环境变量传 `ANVIL_HTTP_API_VERSION`、`ANVIL_LOG_STDERR`、`ANVIL_LOG_DIRECTORY`，并透传 `RUST_LOG` 与 `ANVIL_MICROSOFT_CLIENT_ID`；凭据仍经 stdin 单行传入，ready JSON 仍写 stdout，`shutdown` 行或 stdin EOF 优雅退出。`serve` 不新增 CLI 参数。
- 命名：`anvil serve`。帮助文本注明这是"桌面 App 的 Runtime 进程，由桌面 Shell 启动"；与 `server`（Minecraft 服务端管理）语义区分。人类直接执行会阻塞等待 stdin，属预期行为。
- App manifest `sidecar_entry` 由 `runtime/anvil-desktop(.exe)` 改为 `runtime/anvil(.exe)`；`package-desktop-app.mjs` 改为构建并复制 `anvil` 二进制。
- Shell 启动逻辑经 manifest 引用 sidecar 路径与 sidecar_args（Anvil 为 ["serve"]），开发包、包内种子和 override 统一使用这些参数；Shell 不硬编码业务子命令。

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

## Phase 2 验收记录

2026-10-23（维护者确认后开工）：

- 新增 `.github/workflows/cli-release.yml`，tag `cli/v*` 推送触发。流程：4 target matrix 构建 → 版本校验（tag ↔ `Cargo.toml` workspace 版本 ↔ 二进制 `--version` 三方一致）→ 打包（`anvil-<ver>-<target>.tar.gz` / `.zip`，含二进制 + LICENSE + README.md）→ release job 生成 `SHA256SUMS` 与 `latest.json`（版本、tag、每 target 的归档名/sha256/下载 URL，供将来 Phase 4 自更新器消费）→ draft release → 上传 → publish --latest。已发布 release 的 asset 不可变，重试只允许修复 draft。
- target 矩阵与 Q2Q release 完全一致：`x86_64-unknown-linux-gnu`（ubuntu-22.04，glibc 2.35 兼容）、`x86_64-apple-darwin`（macos-15-intel）、`aarch64-apple-darwin`（macos-15）、`x86_64-pc-windows-msvc`（windows-2022）。`macos-15-intel` 经 Q2Q 10-22 的成功 run 验证可用（actionlint 1.7.7 的 label 数据库滞后，报未知属误报）。
- `latest.json` 生成脚本与版本提取脚本段在本地用假产物实测通过；修复了一处 `.tar.gz` 双后缀被 `rpartition('.')` 切错的真实 bug。
- 发布操作流程：`Cargo.toml` workspace 版本 bump 并合入 main → `git tag cli/vX.Y.Z && git push origin cli/vX.Y.Z`。CI 未真实执行过一次完整发布，首次打 tag 时需人工核对产物。
- 风险提示（Phase 4 开工前复核）：GitHub release asset 无 Ed25519 签名，sha256 与产物同源（同一 release 页面），校验强度低于 Shell 的 `stable.json` 签名通道。

## Phase 3 验收记录

2026-10-23（维护者确认方案后开工）：

- Manifest schema v2：新增必填 `http_api_compatibility`（VersionReq），校验 `http_api_version` 必须落在范围内；六个 `apps/desktop/manifests/*.json`、shell 单元测试与 updater/bundled_app/app_protocol 测试 fixture 同步升级，deny_unknown_fields 保证旧 manifest 显式拒绝。
- Shell 侧 override 机制（`apps/shell/src-tauri/src/runtime.rs`）：启动时解析 `<app>/sidecar-store/current.json`（schema 1：`version`/`sidecar_entry`/`http_api_version`），经限长、路径安全（复用 `bundled_app::resolve_entry` 的 symlink 防护）、可执行与 API 范围校验后优先启动；失败即把版本写入 `blocked.json`（原子写）、删除 current.json、回退包内种子，blocked 版本不再被 resolve。Shell 全程不接触 Anvil 版本策略或下载渠道，机制保持 App 无关。
- Shell 给 sidecar 新传 `ANVIL_SIDECAR_STORE_DIR` 与 `ANVIL_HTTP_API_COMPAT` 两个环境变量（dev 模式不传，更新器在 dev 下天然禁用）。
- `serve` 侧真实握手：`HTTP_API_VERSION` 常量（当前 1.0.0）与 shell 传入的 `ANVIL_HTTP_API_VERSION` 强校验相等，修复此前"回显即通过"的循环校验；新增隐藏诊断参数 `anvil --serve-api-version`（早于 clap 分发）供 release 流水线与更新门禁使用。
- 测试：shell `runtime` 模块新增 4 项 unix 测试（override 优先 + 环境契约、失败回退 + 拉黑、API 范围外跳过、blocked 拒启），全量 `-p anvil-shell` 50 项通过。

## Phase 4 验收记录

2026-10-23：

- `crates/anvil/src/serve/self_update.rs`（约 630 行含测试）：编译期 `option_env!` 注入索引 URL 与 Ed25519 公钥（缺失或为空即整体禁用），运行期要求 shell 的 `ANVIL_SIDECAR_STORE_DIR` + `ANVIL_HTTP_API_COMPAT`。ready 后延迟 2 秒启动，24 小时限频（`update-check.json` 原子记录），shutdown 令牌取消。
- 校验链与 Shell App 更新通道同强度：Ed25519（ring）验签索引 envelope（与 `SignedReleaseEnvelope` 同格式）→ 载荷校验（schema/product/canonical semver/https-only 且无 userinfo/fragment/大小与 sha256 界限）→ 版本过滤（> 当前 `CARGO_PKG_VERSION`、落在 API 兼容范围、不在 `blocked.json`、与 current.json 指针不同）→ `anvil-core::Context::download_file`（q2q-http，sha256 + expected_size）下载裸二进制 → `--version`/`--serve-api-version` 双门禁 → 发布到 `versions/<ver>/anvil` → 原子写 `current.json` → 保留最近 3 个版本。
- anvil-core 新增公共 `Context::fetch_bounded`（Executor GET + 限长，供签名索引等自校验元数据）与 `Context::download_file`（q2q-http 单文件下载 + resume 探测）；q2q-http 的 `DownloadRequest` 强制 checksum，索引因此走 Executor 而非 Downloader。
- 发布流水线（`.github/workflows/cli-release.yml` + `.github/scripts/sign_runtime_index.py`）：build 额外产出裸二进制（机器消费产物，无归档/解压，规避 zip-slip），构建时注入仓库变量 `ANVIL_RUNTIME_UPDATE_MANIFEST_URL`/`ANVIL_RUNTIME_UPDATE_PUBLIC_KEY`；release job 用 ubuntu 二进制自报 `--serve-api-version`，pynacl 以仓库 secret `ANVIL_RUNTIME_SIGNING_KEY` 生成签名 `runtime-index.json` 上传。secret 缺失时跳过签名（release 仍产出，更新器侧等价禁用）。
- 密钥运维：`cargo run -p anvil --example gen_update_key` 生成 Ed25519 密钥对；私钥 seed 已入仓库 secret `ANVIL_RUNTIME_SIGNING_KEY`（macOS 钥匙串另有备份，服务名 `mivinci-Anvil-runtime-signing-key`），公钥入仓库变量。`--example verify_runtime_index` 提供与 serve 同款验签，本地已用真实密钥完成 pynacl 签 ↔ ring 验交叉演练。
- CI：`rust.yml` 的 clippy/测试纳入 `-p anvil-shell`（其 main.rs 一处 `needless_return` 即此盲区暴露，已修）。
- 测试：self_update 9 项（签名/篡改/载荷校验、计划过滤、指针幂等、保留策略、本地 HTTP fixture 端到端安装）；三 crate 全量 143 项通过；`cargo fmt` / clippy `-D warnings` 干净；`actionlint` 干净（`macos-15-intel` label 为已知误报）。
- 未验证边界：CI 从未真实执行一次 CLI release；发布的首个带更新器二进制（bump 到 0.2.0 起）需人工核对 4 个 target 的 `runtime-index.json` 与资产。老于密钥轮换的二进制只信任旧公钥，轮换需随版本发布新公钥。

## 启动参数遗漏修复（2026-10-23）

维护者实际运行 desktop:dev 时，CLI 打印顶层帮助后退出，Shell 报 InvalidReadyMessage。原因是 Runtime 合并为子命令后，打包仅改了二进制路径，Shell 仍无参数执行。现新增可选 manifest sidecar_args（默认空数组，最多 16 项，每项最多 1024 字节且无 NUL），所有 Anvil 平台 manifest 配置 ["serve"]，Shell 用 Command::args 直接传参。开发模式、包内种子和自更新 override 共用 App manifest 参数；override pointer 不新增参数来源。没有启动 shell 命令解释器。

检查：Rust 格式及 Shell 开发/发行编译检查；未运行测试或重启维护者 dev。下次 desktop:dev 的打包步骤会生成含启动参数的基线 manifest，真实握手由维护者重启后确认。
