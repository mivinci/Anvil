# Anvil 项目方向与架构约定

本文记录已确认的产品方向和工程边界，不代替实现计划。当前功能和验证范围以代码、[README](../README.md) 及 `todos/` 中的状态为准。

## 产品方向

- Anvil 是面向 Windows、macOS 和 Linux 的轻量 Minecraft Java Edition 启动与服务端管理工具。
- `anvil` 是 CLI 入口，服务终端用户、脚本和后续 AI 工具；`apps/shell` 是通用 Tauri 启动器，按 App ID 启动和维护版本；`apps/desktop` 是独立 App，拥有桌面界面和产品流程。桌面 Runtime 已提供版本目录、客户端安装/列表及账户管理 API；UI 重构后已接入窗口框架、外观设置、账户菜单/登录弹窗，以及客户端列表、安装、进度、暂停/继续、取消和显式重试；完成后联动刷新实例列表。客户端卡片启动与 Runtime 进程管理已接入，真实启动、下载、登录验收仍待完成。
- 业务逻辑由 CLI 与 App 自己的业务组件使用 `anvil-core`。Shell 不依赖 Core，不承载 Anvil/Minecraft 领域命令；其他入口或 MCP 仍可复用同一核心，不重复实现安装和启动规则。
- 资源目标是按需运行，不设无明确需要的全局常驻服务。下载、解包、日志和并发资源应有界；各入口不因空闲而轮询或长期占用高资源。
- 当前已实现托管 Java 管理、原版服务端创建/列表/前台启动，以及原版客户端 Core 与 CLI 安装/列表/Microsoft 设备码登录/账户管理/前台启动流程。真实账户登录、系统凭证库和客户端启动仍待维护者实测；模组加载器、服务端后台托管和 MCP 仍未实现。
- 只对实际验证过的 OS、CPU 架构及 Minecraft 版本宣称支持。Linux 成功不能替代 Windows 或 macOS 的运行证据。

## Workspace 与 crate 边界

```text
Anvil/
  Cargo.toml                # workspace、共享依赖和构建配置
  Cargo.lock
  crates/
    anvil-core/              # 公共业务、安装、存储、Java 与服务端生命周期
    anvil/                   # CLI 参数、终端交互、JSON/JSONL、退出码；`serve` 子命令承载桌面 App Runtime
    anvil-logging/           # Shell/Runtime 共用日志配置与文件输出，不依赖 Core
  apps/
    shell/                    # 通用 Tauri App 启动器与版本管理器
      src-tauri/             # Cargo workspace：Rust 原生宿主
    desktop/                 # npm workspace：桌面 App 的 SvelteKit UI
    mobile/                  # 未来独立布局的 SvelteKit 移动 App
  todos/                     # 方案、阶段任务及验证记录
```

- Cargo workspace 管理 Rust crates 与 Tauri 宿主，npm workspace 管理前端包；两者分别维护 `Cargo.lock` 与根 `package-lock.json`。根 `tsconfig.json` 提供前端共享选项，应用配置通过 `extends` 继承。
- `apps/` 放可独立构建和运行的 App；`packages/` 只放被多个 App 实际复用的 npm 库。当前没有 `packages/` 目录；出现共享 API 合约或其他真实复用代码时再创建，不预先建立空包。
- `anvil-core` 不依赖 UI 框架、CLI 参数解析、终端交互或 MCP，也不自行创建 Tokio runtime。由每个入口拥有 runtime 和业务上下文。桌面 App 的 Runtime 是 `anvil` CLI 的 `serve` 子命令，作为独立进程复用 Core；它与 npm 前端包解耦，随桌面 App 包分发。方案与 phase 划分见[Runtime 合入 CLI 执行项](../todos/runtime-into-cli.md)。
- `apps/mobile` 未来使用独立的 SvelteKit 页面、布局和交互，不复用 `apps/desktop` 的页面结构；可以共享 API schema 和无界面逻辑。
- CLI 与 App 自己的业务组件可调用 core，不通过启动 CLI 子进程实现常规操作。Shell 只负责通用 App 生命周期和版本维护，不直接调用 core。前端使用 SvelteKit 静态输出，避免依赖 Node 服务端运行时。
- 保持 crate 和模块按现有职责组织。新增 crate、公共 trait 或通用抽象前，先确认当前需求确实需要它。

## 实例启动配置

实例根目录的 launch-settings.json 保存最大/初始内存及可选游戏窗口尺寸，Core 负责校验、原子写入与 revision 并发控制；桌面与 CLI 启动共用配置版参数构建。配置在启动准备阶段读取一次，运行中保存仅影响后续启动。当前无全局继承、任意 JVM 参数或用户 Java 路径。

