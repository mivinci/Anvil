# 原版客户端首条可用流程

状态：执行方案；第 1–6 项、第 7.1–7.2 项及第 8 项的 CLI 接入已实现。Core 已包含客户端文件安装、运行目录准备、Microsoft 设备码授权 provider、系统凭证库适配、公开账户资料存储与结构化启动参数。CLI 已提供客户端与账户流程。原 Slint GUI 已移除；Tauri Shell + SvelteKit 桌面 App 骨架已建立，但 App 业务尚未接入。Shell 目标是通用启动和版本管理，不负责 Core 业务命令。真实 Microsoft 登录、OS 凭证库和 Minecraft 客户端运行尚无验收记录。

## 目标

逐步打通原版客户端的版本选择、文件安装、账户授权和启动。CLI 当前调用 `anvil-core`；目标桌面 App 由自己的业务组件复用 Core。通用 Tauri Shell 只启动和维护 App 版本，不承载客户端领域命令。桌面 App 当前只有 SvelteKit UI 基础结构。下载继续复用固定 revision 的 `q2q-http`，不另造下载与恢复机制。

首个端到端客户端版本定为 1.21.1。首批计划目标为 Windows x86_64、macOS x86_64/aarch64、glibc Linux x86_64。该矩阵是工作目标，不代表已验收或已支持：每个组合通过本地真实安装和启动 smoke 后，才能单独标记为已验证。官方 1.21.1 元数据缺少 Linux aarch64 所需的 LWJGL 原生库，计划生成器会拒绝该组合；Windows aarch64 与 musl Linux 暂不纳入。26.3 包含尚未实现的 `default-user-jvm`、`os.versionRange` 等规则，先不纳入首批范围。

| Minecraft | 平台 | 架构 | 状态 |
| --- | --- | --- | --- |
| 1.21.1 | Windows | x86_64 | 实现目标；真实客户端 smoke 待做 |
| 1.21.1 | macOS | x86_64 | 实现目标；真实客户端 smoke 待做 |
| 1.21.1 | macOS | aarch64 | 实现目标；真实客户端 smoke 待做 |
| 1.21.1 | glibc Linux | x86_64 | 实现目标；真实客户端 smoke 待做 |
| 1.21.1 | glibc Linux | aarch64 | 当前官方元数据缺少 LWJGL ARM 原生库；计划明确拒绝 |

此处的架构映射是实现目标，不能仅凭 Temurin 或 `Platform::current` 能识别平台就推断 Minecraft native 库齐全。安装计划核对了官方 1.21.1 元数据；Linux aarch64 因 LWJGL native 缺失而不在首批计划目标内。只有完成真实游戏启动的组合才算验收。

## 执行项

### 1. [x] 收敛当前 PR 反馈与基础约定

**交付：** README 面向启动器用户描述用途和当前能力；Core 测试文件直接放在 `crates/anvil-core/tests/`，不再按 `unit/` 子目录分层；生产路径不以 `unwrap` 处理可恢复错误，按需接入 `tracing` 记录诊断，并避免记录令牌、完整启动参数等敏感信息。

**验收：** 已按 PR 评审意见改写面向用户的 README，将 Core 测试平铺在 `crates/anvil-core/tests/` 并保留其私有实现测试方式；Core 与 CLI 生产代码不再使用 `unwrap` / `expect` 解可恢复错误，诊断通过 `tracing` 写到 stderr。2026-10-21 本地 18 项 CLI 与 36 项 Core 测试通过，1 项需真实 Mojang 网络的 smoke 按设计忽略；rustfmt、Clippy 和 CLI 构建通过。GitHub Actions run #13 曾验证 Ubuntu、Windows、macOS 以及 Slint 桌面入口；该桌面 crate 后续已移出 workspace。CI 使用本地 fixture，不下载真实 Java 或 Minecraft 文件。

### 2. [x] 确定首批客户端支持矩阵与安装契约

**支持目标：** 仅以 1.21.1 作为首个端到端客户端版本，平台目标见上文。选择 1.21.1 是因为已核对其 Java 21 要求和现代参数/native artifact 布局；不由此推断旧版本或后续版本兼容。目标平台已有的 Java 架构映射只说明能识别架构，不代表游戏 native、账户或运行已验收。

**实例与文件边界：**

