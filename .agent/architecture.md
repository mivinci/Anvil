# Anvil 项目方向与架构约定

本文记录已确认的产品方向和工程边界，不代替实现计划。当前功能和验证范围以代码、[README](../README.md) 及 `todos/` 中的状态为准。

## 产品方向

- Anvil 是面向 Windows、macOS 和 Linux 的轻量 Minecraft Java Edition 启动与服务端管理工具。
- `anvil` 是 CLI 入口，服务终端用户、脚本和后续 AI 工具；`apps/shell` 是通用 Tauri 启动器，按 App ID 启动和维护版本；`apps/desktop` 是独立 App，拥有桌面界面和产品流程。桌面 Runtime 已提供版本目录、客户端安装/列表及账户管理 API；UI 重构后已接入窗口框架、外观设置、账户菜单/登录弹窗，以及客户端列表、安装、进度、暂停/继续、取消和显式重试；完成后联动刷新实例列表。客户端卡片启动与 Runtime 进程管理已接入，Microsoft 账户登录已完成并由维护者确认；启动和下载的平台验收范围见对应执行文档。
- 业务逻辑由 CLI 与 App 自己的业务组件使用 `anvil-core`。Shell 不依赖 Core，不承载 Anvil/Minecraft 领域命令；其他入口或 MCP 仍可复用同一核心，不重复实现安装和启动规则。
- 资源目标是按需运行，不设无明确需要的全局常驻服务。下载、解包、日志和并发资源应有界；各入口不因空闲而轮询或长期占用高资源。
- 当前已实现托管 Java 管理、原版服务端创建/列表/前台启动，以及原版客户端 Core 与 CLI 安装/列表/Microsoft 设备码登录/账户管理/前台启动流程。Microsoft 账户登录已由维护者确认完成，不再以早期应用注册 403 或等待审核作为当前限制。Fabric 客户端安装已实现；其他加载器、服务端后台托管和 MCP 仍未实现，客户端启动的平台验收范围见相关执行文档。
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

Core 的 `catalog_cache` 共用有界磁盘读写、原始获取时间、schema 与原子替换；来源适配器分别负责查询键和内容校验。Runtime 的 `serve/catalog` 共用首次恢复、并发合并与过期先用旧数据的策略：Minecraft 目录与按游戏版本划分的 Fabric 列表有效期 30 分钟，Modrinth 与 CurseForge 浏览目录有效期 5 分钟。Fabric 最多 8 个键、两种模组来源各最多 16 个键，内存与磁盘均有界；Modrinth 搜索按游戏版本/关键词/页缓存，兼容版本按游戏版本/项目缓存整表，翻页复用同一快照。无定时轮询，手动刷新强制获取，更新失败保留旧快照。前端只复用窗口状态及 Runtime 返回的有效期，不重新计时。模组来源仅持久化公开浏览 DTO，不保存 API Key、下载地址或安装计划；Modrinth 安装计划另取当前元数据，实际文件继续复用共享哈希仓库。

模组发现页迁移已完成只读全局 API：`/resources/{source}/projects` 和项目摘要支持无实例浏览，Minecraft 版本筛选可选，首期固定 Fabric；项目摘要、全局搜索和实例浏览共用每来源的 16 项缓存预算，带相同版本的搜索复用原键。`/mods` 页面框架已接来源与显式实例上下文，目标必须经 Runtime 详情读取确认；nav 进入不默选实例，无效/缺失/原版目标明确拒绝。项目搜索/滚动加载、摘要及实例兼容版本浏览已接入，宽窗口分栏、窄窗口同页切换；与实例页共用 PageSidebar，来源/目标/兼容条件位于可收起侧栏，两类页面分别保存窗口内展开状态；页内版本选择、依赖/缓存复用预览和确认提交已接入窗口级任务，实例添加入口已携带当前实例跳转到 /mods，旧发现弹窗已移除，本地管理保留在实例页；兼容版本与安装计划继续要求真实实例，迁移的七项实现与离线浏览器验收已完成，真实下载/原生 WebView/游戏加载的边界见 client-resources。

## 桌面日志

