# 实例光影管理

状态：第 1–6 项完成，本地光影管理、Runtime 接口、受管 Fabric 环境识别、实例光影页和 Iris 安装确认已接入；隔离测试、指定真实 JAR 的缓存提交及 Runtime 在线资源安装链路验收通过，游戏加载与渲染尚未验收。在线光影发现的只读后端（A）已完成；在线下载/导入与页面入口（B）已接入窗口级计划/后台任务闭环，包进度、取消、离页继续和真实 Runtime loopback 下载/导入验收通过；UI 真实 WebView、CurseForge 和游戏加载仍待验收。逐项推进，每项结束汇报并确认下一项。

## 执行项

1. [x] **模型与操作边界**：确定文件识别、共享存储、运行目录、环境状态和安装确认规则。
2. [x] **Core 光影管理**：扩展清单及恢复事务，实现 ZIP 校验、导入、列表、移除和启动前同步。
3. [x] **环境识别与 Runtime API**：提供鉴权光影接口，识别原版/Fabric、受管且启用的 Iris/Sodium 及无法确认的状态。
4. [x] **实例光影页**：接入环境提示、本地 ZIP 导入确认、列表搜索、移除确认及概览真实数量。
5. [x] **安装光影支持**：为 Fabric 实例生成 Iris 及必需依赖的真实安装计划，确认后交给现有窗口级资源任务执行。
6. [x] **交付验证**：完成相称的静态检查与构建，整理状态；真实 ZIP、Iris 下载、游戏内选择及 macOS 渲染效果由维护者按需验收。

## 在线光影发现（A 已完成，B 已接入后台任务闭环）

A 只做只读发现，不下载、不改页面入口：

- Core 新增 `Modrinth::search_shaders`、`shader_project` 和 `shader_files`；搜索固定 `project_type:shader`，版本固定 Iris loader 与当前游戏版本，文件只接受唯一主 ZIP，校验 CDN HTTPS、大小和 SHA-1。
- 新增 `ShaderDiscoveryQuery`/`ShaderDiscoveryPage` 与 `modrinth-shader` 缓存命名空间；搜索、项目、版本分别有界缓存 5 分钟，磁盘命名空间与模组发现隔离，最多 16 项、2 MiB。
- Runtime 新增只读接口：`GET /resources/modrinth/shader-projects`（可选 `gameVersion`/`q`/`index`）、`GET /resources/modrinth/shader-projects/{project}`、`GET /resources/modrinth/shader-projects/{project}/versions?gameVersion=...`，沿用 Modrinth provider、并发门禁和 Origin/Bearer 鉴权。
- 验证：Cache 的 `shader_search_snapshot_round_trips_through_its_own_cache`、ZIP/Iris 选择和项目路由单测通过；`shader_discovery_live_metadata_smoke` 使用真实 Modrinth 搜索、项目 `HVnmMxH1` 和 1.21.1 版本列表通过（ignored，手动执行）。
B 已接入窗口级后台任务闭环：

- Core 新增 `shader_file`，返回指定版本的主 ZIP 与下载地址；新增 `ShaderInstallPlan` 和 `ResourceManager::install_shader`，复用共享对象下载/校验路径，下载完成后调用现有 `import_shader_pack`。
- Runtime 新增 `POST /clients/{name}/shader-plans`，body 为 `{ project, version, revision }`，返回带 `planId` 的单文件计划；通过现有 `POST /clients/{name}/resource-plans/{planId}/install` 启动窗口级资源任务。任务快照增加 `kind: shader`，复用 `/resource-operations` 的进度、取消、列表和离页继续；同一时间仍只允许一个资源安装任务。
- 桌面新增 `/shaders` 导航页和路由：侧栏选择实例，按该实例 Minecraft 版本搜索 Modrinth 光影、查看项目与 Iris 兼容版本；选择版本后生成单文件下载计划，在底部确认栏确认后交给窗口级任务，显示进度并可取消。实例光影页保留本地导入，并提供跳转到该实例的 `/shaders?instance=` 入口。
- 已通过 Core 真实 smoke 实际下载 Complementary 光影 ZIP，并校验字节数；Runtime `shader_api_live_plan_background_download_and_import` 通过真实 loopback 完成计划、后台下载、进度轮询、导入和清单读取。`cargo test`、Clippy、`svelte-check` 和桌面构建通过。
- 仍未做：UI 真实 WebView 验收、计划过期后的自动重生成提示、CurseForge 光影、游戏内加载。UI 已接进度/取消/离页任务，但未在原生 WebView 实际操作验证。