- 客户端和服务端共用 `data/instances/`，由清晰的 `client-` / `server-` ID 前缀及 `instance.json` 类型字段区分。客户端名称沿用服务端命名规则：1–48 个 ASCII 字节，以字母或数字开头，只允许字母、数字、`.`、`_`、`-`；ID 为 `client-` 加小写名称，同名大小写视为同一实例。首版不提供改名。
- 客户端目录包含 `instance.json`、原子提交的 `installed.json` 和可变的 `game/`。`game/` 保存该实例的 options、saves、screenshots、resourcepacks 等用户数据；升级游戏版本不能覆盖或清除此目录。
- Mojang 元数据、游戏 JAR、libraries、asset indexes/objects 等可验证的不可变文件进入共享 cache，客户端实例通过安装记录引用文件身份，不复制一份到每个实例。托管 Java 继续放在共享 `runtimes/`。natives 可写工作目录和会话日志按运行会话隔离，不放进共享缓存。
- 默认沿用 `Paths` 的系统应用数据目录和缓存目录；`--data-dir ROOT` / `ANVIL_DATA_DIR` 与现有 CLI 一致，覆盖时数据放在 `ROOT`、缓存放在 `ROOT/cache`。客户端和服务端共用同一根目录，但锁和记录按实例类型区分。
- `instance.json` 表达稳定身份、类型和显示名等实例信息；`installed.json` 使用独立 schema 版本，记录游戏版本、目标 OS/架构、解析后的版本元数据身份、所需 Java 信息、asset index 身份和已验证文件引用。安装阶段最后才原子替换它；损坏或不兼容的记录明确报错，不按目录名猜测恢复。
- 新实例在同文件系统 staging 中准备，所有必需文件验证成功后再原子发布实例目录；已有实例升级时只原子替换安装快照。失败或取消保留旧快照和 `game/` 用户数据，不留下可被误认成完整安装的新实例。

**命令契约：**

```bash
anvil client install <name> --version 1.21.1
anvil --json client list
anvil account login --client-id <registered-microsoft-client-id>
anvil client launch <name> --account <uuid> --client-id <registered-microsoft-client-id>
anvil --data-dir ./anvil-data client install vanilla --version 1.21.1
```

`--json` 是当前 CLI 的全局参数，成功结果使用 schema 版本化 envelope；普通进度、设备码和诊断写 stderr。Microsoft Client ID 可由 `ANVIL_MICROSOFT_CLIENT_ID` 提供，或在登录/启动时显式传入。

| 命令 | 副作用 | 重试与边界 |
| --- | --- | --- |
| `client install NAME --version ID` | 可联网读取官方 metadata 并下载/校验缺少的游戏文件；创建实例或更新其安装快照。不登录、不启动游戏，不改写 `game/` 用户数据。 | 相同版本且安装记录和文件有效时返回 `already_installed=true`；有效共享缓存和 q2q-http 恢复数据可复用。失败或取消不提交新 `installed.json`，重试可继续。已有同名实例换版本时仅原子替换安装快照并保留 `game/`；同名不同大小写冲突到同一 ID。 |
| `client list` | 只读本地 `instance.json` / `installed.json`；不联网、不扫描 Java、不遍历或哈希每个缓存文件。 | 无记录时返回空列表。JSON 为排序后的 schema 1 envelope，明确表示安装记录和版本，但不宣称缓存仍完整或可以启动；损坏记录返回有上下文的错误，不静默跳过。 |
| `account login/list/logout` | login 通过 Microsoft 设备码登录；profile JSON 仅存公开身份和当前选择，refresh token 交给系统凭证库。list 只读本地 profile；logout 删除系统 refresh token 和 profile 条目。 | 需使用已注册的 Microsoft 公共客户端 ID，不需要 client secret。设备码只经 stderr 展示；凭证库不可用时明确报错，不把令牌回退写入文件。 |
| `client launch NAME [--account UUID]` | 刷新选定账户，读取 Mojang 版本 metadata 和 asset index 以重建启动计划；校验已安装文件，准备兼容 Java、assets 和临时 natives，并以前台进程启动游戏。游戏自身可改写该实例的 `game/`。 | 缺少账户时在启动进程前返回 `account_required`。不会下载或修复缺失的游戏文件；启动计划所需 metadata/index 可联网读取，缺失 Java 时按计划安装托管 Java。进程运行期间持有实例锁和会话 natives，日志转发到 stderr；取消会终止客户端进程并清理临时 natives。 |