Shell 与 Runtime 入口共用 `anvil-logging`，开发模式写终端和文件，发行模式写文件；Core 仅产生 tracing 事件，不持有日志输出。文件轮转、队列与历史数量有界，Runtime stdout 保留为握手协议，日志使用 stderr。不得记录认证凭据、验证码或请求体。实例日志页只按需读取游戏自身 latest.log 的末尾快照（上限 256 KiB），不混入启动器日志，不设常驻日志流。桌面启动另以有界内存保留经认证信息过滤的 Java stdout/stderr（每任务 256 KiB、单行 8 KiB），日志页手动刷新查看；Runtime 结束或任务历史淘汰即清空。实例日志页另支持按需浏览最近 50 份崩溃报告，单份读取上限 2 MiB，不作自动诊断。实施状态与验收边界见[桌面日志执行项](../todos/desktop-logging.md)。

## 桌面游玩统计

Runtime 在客户端启动进入 `running` 时开始记录会话，游戏退出、取消或失败时按实际时长结算；小于 10 秒的会话只计启动次数，单局上限 24 小时。统计按账户 UUID 与实例 ID 聚合，离线测试单独记为 `offline`；删除实例和退出账户都保留历史，账户累计值独立于有界实例明细。数据保存在 Runtime 数据目录的 `play-stats.json`（schema 1，有界 512 KiB），原子替换，损坏文件备份后重建；只由 Runtime 生命周期写入，桌面通过鉴权的 `GET/DELETE /play-stats` 和 `DELETE /play-stats/accounts/{id}` 读取、全量清除或按账户清除，不接收前端计时、不云同步、不记录凭据。首页首行用玩家卡、Anvil 动态轮播和 Minecraft 官方动态三列展示；玩家卡常驻快捷游玩，活动实例优先，空闲读取当前账户统计中的最近有效实例，离线模式兼容本机最近记录，复用现有启动控件。Anvil 轮播目前使用内置编辑数据与 SVG 插画，没有远端新闻服务；自动切换尊重悬停、焦点、页面可见性与减少动态设置。实例列表在下方占满整行。设置的账户页提供当前账户和本机全部记录的确认清除入口；启动/运行中由前后端阻止清除，Runtime 在启动准入锁内完成持久化以防会话写回竞态。全量清除确认后同时清除本机最近游玩缓存，单账户清除保留其他账户与离线记录；响应不确定时只读核对，不自动重发。执行项与验收见[桌面首页玩家卡](../todos/desktop-home.md)。

## Runtime 自更新

- 安装包内的基线 App 版本高于活动版本时，Shell 在启动阶段先发布基线并标记为 pending，再由现有健康确认/回滚流程激活；同版本或更旧基线不覆盖活动版本。
- Shell 保持通用：启动时优先读取 `<app>/sidecar-store/current.json`（schema 1：`version`/`sidecar_entry`/`http_api_version`），校验失败或启动失败即回退包内种子，并把失败版本写入 `blocked.json`。Shell 不了解 Anvil 版本策略或下载渠道。
- App manifest 为 schema 2，新增 `http_api_compatibility`（VersionReq）：种子声明的 `http_api_version` 和 override 声明的版本都必须落在范围内。`serve` 侧以编译期常量校验 `ANVIL_HTTP_API_VERSION` 相等，握手不再只是回显。
- Shell 给 sidecar 传 `ANVIL_SIDECAR_STORE_DIR` 与 `ANVIL_HTTP_API_COMPAT`；`serve` 的自更新器在编译期注入索引 URL 与 Ed25519 公钥（`ANVIL_RUNTIME_UPDATE_MANIFEST_URL`/`ANVIL_RUNTIME_UPDATE_PUBLIC_KEY`，缺失即禁用），24 小时限频拉取签名 `runtime-index.json`，按版本、API 范围、拉黑列表过滤，下载裸二进制经 sha256 与 `--version`/`--serve-api-version` 门禁后发布到 `versions/<ver>/` 并原子写 `current.json`，下次启动生效。
- 校验强度与 Shell 的 App 更新通道对齐：Ed25519 签名索引 + HTTPS + 签名载荷钉死的 SHA-256。密钥经 `cargo run -p anvil --example gen_update_key` 生成，私钥存仓库 secret `ANVIL_RUNTIME_SIGNING_KEY`，公钥存仓库变量并在 release 构建时注入。

## Shell 重装更新