## 版本目录缓存

Core 的 `catalog_cache` 共用有界磁盘读写、原始获取时间、schema 与原子替换；来源适配器分别负责查询键和内容校验。Runtime 的 `serve/catalog` 共用首次恢复、并发合并与过期先用旧数据的策略：Minecraft 目录与按游戏版本划分的 Fabric 列表有效期 30 分钟，CurseForge 项目搜索/文件列表有效期 5 分钟。Fabric 最多 8 个键、CurseForge 最多 16 个键，内存与磁盘均有界；无定时轮询，手动刷新强制获取。更新失败保留旧快照。前端只复用窗口状态及 Runtime 返回的有效期，不重新计时。CurseForge 仅持久化公开浏览 DTO，不保存 API Key、下载地址或安装计划；安装与启动仍走独立的元数据及文件校验。

## 桌面日志

Shell 与 Runtime 入口共用 `anvil-logging`，开发模式写终端和文件，发行模式写文件；Core 仅产生 tracing 事件，不持有日志输出。文件轮转、队列与历史数量有界，Runtime stdout 保留为握手协议，日志使用 stderr。不得记录认证凭据、验证码或请求体。实例日志页只按需读取游戏自身 latest.log 的末尾快照（上限 256 KiB），不混入启动器日志，不设常驻日志流。桌面启动另以有界内存保留经认证信息过滤的 Java stdout/stderr（每任务 256 KiB、单行 8 KiB），日志页手动刷新查看；Runtime 结束或任务历史淘汰即清空。实例日志页另支持按需浏览最近 50 份崩溃报告，单份读取上限 2 MiB，不作自动诊断。实施状态与验收边界见[桌面日志执行项](../todos/desktop-logging.md)。

## Runtime 自更新

- Shell 保持通用：启动时优先读取 `<app>/sidecar-store/current.json`（schema 1：`version`/`sidecar_entry`/`http_api_version`），校验失败或启动失败即回退包内种子，并把失败版本写入 `blocked.json`。Shell 不了解 Anvil 版本策略或下载渠道。
- App manifest 为 schema 2，新增 `http_api_compatibility`（VersionReq）：种子声明的 `http_api_version` 和 override 声明的版本都必须落在范围内。`serve` 侧以编译期常量校验 `ANVIL_HTTP_API_VERSION` 相等，握手不再只是回显。
- Shell 给 sidecar 传 `ANVIL_SIDECAR_STORE_DIR` 与 `ANVIL_HTTP_API_COMPAT`；`serve` 的自更新器在编译期注入索引 URL 与 Ed25519 公钥（`ANVIL_RUNTIME_UPDATE_MANIFEST_URL`/`ANVIL_RUNTIME_UPDATE_PUBLIC_KEY`，缺失即禁用），24 小时限频拉取签名 `runtime-index.json`，按版本、API 范围、拉黑列表过滤，下载裸二进制经 sha256 与 `--version`/`--serve-api-version` 门禁后发布到 `versions/<ver>/` 并原子写 `current.json`，下次启动生效。
- 校验强度与 Shell 的 App 更新通道对齐：Ed25519 签名索引 + HTTPS + 签名载荷钉死的 SHA-256。密钥经 `cargo run -p anvil --example gen_update_key` 生成，私钥存仓库 secret `ANVIL_RUNTIME_SIGNING_KEY`，公钥存仓库变量并在 release 构建时注入。

## 下载与文件安全

- Anvil 通过 git 分支依赖的 `q2q-http` 复用 Q2Q 提供的 HTTP Executor、并发下载、哈希校验和恢复能力（`Cargo.lock` 锁定精确提交）。不要另写 Range worker、重试队列或下载恢复格式。
- 官方元数据与 Artifact 的可信哈希决定内容身份。传输完成不等于校验通过；校验、原子发布和安装记录提交是不同阶段。
- 缓存放可验证、可重建的共享内容；实例目录放世界、配置和会话等可变数据。安装 staging 与最终发布应避免让半成品被识别为已安装。
- 解包、下载、元数据读取和日志缓冲设定上限；取消操作要等待它拥有的实际 I/O 与清理工作完成。

## CLI 与进程生命周期