缓存丢失不损坏实例用户数据：再次 install 会重新获取所需不可变文件。list 只报告本地记录，不把“有 `instance.json`”误报为“安装完整”；launch 在真正使用前检查所需文件。

**验收：** 本项完成设计决策与文档对齐，不代表客户端代码已实现。版本、OS/CPU 矩阵作为实现和实机 smoke 目标；尚未完成本地真实运行的组合不得标记为已验证或受支持。架构目录、命令副作用、重试规则、账户前置条件及实例/共享缓存边界均已明确。

### 3. [x] 解析并校验客户端版本 JSON

**交付：** `metadata::resolve_client` 复用 `Context` 中的 HTTP Executor，从官方版本总清单选择 JSON，先对原始字节校验 SHA-1，再解析版本 ID、Java 要求、client JAR、asset index、main class、libraries 和现代 JVM/game 参数数组。版本 JSON 最多读取 4 MiB；client JAR 和 asset index 分别限制为 1 GiB 与 16 MiB；文件 URL 必须是无凭据的 HTTPS。libraries 与参数条目保留原始结构，平台规则求值和参数展开留给后续阶段。

**验收：** 本地 fixture 覆盖成功解析、哈希不符、版本 ID 不符、缺少 client JAR/Java/asset index/arguments、非 HTTPS URL、无效长度或 SHA-1、超限响应及不可信 metadata 来源，不访问真实 Mojang 网络。2026-10-21 `cargo test --locked --offline -p anvil-core` 为 40 项通过、1 项需真实 Mojang 网络的既有 smoke 忽略；Clippy、rustfmt 检查通过。

### 4. [x] 生成平台相关的客户端安装计划

**交付：** `client_plan::plan_client` 校验 asset index 的声明长度与 SHA-1，并根据目标 OS/架构和 Mojang rules 生成确定的文件清单。rules 按顺序求值，最后一个匹配项决定 allow/disallow；支持 OS 名、Java OS 架构正则、已提供的 OS 版本和显式 feature 值。普通 library 与现代 native JAR 进入 classpath 计划；旧式 classifier 归档单独列出并携带安全校验过的提取排除路径。按资产 SHA-1 去重，生成资源对象 URL 和日志配置计划。未知条件、虚拟/映射旧资源布局及缺少目标原生库时返回 unsupported，不默认放行。

**验收：** 离线 fixture 使用官方 1.21.1 版本 JSON 和 asset index，覆盖 Windows x86_64、macOS x86_64/aarch64、glibc Linux x86_64 的原生库选择、日志配置与 3888 个唯一资源对象；Linux aarch64 因缺少 LWJGL ARM native 被拒绝。合成 fixture 覆盖 rules 顺序、未知 feature、旧 classifier 与提取排除、路径穿越和旧资源布局拒绝。计划不下载或解包实际文件，也不代表游戏已真实启动。

2026-10-21 本地 `cargo test --locked --offline -p anvil-core` 为 44 项通过、1 项联网 smoke 忽略；Clippy、rustfmt 检查和 `cargo check --locked -p anvil` 通过。测试使用已提交的离线 Mojang metadata/assets fixture 和本地 loopback fixture，不下载游戏或 Java 文件；未 push，也未触发 CI。

### 5. [x] 安装文件并原子提交客户端实例

**交付：** `ClientInstaller::plan` 通过 `metadata::resolve_client` 解析官方版本 JSON，以 `q2q-http` 下载并验证 asset index，再生成平台计划。`ClientInstaller::install` 收集 client JAR、libraries、native archives、asset index、asset objects 与日志配置，按 SHA-1 去重，最多并行提交 4 个文件给共享 `q2q-http`；下载长度、哈希、恢复和缓存发布由该依赖负责。缓存位于 `cache/clients/sha1/`，实例记录引用共享内容，不复制文件。`installed.json` 记录版本元数据 URL/SHA-1、asset index ID、目标平台、Java 要求和完整文件引用。新实例在 staging 中创建并原子发布；更新已有实例只原子替换 `installed.json`，保留 `game/` 用户数据。取消会取消并等待 q2q-http 任务；未发布的 staging 自动清理。共享缓存中的 q2q-http 分块和恢复记录可留待重试复用。