Shell 版本以 Tauri 安装包的 `tauri.conf.json` 版本为准，不使用 Rust crate 版本。Shell 数据根目录取 Tauri `identifier`（当前 `app.q2q.anvil`），并在首次启动时将旧 `mivinci/Anvil` 目录迁入；Desktop、Shell 与 Runtime 各使用唯一签名更新 feed。预发布通过 SemVer 后缀发布到同一 feed，stable 版本拒绝自动更新到预发布，预发布按 SemVer 顺序更新并可升级到 stable。清单绑定产品、SemVer、目标 triple 和对应 GitHub release 页面。Shell 只展示新版本并打开发布页，用户选择与当前安装方式匹配的安装器重新安装；Shell 不自动执行 MSI、NSIS、DMG、AppImage 或系统包。发行版每次启动检查，运行期间每小时尝试并由持久时间戳限频为 24 小时，About 页面允许手动检查；Dev 启动不自动检查。

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

后续详情页采用 `/clients/[name]`，mod 等不可变资源按内容哈希存于持久共享仓库，实例记录资源引用并生成独立运行入口；配置/世界等可变内容始终隔离。跨实例不重复下载，复制回退可能占用额外磁盘，不承诺所有平台零重复存储。引用事务、运行锁、共享传输取消隔离与显式清理需一起实施，详见[客户端资源设计](../todos/client-resources.md)。当前详情路由、可收起 sidebar、独立详情 API 与概览、本地 JAR 共享原件及实例资源引用管理已实现；Fabric 实例启用资源时已生成 game/mods 的独立副本，并记录所有权及可恢复事务；共享原件保留，原版实例不启用模组。新建实例的 Fabric Loader 查询/合并安装计划/校验下载已接入；Modrinth 与 CurseForge 浏览、受限必需依赖计划、共享校验下载与实例事务提交已实现；Modrinth 无需凭据，默认来源的真实只读元数据与计划检查通过，真实下载待验收；CurseForge 仍等待 API Key 联调；资源任务支持进度/速度/取消，暂停/恢复与多个消费者共享在途传输尚未实现；完整依赖约束求解、其他加载器仍未实现；桌面游戏启动已接入，真实 Fabric 安装/运行及资源事务恢复待验收。

### 本地资源包当前边界

Core 资源清单 schema 5 保留 resourcePacks、Modrinth 来源和版本依赖记录，新增独立 shaderPacks；旧 schema 1–4 兼容读取，成功变更才升级。资源包 ZIP 导入与模组共用内容摘要仓库、实例/仓库锁和修订检查，解析有界 pack.mcmeta，不推断游戏兼容性。Core 已共用现有资源事务应用到 game/resourcepacks，并在启动前恢复/补回受管副本，支持原版与 Fabric；Runtime 已提供鉴权资源包列表/上传/移除 API，UI 已接本地 ZIP 确认导入、列表与移除，概览按需读取受管引用数量，不改写游戏内资源包选择与顺序。执行项及验收状态见 client-resources。光影 Core 已接本地 ZIP 识别、共享引用、game/shaderpacks 独立副本、移除与启动前恢复；Runtime 光影列表/上传/移除与受管 Fabric 环境识别已接入鉴权和请求限额；有界识别嵌套模组和唯一提供者别名，同版本不同依赖契约的重复 ID、别名冲突或不支持的版本约束返回无法确认；嵌套库版本差异交给 Fabric 解析，UI 已接本地 ZIP 确认导入、搜索、同步、移除及环境提示，概览按需读取数量；Iris 兼容版本、依赖确认和窗口级安装任务已接入，提交前校验实际 JAR 的受管依赖环境；指定真实 Iris/Sodium 的隔离缓存提交，以及 Runtime 在线计划、q2q-http 下载、后台任务幂等提交和环境读取均已验收；使用临时游戏记录，游戏本体安装、启动及渲染仍待验收。另已接入 Modrinth 光影只读搜索/项目/版本与独立 5 分钟有界缓存，以及按版本生成单文件计划、窗口级后台下载、SHA-1 校验和实例导入；桌面新增 `/shaders` 导航页与实例目标侧栏，实例光影页保留本地导入并跳转该路由。CurseForge 光影和真实 WebView/游戏验收仍未完成。执行项与隔离验证范围见[实例光影管理](../todos/client-shaders.md)。

### 离线测试入口