- `--json` 为短操作输出单一 schema 版本化 envelope，保持 stdout 可解析；进度和诊断写 stderr。
- 长时间运行的 `server start` 可用 `--json-events` 输出 JSONL 生命周期事件。只有观察到服务端 Ready 且进程正常结束，才将启动记录为成功；服务端日志不混入 JSON stdout。
- 客户端游戏进程由 CLI 前台持有，进程 stdout 转发到 stderr，使 `--json` stdout 保持为单一结果。运行句柄持有临时 natives 和实例锁直到游戏退出；客户端进程取消时终止子进程并清理会话目录。
- `--accept-eula` 表达用户明确接受，不能自动接受。测试 EULA 仅用于隔离的临时验收数据。
- 服务端当前以前台方式运行，Anvil 持有进程和实例锁，并通过控制台 `stop` 或处理 Ctrl-C 请求优雅停止。不要在未设计后台进程所有权和跨命令 stop 前引入守护进程。
- 原版服务端不需要玩家微软账户。在线认证是否开启属于服务端配置；真实 smoke 默认保持 `online-mode=true`，测试端口只绑定 loopback。

## 验收与能力边界

- 普通 push/PR CI 运行本地测试 fixture，不下载真实 Minecraft 服务端。真实服务端跨平台验收单独手动触发并默认关闭。
- 记录 smoke 的精确 OS、CPU、Minecraft 版本、Ready、停止和保存结果。认证服务连通性单独说明；没有玩家登录验证时，不声称认证成功。
- 阶段执行清单和已验证结果见 [服务端实施项](../todos/server-milestone.md)。Java 安装来源和平台差异见 [Java 管理](../todos/java-management.md)，Minecraft 官方元数据和文件结构依据见 [安装设计依据](../todos/minecraft-installation.md)。

## 客户端资源管理（设计中）

后续详情页采用 `/clients/[name]`，mod 等不可变资源按内容哈希存于持久共享仓库，实例记录资源引用并生成独立运行入口；配置/世界等可变内容始终隔离。跨实例不重复下载，复制回退可能占用额外磁盘，不承诺所有平台零重复存储。引用事务、运行锁、共享传输取消隔离与显式清理需一起实施，详见[客户端资源设计](../todos/client-resources.md)。当前详情路由、可收起 sidebar、独立详情 API 与概览、本地 JAR 共享原件及实例资源引用管理已实现；Fabric 实例启用资源时已生成 game/mods 的独立副本，并记录所有权及可恢复事务；共享原件保留，原版实例不启用模组。新建实例的 Fabric Loader 查询/合并安装计划/校验下载已接入；CurseForge 浏览、受限必需依赖计划、共享校验下载与实例事务提交已实现但等待 API Key 联调；资源任务支持进度/速度/取消，暂停/恢复与多个消费者共享在途传输尚未实现；完整依赖约束求解、其他加载器仍未实现；桌面游戏启动已接入，真实 Fabric 安装/运行及资源事务恢复待验收。

### 本地资源包当前边界

Core 资源清单 schema 3 在 mods 之外保存 resourcePacks，旧 schema 1/2 按空集合读取，成功变更才升级。资源包 ZIP 导入与模组共用内容摘要仓库、实例/仓库锁和修订检查，解析有界 pack.mcmeta，不推断游戏兼容性。Core 已共用现有资源事务应用到 game/resourcepacks，并在启动前恢复/补回受管副本，支持原版与 Fabric；Runtime 已提供鉴权资源包列表/上传/移除 API，UI 已接本地 ZIP 确认导入、列表与移除，概览按需读取受管引用数量，不改写游戏内资源包选择与顺序。执行项及验收状态见 client-resources。

### 离线测试入口

登录弹窗左下角提供离线模式入口，选择仅在当前窗口保留，成功登录或切换账户后恢复账户模式。桌面 launch 请求可显式选择 offlineTest，不提供 accountId；常规账户启动不自动降级。Core 测试会话只持有固定本地玩家和无认证能力的占位参数，不保存为 Microsoft 账户，不写凭据。继续使用同一实例/文件/Java/进程生命周期规则；这是开发验证入口，真实启动与版本兼容待验收。

### Fabric 类路径视图

共享下载对象保留无后缀哈希路径；Fabric 的 remapper 按扩展名扫描，因此启动准备在临时 session/classpath 生成带 .jar 后缀且校验过的客户端/类库独立副本，同会话按摘要去重。视图随运行准备句柄持有，正常退出/取消/准备失败自动清理，不改变持久安装记录；临时空间与复制开销属于该启动会话。原版沿用原类路径。

### 客户端 Java 准备时机

CLI 与桌面客户端安装流程在解析计划后通过 Core 的 prepare_install_java 复用/补装托管 Java，再下载与发布游戏文件；低层游戏文件安装与 JavaManager 各守职责。启动阶段仍验证 Java，桌面缺失时保留显式补装，不自动跳过检查。安装 Java 阶段可取消，游戏传输的暂停/继续保持原边界。启动任务记录是否真正开始 Java 安装，前端据此展示有界终态历史，避免把缓存探测当下载任务。