**验收：** 本地 fixture 覆盖缓存复用、损坏缓存修复、哈希失败、截断响应、并发安装、取消、失败后重试和未完成实例不可列出。2026-10-21 `cargo test --locked --offline -p anvil-core` 为 51 项通过、1 项真实 Mojang metadata smoke 忽略；Clippy `-D warnings`、rustfmt 检查、`cargo check --locked --offline -p anvil-core` 通过。测试使用 loopback HTTP fixture 和已提交 Mojang metadata/assets fixture，不下载真实游戏或 Java 文件；未 push，也未触发 CI。

本阶段支持的 client artifact 都必须带官方大小与 SHA-1，缺少必要完整性字段时拒绝计划；版本 JSON 等无预期文件大小的 metadata 继续使用有界 HTTP 读取。取消留下的 `.part` / `.resume` 是 q2q-http 管理的可恢复缓存状态，不会被列为已发布 artifact 或安装实例。

### 6. [x] 准备 Java、assets 与 natives 运行目录

**交付：** `ClientInstaller::prepare_runtime` 校验实例快照仍对应传入的安装计划与当前平台，逐个校验运行所需缓存文件的长度和 SHA-1（包括 asset index 与所有 asset objects）。通过硬链接把已验证文件映射为标准 `assets/indexes` 和 `assets/objects` 布局；文件系统不支持硬链接时复制，源内容缓存保持内容寻址。复用 `JavaManager::ensure_major` 找到当前平台上匹配的托管 Java，缺少时安装所需主版本。旧式 native 归档使用安全解包器并应用 Mojang `extract.exclude`，解到每次准备独立的 `data/sessions/` 临时目录；返回的准备句柄释放时自动清理 natives。该步骤不启动游戏，也不隐式修复或重新下载客户端文件。

**验收：** fixture 覆盖 Java 主版本/平台架构不匹配、资产对象缓存损坏、native 排除规则与路径穿越、并发会话目录隔离和句柄释放清理。`2026-10-21` 本地 `cargo test --locked --offline -p anvil-core` 为 54 项通过、1 项需真实 Mojang 网络的 smoke 忽略；workspace Clippy `-D warnings`、rustfmt 检查和 `cargo check --locked --offline -p anvil` 通过。测试只用本地 fixture，不下载真实 Java/Minecraft 文件；未 push，也未触发 CI。真实 Java 探测、游戏运行与各平台客户端 smoke 尚未验收。

### 7.1 [x] 定义账户服务边界并生成客户端 LaunchSpec

**交付：** Core 定义 `AccountProvider` 与 `CredentialStore` 接口，设备码流程通过事件 channel 汇报给入口；`AccountService` 协调授权、刷新令牌轮换和登出。登出删除本地 refresh token，不声称撤销已签发的服务端 access token。账户资料可以序列化，Access/Refresh Token 和短期会话不可序列化且 Debug 脱敏。Microsoft `clientid` 必须来自注册的 Azure 应用 ID，不从启动器名称猜测。客户端安装快照保留版本类型与 Mojang 参数数组；`build_launch_spec` 根据平台/账户 rules 展开现代 JVM/game 参数，并将 Java、main class 和参数作为独立 argv 元素交给调用方。LaunchSpec 持有运行时准备句柄，确保进程启动前会话 natives 不会被清理。

**验收：** fake provider 与内存 credential store 覆盖设备码事件、令牌刷新/轮换、身份一致性、登出和敏感值脱敏；离线 Mojang fixture 覆盖 classpath、日志配置、平台 rules、账户占位符、分辨率、含空格路径、缺少 clientid 和 LaunchSpec 生命周期。真实凭证没有写入仓库；本项没有制造真实 Microsoft 凭证。

### 7.2 [x] 实现 Microsoft/Xbox/Minecraft 授权与系统凭证库适配

**交付：** `MicrosoftAccountProvider` 实现 Microsoft OAuth 设备码与刷新流程、Xbox User/XSTS 交换、Minecraft Services 登录和 Java profile 查询；处理待批准、降速、拒绝、过期、取消、无游戏 profile 与令牌轮换。请求使用继承的系统 HTTP 代理、TLS、禁止重定向、每 host 单空闲连接、20 秒超时与 64 KiB 响应上限。`SystemCredentialStore` 使用 macOS Keychain、Windows Credential Manager 和 Unix Secret Service，只保存 Microsoft refresh token；系统凭证库不可用时明确报错，不回退到明文文件。设备码事件由当前 CLI 消费。