登录弹窗左下角提供离线模式入口（仅开发构建显示），选择仅在当前窗口保留，成功登录或切换账户后恢复账户模式。桌面 launch 请求可显式选择 offlineTest，不提供 accountId；常规账户启动不自动降级。Core 测试会话只持有固定本地玩家和无认证能力的占位参数，不保存为 Microsoft 账户，不写凭据。继续使用同一实例/文件/Java/进程生命周期规则；这是开发验证入口，真实启动与版本兼容待验收。

### Fabric 类路径视图

共享下载对象保留无后缀哈希路径；Fabric 的 remapper 按扩展名扫描，因此启动准备在临时 session/classpath 生成带 .jar 后缀且校验过的客户端/类库独立副本，同会话按摘要去重。视图随运行准备句柄持有，正常退出/取消/准备失败自动清理，不改变持久安装记录；临时空间与复制开销属于该启动会话。原版沿用原类路径。

### 客户端 Java 准备时机

CLI 与桌面客户端安装流程在解析计划后通过 Core 的 prepare_install_java 复用/补装托管 Java，再下载与发布游戏文件；低层游戏文件安装与 JavaManager 各守职责。启动阶段仍验证 Java，桌面缺失时保留显式补装，不自动跳过检查。安装 Java 阶段可取消，游戏传输的暂停/继续保持原边界。启动任务记录是否真正开始 Java 安装，前端据此展示有界终态历史，避免把缓存探测当下载任务。

### 实例删除边界

桌面实例设置提供明确确认后的单实例删除，Runtime 鉴权 DELETE 接口调用 Core；Core 与安装、资源写入、运行持有同一实例锁，并取得设置写入锁，拒绝链接及越界路径。先原子移出可启动实例目录再清理，删除只涉及当前实例可变数据和副本，共享游戏/Java/资源缓存保留。清理失败或崩溃残留不自动删除，前端区分移除成功和清理待完成；请求结果未知先读取确认。隔离测试与真实验收边界见 desktop-client-library。

## macOS Runtime 签名

Runtime 使用固定 `app.q2q.anvil.runtime` 签名标识；本机与 Desktop/CLI 发布 CI 支持同一固定证书签名，未配置时保留 ad-hoc，配置错误则停止构建。CI 使用临时钥匙串导入仓库 Secrets，Tauri 外层构建复用身份，构建后校验内置与更新包 Runtime，再恢复搜索列表并清理钥匙串。当前配置为内测自签名证书，不代表 Developer ID、公证或跨版本钥匙串授权已验收；操作见 [macOS 签名](../docs/macos-signing.md)。

## 外观配色

显示模式（跟随系统/浅色/深色）独立于主题。当前提供潜影紫、末影黑、苦力怕绿、悦灵蓝四套预设，每套拥有浅/深配色，末影黑采用冷白/石墨黑与炭黑/银灰的中性配色，预设颜色固定，不受保存的自定义草稿影响；设置页只展示当前模式的色板。选择自定义才开放浅/深色的页面背景、卡片背景、文字及强调色编辑；切回预设保留自定义草稿，“基于此主题自定义”复制当前预设的浅/深色重新开始，并记录来源以供重置。颜色继续保存于本机 anvil.theme，不涉及账户或云同步；潜影紫沿用旧 default 标识，已有选择不变；预设与自定义共用一套颜色派生，确保复制后不会变色。

## 整合包导入

已确认在客户端“安装版本”左侧提供“导入”，选择本地 `.mrpack` 后由同一全局任务完成解析和安装，不增加确认弹窗；首期原版/Fabric。当前本地接入 Core index/ZIP、安装事务与共享资源、Runtime 上传/全局任务及桌面入口；真实下载、WebView 和游戏验收待维护者完成。Q2Q 的重定向 URL policy 已合入，Anvil 通过 git 依赖锁定其合并提交，Anvil 改动仍为本地未提交。按项实施与验收范围见[整合包导入](../todos/client-modpacks.md)。

## 整合包导出

已确认在实例概览启动按钮旁提供次级导出入口，默认导出模组、资源包、光影和配置，存档显式可选；运行中禁用，通过全局任务执行。当前完成 Core 选项/index 和有界只读快照打包、无覆盖发布与离线测试；Runtime 与界面未接入，见[实例导出整合包](../todos/client-modpack-export.md)。