## 用户流程

光影页顶部显示当前实例的环境状态，下方管理已添加的 ZIP。即使尚未安装 Iris，也允许先导入和移除光影；“已添加”只表示文件已进入实例，不表示游戏内正在使用。

Fabric 实例缺少 Iris 时显示“安装光影支持”；已有受管 Iris 被停用时引导到模组管理，不重复安装。原版实例说明需要支持光影的运行环境，并链接现有新建 Fabric 实例流程，不转换当前实例或迁移世界。其他加载器暂不提供自动环境安装。

玩家在游戏中选择光影、启用或关闭效果及调整参数。首期不写入 `iris.properties`、`options.txt` 或光影配置文件，不提供含义不明的启动器“启用”开关。移除 ZIP 时保留游戏及光影配置，游戏内失效选择的处理由游戏负责。

## Core 与文件边界

- `resources.json` 从 schema 4 扩展到 schema 5，新增独立 `shaderPacks` 集合。条目保存本地来源、SHA-256、大小和安全文件名；不复用资源包格式字段或模组依赖/启停语义。
- 兼容读取 schema 1–4，缺少光影集合时视为空；读取不升级文件。各类资源共用 revision，任一成功变更保存其他集合，统一计算每实例最多 1024 条受管资源。
- 原件继续保存到 `resources/objects/sha256/<前两位>/<摘要>`；实例持有引用，运行目录为 `game/shaderpacks/`，使用独立副本。相同内容跨实例复用共享原件。
- 复用实例运行锁、仓库锁、目录所有权及事务恢复；准备、备份、提交、启动前恢复统一覆盖 mods/resourcepacks/shaderpacks。游戏运行或安装占用实例时拒绝变更，不承诺运行中热加载。
- 同内容重复导入幂等；同类型不同内容同名（忽略 ASCII 大小写）拒绝；不覆盖或收编非受管同名文件。外部修改的受管副本保持冲突，不能直接删除。移除只撤销当前引用与受管副本，保留共享原件。

### ZIP 识别

- 首期仅导入本地 `.zip`，沿用 64 MiB 上传和 4096 个归档条目的限制；超限明确拒绝。目录导入、嵌套外层目录自动修复及 ZIP64 暂不支持。
- 共用已有归档安全检查，拒绝绝对路径、父目录穿越、链接、重复或含糊的路径。不整体解压或执行包内内容。
- 根层必须存在 `shaders/` 下的实际着色器源文件，识别 `.vsh`、`.fsh`、`.gsh`、`.csh`、`.glsl` 等源码入口；仅有空目录、图片或嵌套 ZIP 不作为光影包接收。实现时按实际归档结构进行有界识别，不要求 `pack.mcmeta`。
- 结构识别仅用于判断候选光影包，不代表源码编译通过，也不从文件名推断 Minecraft、Iris 或显卡兼容性。本地包统一展示“兼容性未确认”，由实际游戏加载验收。
- 识别时有界读取一份候选源码（最多 4 MiB），要求为非空 UTF-8 文本；其余内容只检查归档路径和条目属性，不整体解压或验证 GLSL 语法。

## 环境识别

Core 从真实实例记录和受管资源状态提供环境摘要，不由页面猜测路径或模组文件名。