**验收：** 本地 loopback HTTP fixture 覆盖设备码与轮询、拒绝、取消、Microsoft → Xbox/XSTS → Minecraft profile、刷新令牌轮换和账户资料；所有请求不访问 Microsoft、Xbox 或 Mojang。workspace Clippy、格式检查、Core 62 项通过、1 项需真实 Mojang 网络的 smoke 忽略，CLI 构建检查通过。Keychain/系统凭证库未在当前无桌面凭证服务的环境中写入测试；真实 Microsoft 交互登录与 Windows/macOS 实机验证待后续 CLI 集成和维护者手动 smoke。需要 Anvil 注册的 Microsoft Client ID，不需要 client secret。

### 8. 接入 CLI 与桌面基础工程

**CLI 交付：[x]** `client install/list/launch` 与 `account login/list/logout` 直接调用 Core API；profile 文件只保存公开身份并以原子替换更新，refresh token 使用系统凭证库。`--json` stdout 为单一 schema 版本化结果，进度和设备码写 stderr。安装和启动准备可由 Ctrl-C 取消；游戏在前台运行，stdout 日志转发至 stderr，退出前持有实例锁并清理 natives 会话目录。

**CLI 本地验收：** 2026-10-21 `cargo test --locked --offline -p anvil -p anvil-core`：1 项 CLI 单测、20 项 CLI 集成测试、62 项 Core 测试通过，1 项真实 Mojang metadata smoke 忽略。CLI 测试验证账户 profile 的原子存储、空客户端/账户列表的 JSON stdout、无效客户端名/Client ID 和缺少客户端错误；Core 测试验证重复安装结果与 LaunchSpec 生命周期内实例锁保持。Loopback fixture 测试需本地网络权限，本次仅访问回环地址；没有进行真实 Microsoft 登录、系统凭证库写入、Mojang 下载或客户端启动。

**桌面工程：** 已删除原 Slint `anvil-gui` crate 与桌面依赖。`apps/shell/src-tauri` 是 Cargo workspace 中的通用 Tauri 宿主；`apps/desktop` 是独立的 SvelteKit App npm workspace，通过 Shell 加载。前端使用 shadcn-svelte 组件配置和 Lucide 图标，并继承根 `tsconfig.json`。当前界面是静态壳，按钮未执行客户端操作；App 自己的业务组件和 Shell 的通用 Runtime API 均待设计/接入。当前 Shell crate 仍声明 Core 依赖，是骨架阶段的过渡状态。

**桌面验收：** 2026-10-22 拆分宿主与页面后，本地 `npm run desktop:check`（0 错误、0 警告）、`npm run desktop:build`、`cargo check --offline -p anvil-shell` 与 `cargo fmt --all -- --check` 通过；Tauri 生成 `target/release/anvil-shell`。尚未在 Tauri WebView 中运行；真实设备码登录、OS 凭证库、各 OS/CPU 的真实安装和游戏启动仍需维护者手动 smoke 逐一记录。Linux 桌面构建已加入 Live CI。

## 首批不包含

Fabric、Forge、NeoForge、整合包、账号共享/云同步、后台启动、MCP、自动迁移旧启动器数据和自动更新。每项需要独立的数据与生命周期方案。

## 依据

- [Minecraft 文件安装与启动设计依据](minecraft-installation.md)：官方元数据字段、平台规则、assets、natives 与参数差异。
- [Microsoft OAuth 设备码流程](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)：设备码展示、轮询错误、令牌刷新。
- [Xbox 服务登录流程](https://learn.microsoft.com/en-us/gaming/gdk/docs/services/fundamentals/s2s-auth-calls/service-authentication/live-website-authentication?view=gdk-2604)：Microsoft 令牌交换 Xbox User Token 与 XSTS 的字段和请求头。
- [整体架构](architecture.md)：Core/CLI 边界、下载预算、缓存与进程生命周期。
- [Java 管理](java-management.md)：托管运行时选择、安装和平台限制。