- 原版：`requires_fabric`；Fabric 且未发现启用的 Iris：`missing_iris`；发现受管但停用的 Iris：`iris_disabled`。
- 对受管 JAR 的 `fabric.mod.json` 作有界读取，按真实模组 ID 识别 `iris`、`sodium` 并返回版本与启停状态；不能仅依赖 Modrinth 项目名，因为也存在本地导入。
- 启用的 Iris 及其必需依赖经现有能力确认后才显示“已安装光影支持”，不声称保证光影或显卡兼容。元数据无法解析、重复 ID 或依赖无法确认时返回原因，不能把读取失败当成缺失并自动重装。
- 首期只确认 Anvil 受管模组；页面标明识别范围。非受管文件和其他光影加载器不标为已支持，自动安装前必须遵守现有目标目录冲突检查。
- 按需读取环境，不增加后台轮询；模组任务结束、切换实例或用户刷新时重新读取。
- 单次受管 JAR 声明大小合计最多 128 MiB，每份 `fabric.mod.json` 最多 256 KiB；超限、损坏或完整性失败返回 `unconfirmed` 和原因，不把它当作缺失。启用模组的实例副本也必须存在且摘要匹配。
- 识别 Fabric schema 1 的顶层模组和声明的嵌套 JAR；嵌套最多 4 层、合计 128 个，单份不超过 64 MiB，顶层和嵌套原始字节合计最多 128 MiB。另设内置库解压内容合计 128 MiB 预算，逐条流式摘要，不解压到磁盘。缺失声明目标、路径歧义/链接/加密、损坏归档、超限或无法解析的元数据返回 `unconfirmed`。
- 内置库来自多个父模组时，只有所有具名条目的解压内容完全相同才合并识别，可忽略 ZIP 压缩、时间戳和归档注释差异。不同内容或版本、顶层与嵌套同 ID，以及重复顶层 ID 均拒绝自动选择；不模拟 Fabric 的多版本求解。
- 嵌套模组继承父模组的启停条件，并检查自身必需依赖。`provides` 别名使用提供者的版本参与依赖与双向不兼容检查；别名与真实 ID、其他提供者或 Minecraft/Fabric/Java 标识冲突时返回 `unconfirmed`。
- 必需依赖从 Iris 的 `depends` 及实际加载的嵌套模组开始递归检查，包含 Minecraft、Fabric Loader 和实例声明的 Java 版本；不无条件要求 Sodium。检查必需依赖闭包的 `breaks`，以及其他启用模组对该闭包的反向不兼容声明。
- 版本约束支持普通稳定版（最多三个数字分量）、比较、`~`/`^`、数字通配符、空格 AND 和数组 OR；裸版本按精确版本处理，忽略版本 build 后缀。实际稳定版另可与普通 SemVer 预发布上下界比较（如 `>=1.15-alpha.19.39.a`）。实际预发布、快照和更多数字分量只接受明确相同的字面版本；其他未支持的范围和 Fabric 扩展约束返回 `unconfirmed`，不改写依赖。依据见 [Fabric 元数据规范](https://wiki.fabricmc.net/documentation:fabric_mod_json_spec)。
- 环境状态为 `requires_fabric`、`missing_iris`、`iris_disabled`、`missing_dependencies`、`unconfirmed`、`ready`；`ready` 仅表示受管 Iris 的已支持依赖检查通过，不等于整个模组集合、光影包或 GPU 已通过实际运行验证。

## Runtime 与页面

- 新增 `/clients/{name}/shader-packs` 的 GET、POST 与按摘要 DELETE；POST 携带安全文件名及 revision，沿用鉴权、64 MiB 请求上限和资源请求并发门禁。页面不提交磁盘路径。
- GET 在同一次实例/仓库锁内读取 revision、appliedRevision、光影列表及环境摘要，避免混合不同修订。资源同步沿用既有入口，并覆盖新增集合；环境无法确认不抹去已读取的光影列表。
- POST/DELETE 返回已提交的 revision、appliedRevision 和 packs，不携带环境。页面在写入成功后显式 GET 刷新环境；环境读取失败不能把已经成功的上传/移除误报成写入失败，也不能触发重复提交。
- 复用资源包页面的紧凑列表、搜索、ZIP 确认、错误恢复和未知结果刷新规则；共享组件只抽取实际重复部分，不把两种资源的元数据校验混在一起。
- 概览按需显示真实光影数量，读取失败不显示 0；空态提供本地导入入口。环境安装任务离页继续，完成后刷新光影环境与模组集合。

## Iris 安装

“安装光影支持”使用 Modrinth 的真实 Iris 项目标识，为当前实例的 Minecraft/Fabric 条件选择可安装版本，再复用现有必需依赖解析和安装计划；Sodium 等依赖由选定版本的元数据决定，不硬编码版本号或无条件重复添加。

确认区展示拟安装版本、必需依赖、已有资源复用及下载量。已有 Iris/Sodium、禁用条目、本地同 ID 模组或不兼容依赖不能静默替换；在现有受限依赖计划无法安全处理时明确拒绝，交由玩家管理模组后重试，不扩展成新的完整依赖求解器。

确认后使用已有窗口级资源任务、q2q-http 下载、摘要校验和实例事务。请求结果未知时先读取确认；网络或计划失败不改变光影 ZIP 或已有环境。没有可用版本时说明原因，不降级下载其他 Minecraft 版本。

## 验证边界

后续实现重点检查旧清单兼容、不同资源互不覆盖、ZIP 结构及路径边界、重复导入、文件所有权冲突、运行锁、事务恢复和未知结果。环境识别覆盖本地/在线来源、启停及损坏元数据；Iris 安装确认覆盖计划过期与已有模组冲突。

静态检查或隔离 fixture 不等于真实光影可用。实际 Minecraft 启动、游戏内选择、依赖加载及不同 GPU 的渲染效果分别记录。自动验证只使用临时实例，不修改用户实例或游戏配置。

## 当前实现

Core 的光影集合与事务接入 schema 5、layout 光影集合及 journal schema 3，兼容旧清单和 journal；Modrinth/CurseForge 操作保留光影引用。`ResourceManager::shader_packs` 在锁内读取一致的清单与环境摘要。

光影页显示环境、受管 Iris/Sodium 版本及启停状态，支持 ZIP 导入确认、搜索、同步和移除确认。原版仍可管理 ZIP，并提供客户端入口；已有但停用的 Iris 或不完整依赖引导到模组管理。概览通过一次资源清单查询读取资源包及光影数量，失败显示暂不可用。

缺少 Iris 时展开兼容版本及安装确认区，固定查询官方项目 `YL57xq9U`，复用现有版本过滤、分页、必需依赖计划和确认组件。确认区可展开各文件版本、依赖、缓存复用及下载量；无兼容版本显示空态，不更换 Minecraft 版本。游戏运行或资源变更期间禁止提交。

Core 对以 Iris 为直接项目的 Modrinth 计划统一执行检查：现有环境必须能确认缺少 Iris，不能重新启用已有禁用依赖，项目版本冲突沿用原有规则拒绝。下载并核对摘要后，在实例事务提交前扫描候选实际 JAR；重复 ID、依赖不匹配或无法确认时拒绝提交，已有实例资源和光影 ZIP 保留。已校验对象可能留在共享缓存，不等于安装成功。

安装任务由窗口层持有，关闭确认区或离页不会取消；完成后按需刷新环境，不增加页面轮询。提交响应丢失时读取同一任务，已查到任务即显示已提交；仍未知时保留原计划用于幂等重试，禁止换版本或刷新版本列表丢失原计划。明确拒绝、过期和失败分别提供重试或重新生成入口。

## 已完成验证

- Core：光影专项 26 项离线测试通过，覆盖清单兼容、ZIP 边界、共享与副本隔离、运行锁、修订冲突、事务恢复、真实 ID、依赖及反向不兼容、嵌套深度/数量/字节预算、不同 ZIP 编码的相同库、别名冲突、Iris 成功提交与冲突拒绝、取消和陈旧计划。普通测试不请求在线平台。
- Runtime：2 项 loopback HTTP 测试通过，覆盖鉴权、上传/删除、共享门禁、revision、文件名/ZIP、64 MiB 正文上限及不存在实例。
- 前端：14 项离线测试通过，覆盖本地文件限制、清单与环境校验、已提交写入后的读取失败、旧响应/离页取消、计划身份、过期、重复确认、未知提交及窗口任务阻塞。
- 隔离浏览器：内存 Runtime fixture 检查光影列表、搜索、导入/移除与失败恢复、数量及小窗口布局；Iris 兼容版本、依赖/缓存详情、确认后离页继续、返回后就绪、无版本、计划冲突、过期，以及响应丢失后的同任务恢复和已提交显示。
- 静态检查：Core/CLI 全目标 Clippy（`-D warnings`）、Rust 格式、Svelte 检查（0 errors / 0 warnings）及桌面生产构建通过。

### 指定真实 JAR 验收

通过 [Modrinth Iris 项目](https://api.modrinth.com/v2/project/iris) 核对项目 ID；[1.21.1 Fabric 版本元数据](https://api.modrinth.com/v2/project/YL57xq9U/version?loaders=%5B%22fabric%22%5D&game_versions=%5B%221.21.1%22%5D) 和 [Sodium 版本元数据](https://api.modrinth.com/v2/version/26nVNc41) 提供文件身份。下载官方 Iris `1.8.8+mc1.21.1`（`zsoi0dso`）和 Sodium `0.6.9+mc1.21.1`（`26nVNc41`），核对 SHA-1 后，在临时缓存和实例中运行 Core 计划、缓存复用、提交及环境读取，结果为 `ready`。验证识别了真实嵌套 Fabric API、Sodium 的 `indium` 别名和嵌套模块的预发布版本下界。

`shader_install.rs` 中的 `shader_install_official_iris_and_sodium_in_isolated_instance` 默认 ignored，显式设置 `ANVIL_SHADER_SMOKE_IRIS`、`ANVIL_SHADER_SMOKE_SODIUM` 为上述官方文件路径后用 `--ignored` 运行。测试固定校验摘要，不自动下载，不运行 JAR，不接触用户实例。

### 在线资源安装链路

Core 的 `shader_install_live_modrinth_plan_download_and_commit` 从空缓存读取官方 Iris `1.8.7+mc1.21.1`（`A99vfwJc`）计划，由其元数据选择固定 Sodium 依赖 `0.6.9+mc1.21.1`（`26nVNc41`）；通过生产 `q2q-http` 路径实际下载、校验和提交，环境读取为 `ready`。

Runtime 的 `shader_api_live_plan_background_download_and_idempotent_submit` 通过真实 loopback HTTP、Origin/Bearer 鉴权和页面同款接口执行：先导入测试 ZIP，再在线生成计划、两次提交同一个 plan ID、轮询同一个后台任务直到完成，读取最终光影环境及清单。确认只有一个任务、实际接收字节大于 0、两个模组提交完成、revision 从 1 变为 2、原光影 ZIP 保留。Core 与 Runtime 在线验收均通过。

这两项用临时安装记录声明 Minecraft 1.21.1 / Fabric 0.16.10，实际下载的是模组，不下载或启动 Minecraft 本体。数据、缓存和任务都随临时实例清理，不接触用户实例。在线测试默认 ignored，普通 CI 不联网；显式使用：

```sh
ANVIL_SHADER_LIVE_SMOKE=1 cargo test --locked -p anvil-core shader_install_live_modrinth -- --ignored
ANVIL_SHADER_LIVE_SMOKE=1 cargo test --locked -p anvil shader_api_live -- --ignored
```

## 尚未验收与限制

全量并行验证曾出现一次临时实例锁 `Busy`，之后在允许 loopback 的环境复验未重现；未据此修改生产锁规则。沙箱禁止本地监听导致的 HTTP fixture 失败不作为业务逻辑失败。

在线验收覆盖上述固定版本及其元数据固定的依赖组合，不代表所有 Iris/Minecraft 版本都兼容。未固定版本的项目依赖仍沿用现有受限解析器；若它选到与实际 JAR 约束不符的版本，提交前明确拒绝，不自动寻找替代版本。

真实光影 ZIP、原生 WebView 操作、Minecraft/Fabric 启动、游戏内选择、不同 GPU 和 macOS 渲染效果仍由维护者验收。`ready` 只代表已支持规则下的受管依赖检查通过。
