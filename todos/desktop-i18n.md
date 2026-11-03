# 桌面国际化

状态：第 1–10 项已在本地实现，第 12 项文档已整理：语言基础层、设置入口、壳层/导航、首页、实例列表、安装与启动、实例详情、本地资源管理和在线模组/光影发现及安装确认。外观、账户与关于页及更新提示已迁移；Runtime 应用错误已补稳定码；Shell 恢复页已按系统语言本地实现；格式化/复数、无障碍名称与双语窄窗布局已检查；文案守卫已接入 CI，综合浏览器回归已用双语通过；原生 WebView 最终验收待实测。支持 zh-CN/en，手动选择优先、默认跟随系统，无法匹配回退 zh-CN。

**迁移范围：** 包含当前已合入的光影、local-pack 校验、整合包导入/导出及 Shell 兼容性提示；开始各项时按实际页面和 Runtime 合约核对清单。

## 范围

- 纳入：`apps/desktop` 的页面、组件、组件内状态模块；`apps/shell/bootstrap` 恢复页；Runtime 通过 HTTP/DTO 返回给桌面 UI 的错误、状态、警告和失败原因。
- 不纳入：CLI 输出与 `--json`（当前为英文机器输出）、tracing 日志、Minecraft/模组/崩溃报告/游戏日志等第三方内容、用户输入与文件名、Shell 原生窗口标题与更新 feed 文本。
- 首期语言仅 zh-CN 与 en。新增语言时同时更新 Locale 列表、系统语言匹配规则和两套字典。

## 原则

- zh-CN 为源语言并导出 `Messages` 类型；en 声明 `: Messages`，缺键或多键由 `npm run desktop:check` 失败。字典按域拆分子文件，不写成单个数千行文件。
- 不引入第三方 i18n 依赖；沿用 Q2Q platform 的手写字典、`Intl` 格式化与下拉语言选择模式。
- 产品名（Anvil、Minecraft、Fabric、Iris、Sodium、Modrinth、CurseForge、JVM、JAR、ZIP 等）保留原文；占位符、单位和标点按目标语言处理。
- 禁止用字符串拼接组装句子；插值、数量和复数写成字典函数。同一中文句子在不同语境分别建 key，英文按语境翻译。
- 英文文案人工审校，不做逐字机翻；新增或修改文案必须双字典同步。
- 无障碍文本（`aria-label`、`title`、`placeholder`、`alt`、`sr-only`）与可见文本同等对待。

## 与 Q2Q platform 的差异

- Q2Q 是 SSR：Cookie、`Accept-Language`、`hooks.server.ts`、`page.data.locale`、`%q2q.locale%`。Anvil 桌面是 `ssr=false` 的静态 SPA，没有服务端请求上下文；偏好保存在客户端，默认值来自 `navigator.languages`，切换即时生效，不需要 Cookie 或重载。
- Anvil 的 `app.html` 不会由服务端替换语言；Tauri CSP 为 `script-src 'self'`，不能内联脚本。首帧语言与启动提示需要 `apps/desktop/static/` 下的外部脚本在 hydration 前处理。
- Q2Q 字典约 90 个 key；Anvil UI 约 6000+ 汉字，必须按域拆分。
- Anvil Runtime 保留原始诊断句子，新增 code/errorCode、光影 reasonCode/reasonParams；UI 按码翻译，原文仅供详情。Q2Q 使用服务端错误 key，思路相同。

## 执行项

0. [x] **范围与决策**：zh-CN + en；已保存偏好 > 系统语言 > zh-CN；Runtime 可见文案纳入；CLI 不纳入；光影页与整合包纳入。回退语言集中在单一常量，后续调整不扩散。
1. [x] **词典与 locale 基础层**：新增 `apps/desktop/src/lib/i18n/`，包含 `index.ts`（`locales`、`localeNames`、`isLocale`、`resolveLocale`、`messages`）、`zh-CN.ts`（源语言与 `Messages`）、`en.ts`（`: Messages`），按 `common`/`nav`/`home`/`clients`/`instance`/`mods`/`settings`/`account`/`errors` 拆分子文件。新增 `locale.svelte.ts`：`$state` 单例、`setLocale`、`localStorage` 键 `anvil.locale`、同步 `document.documentElement.lang`；读取或写入失败要有明确降级，不能吞掉。`resolveLocale` 支持 `zh`/`zh-Hans`/`zh-CN`/`en`/`en-US`，`navigator.languages` 为空时回退 `navigator.language`。首帧通过 `static/` 外部脚本设置 `lang` 与启动提示，CSP 不放开 `unsafe-inline`。新增 `formatBytes`/`formatCount`/`formatDate`/`formatDateTime`/`plural`，全部接收 locale；保留现有十进制 KB 与二进制 KiB 的语义差异，只本地化数字格式，不借机改单位。新增 `apps/desktop/tests/i18n.test.ts` 覆盖回退、格式化和键一致性。
2. [x] **设置入口与语言切换**：在 `/settings` 新增“通用”分类（语言与后续偏好），或与维护者确认放在“外观”下；语言选择使用下拉/单选，语言名带 `lang`，切换即时生效、不重载、不丢失当前页面状态。设置搜索关键词与占位说明同步进字典。`localStorage` 不可用时提示“本次选择仅在当前窗口生效”。
3. [x] **公共壳层与导航**：导航、标题栏、启动状态、更新按钮、错误边界、通用动作（取消/保存/重试/刷新/搜索/关闭/导入/移除）、空态、ComingSoon，以及所有页面 `<title>` 模板。壳层完成后应无字典外的中文。
4. [x] **首页、实例库、安装与启动**：`/`、`/clients`、实例卡片、安装弹窗与进度、Java 进度、启动控制、最近游玩、新闻、删除实例。把 `client-install-state`、`client-library-state`、`client-launch-state`、`client-removal-state`、`recent-plays` 等状态模块的文案迁出。时间、文件大小、下载速度按 locale 格式化。
5. [x] **实例详情、设置、日志、本地资源管理**：概览计数、实例设置、日志与崩溃报告、实例模组列表、本地资源包/光影页面和共享 `local-pack.ts` 校验错误；整合包导入/导出面板和任务状态。在线光影发现与 Iris 共用安装确认组件归第 6 项统一迁移。
6. [x] **在线模组/光影发现与安装**：包含光影发现页与 Iris 共用的版本选择、依赖计划确认组件；`/mods`、搜索与筛选、详情、兼容版本、计划确认、窗口级任务进度、项目统计与更新时间。`mod-plan-warnings.ts` 已按新增 warningDetails 的稳定 code 翻译和折叠；旧 Runtime/未知警告保留可见提醒与原文详情。
7. [x] **设置、账户、关于、更新**：`/settings` 分类与搜索、外观设置、关于页、账户菜单/登录弹窗/离线测试入口、更新按钮。账户错误与状态全部可双语。
8. [x] **Runtime 可见文案结构化**：分两步。
   - 8a（页面迁移前的契约）：
     - [x] 安装计划保留旧 `warnings: Vec<String>`，新增 `warningDetails`（稳定 code + 原始 message），覆盖 Modrinth/CurseForge 并随 Runtime 扁平 DTO 输出；页面不再匹配中文原文。
     - [x] 复用启动 errorCode，为安装/整合包导入、资源安装、整合包导出与账户登录新增 errorCode；光影环境新增 reasonCode/reasonParams，旧 reason 保留。
   - [x] 8b（收尾前）：`ApiError` 的 JSON 增加稳定 `code`，为现有构造点逐一赋码；操作 DTO 增加 `errorCode`；前端 `RuntimeRequestError` 暴露 code，按字典翻译；未知 code 使用通用文案，原始 detail 放进可展开的“技术详情”，不直接作为英文主文案。字段只增不改，旧 Runtime 缺 code 时回退；不提高 `ANVIL_HTTP_API_VERSION`。当前光影代码已合入，不再等待该前置条件。
9. [x] **Shell 恢复页与启动提示**：`apps/shell/bootstrap/index.html` 按系统语言显示中/英文案并设置 `lang`；恢复页与 App 页面可能不同源，不能依赖 `anvil.locale`。先确认 Tauri CSP 对内联脚本和内联事件的实际效果，优先使用外部脚本或静态双语排版，不放开 CSP。覆盖包缺失、包损坏、版本不兼容三种恢复场景。
10. [x] **格式化、复数、布局与无障碍**：替换硬编码的 `zh-CN`/`en-US` 调用；英文复数与中文量词；英文文案通常更长，检查固定高度按钮、168px 设置侧栏、卡片截断和窄窗口；`aria-label`、`title`、`placeholder`、`alt`、`sr-only` 全部进字典，语言名用 `lang` 标注。
11. [ ] **测试与验收**：自动化部分完成，原生手测待验收。
   - [x] 综合浏览器脚本使用实际词典选择器，按 zh-CN/en 分别运行；旧首页断言已匹配玩家卡与实例库结构，保留假任务/未知结果重试/删除冲突与只读确认测试。
   - [x] 全量 Desktop src 汉字守卫与中英文词典键/类型/插值参数校验已接入 CI；静态双语启动文字严格匹配词典，不整文件豁免。四项守卫拒绝边界测试通过。
   - [x] 95 项 Desktop Node、3 项 Shell Node、Svelte 检查、production build、双语综合脚本及 60 组语言/布局矩阵通过；本项无 Rust 变更，不重复第 8–9 项已记录的 Rust 验证。
   - [ ] macOS 原生 WebView：首帧、持久化、语言切换和英文布局。
   - [ ] Windows 原生 WebView：首帧、持久化、语言切换和英文布局。
12. [x] **文档与状态**：README 补充简短用户语言说明；`.agent/architecture.md` 记录语言与错误契约；测试 README 提供验收命令；[国际化维护](../docs/i18n.md) 集中说明实现位置、术语与新增语言步骤。原生 WebView 未验收项继续保留在第 11 项。

## 第 1 项实现范围

- 中英文词典按 9 个域拆分，当前只提供基础 key；后续迁移页面时扩充，不宣称页面已双语。英文域对象使用源字典 Messages 类型约束，离线测试同时检查完整键结构。
- LocaleState 使用 Svelte rune 持有窗口状态，支持保存指定语言和恢复系统选择，同步 document.lang；存储读取/保存/移除失败分别保留可读状态与本地化说明，保存失败仍保留当前窗口语言。第 2 项负责把状态接到设置页面。
- 外部 locale-bootstrap.js 在 hydration 前设置首帧语言；加载提示使用按 lang 显示的静态中英文文本，不添加内联脚本，不修改 CSP。bootstrap 与应用解析规则的一致性由离线矩阵测试覆盖。
- Intl 辅助函数均显式接收 locale，文件大小可选 1000/1024，保留 KB/KiB 区别；日期允许传 timeZone，复数按 Intl.PluralRules 选择。
- 11 项离线测试通过，覆盖语言优先级、无效偏好、系统回退、bootstrap 一致性、存储失败/恢复、字典结构和格式化；Svelte 检查（0 错误/0 警告）与 production build 通过，CI 已加入这两组测试。真实 WebView 首帧与持久化尚待第 2 项后验收。未启动/重启 dev 或访问真实 Runtime/账户/游戏。

## 第 2 项实现范围

- 设置新增“通用”并作为默认分类；保留已有 section 链接。语言选择包含跟随系统、简体中文和 English，语言名称带 lang。切换通过窗口 LocaleState 更新，不重载、不清空搜索或重置分类。
- 设置页标题、分类、搜索、空态及尚未开放说明已进入双语字典；中英文关键词均可搜索，换语言后仍保留搜索内容。外观/账户/关于子页自身文案继续留待后续步骤迁移。
- 存储读取失败和保存失败显示本地化 role=status 提示；保存失败仍即时更新当前窗口，恢复系统时移除持久偏好。
- 验证：Svelte 0 错误/0 警告、production build 与 11 项基础层测试通过；新增 opt-in settings-language.browser.mjs，使用本地构建、假 Runtime 和无头 Edge 验证即时切换、刷新持久化、恢复系统、双语搜索、section 链接不重置选择和 localStorage 被禁用的提示。查看 900×680 英文截图，布局无溢出；原生 Tauri、其他平台与后续页面文案仍待验收。

## 第 3 项实现范围

- 主导航、标题栏后退/前进/侧栏切换、主内容与拖动区域的无障碍名称、App 连接/准备/失败/超时/日志提示、更新按钮、外观模式菜单、全局任务面板标题/触发器/空态和 ComingSoon 已使用窗口字典。
- 启动与外观保存错误保存状态标识，渲染时翻译；更新/任务/侧栏等完整句子通过字典函数插值。所有已有页面 title 模板迁移，用户实例名保留原文。
- 新增 App 错误页：404 与其他失败显示双语主文案，提供重试/回首页，原始错误仅作为可展开的技术详情。该页不替代 Shell 原生恢复页（第 9 项）。
- 当前迁移的壳层组件已无硬编码中文；账户子菜单、业务进度子组件、具体页面空态和动作继续留在第 4–7 项，不宣称这些区域已双语。
- 验证：11 项语言/状态测试、Svelte 0 错误/0 警告、production build 通过。隔离 Edge 脚本增加英文导航/后退、任务空态、服务端占位及 title、404 和 Runtime 启动失败验收，全部通过；查看 900×680 英文设置与失败页截图。仅本地 fixture，无真实 Runtime/账户/下载，不替代原生 WebView 或其他平台测试。

## 第 4 项实现范围

- 首页玩家卡、快捷游玩、全部实例、内置 Anvil 动态及官方新闻的界面文案已迁移；内置编辑内容提供中英文，官方文章标题、用户名称、版本和文件路径保留原文。游玩时长、相对时间和次数显式接收 locale。
- 实例列表/卡片、三步安装向导与 Fabric 选择器、安装/Java 进度、导入提示、启动控件和删除确认已迁移。下载大小/速度仍使用十进制 KB/MB 与原精度，Intl 只处理数字；不把此处改为 KiB。
- 安装/启动/删除的本地错误与删除回执按当前语言渲染，不因切换语言重复提交。启动按钮短标签不再依赖中文正则删字；未知 Runtime 错误在技术详情保留，稳定 code 的完整映射继续属于第 8 项。recent-plays 仅存用户名称/时间，没有产品文案，不改变其格式或行为。
- 78 项 Node 回归通过（全部 .test.ts，加 locale、导入/导出和客户端流程状态测试）；Svelte 0 错误/0 警告与 production build 通过。新增状态测试覆盖原有失败/回执切换语言、安装任务不重复请求与原始技术详情保留。
- 隔离 Edge 验收英文首页、官方原文保留、空实例库、Fabric 安装三步流程、中文实例名/搜索及空结果。查看 900×680 首页和确认页截图，调整首页数量间距；未点击开始安装、未删除实例、未下载或启动游戏，不代替真实 Runtime/Tauri 和跨平台验收。

## 第 5 项实现范围

- 实例侧栏、概览、设置、模组列表、本地资源包/光影管理、日志与崩溃报告、整合包导出选项和任务提示已迁移。整合包导入状态已在前序步骤迁移；在线发现及 Iris 共用版本/依赖确认组件留在第 6 项统一处理。
- 本地 ZIP 校验暴露稳定 issue，渲染时选择语言；保留文件大小、路径和安全限制。资源文件沿用二进制 KiB/MiB，网络进度沿用十进制 KB/MB；计数、日期与内存数字按 locale 格式化。
- 已有失败、未知结果与任务阶段切换语言时重新渲染，不重复提交；原始 Runtime 原因只放技术详情。清除错误时同步清除旧详情，已取消的读取不发布错误。光影环境按现有稳定 status 给出本地化说明，详细 reason 保留原文，后续仍需第 8 项补完整 code。
- 80 项 Node 回归通过，包含 ZIP 校验语义不变、环境标签、导出阶段及保留错误切换语言；Svelte 0 错误/0 警告与 production build 通过。隔离 Edge 覆盖英文实例侧栏、设置、ZIP 校验、光影空态、原始游戏日志、崩溃报告空态和导出选项；查看 900×680 导出与光影截图。
- 浏览器只读取假 API、选择测试 ZIP 和打开导出选项，未保存真实设置、导入/导出文件或启动游戏；不替代真实 Runtime/Tauri 及跨平台验收。

## 第 6 项实现范围

- 模组发现侧栏、实例/来源选择、搜索与滚动结果、摘要、兼容版本和依赖计划确认已迁移；在线光影浏览、下载确认及实例页共用的 Iris 安装确认同步迁移。项目名、作者、第三方摘要、版本名和文件名保留原文。
- Modrinth/CurseForge 的 DependencyPreview 新增 warningDetails，包含 snake_case code 与旧原始说明；原 warnings 字段不改，HTTP API 版本不变。已知说明按 code 翻译/折叠，旧 Runtime 或未知 code 显示通用可见提醒并保留技术详情，不按中文句子或前缀隐藏警告。
- 窗口资源任务的阶段、提交拒绝、未知结果、取消失败和错误详情可随语言渲染。计划过期、单任务准入、原计划重试与离页下载规则保持不变；网络大小继续十进制，任务面板原有 MiB 保持二进制。下载量按 locale compact 格式，项目/版本日期按 locale 显示同一 UTC 日。
- 82 项 Node 回归通过，覆盖警告码、旧 Runtime/未知警告、UTC 日期与英文下载量，以及已保留资源错误/阶段切换语言不重复提交。CI 纳入全部 .test.ts 和实际 Svelte 状态测试；Svelte 0 错误/0 警告、production build、CLI 编译、Core/CLI Clippy 和 rustfmt 检查通过。
- Core 离线测试串行通过 158 项、默认跳过 4 项真实网络/JAR smoke；新增双来源预览字段兼容性测试另行通过。完整串行运行需要允许临时 loopback fixture；首次并行运行有一个已有 shader fixture 锁冲突，未把它计作通过。
- 隔离 Edge 验收英文模组计划、可选依赖/文件数量、未知警告可见、CurseForge 未配置、在线光影下载确认和实例页 Iris 确认。展开详情不重复请求计划，所有路径均未提交安装；查看 900×680 的计划截图。原生 Tauri、真实下载与游戏加载仍待手测，旧综合浏览器脚本的全量双语迁移继续属于第 11 项。

## 第 7 项实现范围

- 外观设置、预设主题名称、自定义颜色编辑/校验、账户菜单/切换子菜单、登录弹窗、开发态离线入口、账户设置和游玩记录清除确认均已迁移。主题 ID、保存的数据与颜色值不变，名称按当前 locale 显示。
- 登录阶段和本地账户错误按当前语言显示；已保留的失败不因切换语言再次发起登录、读取账户或清除记录。账户错误对象保留动态 message，重试前仍先读取确认状态；原始错误保留在可展开详情。该步骤尚沿用旧英文错误分类；第 8 项已用稳定错误码替换，不再依赖这些句子。
- 关于页版本、日志状态/目录、更新检查错误、新版 Shell 与兼容性重装说明已迁移；既有更新导航按钮已在第 3 项迁移。用户账户名、验证码、版本号和日志目录保持原样。登录弹窗动作区允许换行，避免开发态离线入口与较长英文动作挤占宽度。
- 86 项 Node 回归通过，新增账户已保留错误/验证码过期的语言切换、离线回调不发起登录、未知清除结果防止重发、双语主题名称和 Shell 重装说明；Svelte 0 错误/0 警告、production build 和 diff 检查通过。本项仅改前端，没有新增 Rust 改动或真实账户联调。
- 隔离 Edge 覆盖英文外观/无效色值、账户菜单及切换子菜单/当前账户标记、登录验证码/等待/取消/失败详情、账户与全部记录清除确认、版本/日志和兼容性更新提示。验证码与账户仅来自本地假服务，原生 Shell API 为替身且不打开真实网站；清除仅打开确认框再取消，未提交 DELETE。查看 900×680 外观、登录、菜单、清除确认与关于页截图；原生 WebView、开发态离线按钮实机布局及其他平台仍待验收。

## 第 8 项实现范围

- ApiError 构造器强制提供 code，现有各构造点按明确分支/类型赋码，响应保留旧 error 与 HTTP 状态。Core 失败沿用 Error::code()；Runtime 补计划过期、未配置来源、上传中断/不完整、实例忙等场景码。请求超时也返回带码 JSON；框架自动拒绝等无 code 响应使用前端兼容回退，不要求其诊断文本符合词典。
- 安装/整合包导入、资源安装、整合包导出和账户登录 DTO 新增可空 errorCode，启动 DTO 沿用既有 errorCode。登录授权、保存与清理在 Runtime 内保留类型化失败；清理失败有独立 account_cleanup_failed，前端不再匹配 expired/cleanup 等英文片段猜测语义。旧 Runtime 取消后的非空详情仍可见，不凭原文宣称清理成功或失败。
- 光影环境保留 status/reason，新增 reasonCode 与有界字符串参数 reasonParams，覆盖未安装/禁用 Iris、扫描预算、元数据/副本、ID/别名冲突、缺少依赖、版本/约束与双向不兼容。UI 按码插入模组 ID/文件名；旧 Runtime 按 status 回退，未知原因给出通用未确认说明，原文保留在技术详情。
- 缓存刷新失败新增 refreshErrorCode；前端 RuntimeRequestError 暴露经格式校验的 code，独立运行的 runtime-errors 帮助函数统一映射。未知合法码不显示为产品句子，而使用通用失败；缺少或格式无效的码保留已有上下文提示。结果未知时的只读核对、同计划重试、清除准入和后台任务生命周期保持不变。
- HTTP API 仍为 1.0.0，既有字段不删除、不改名，CLI 原始输出/日志不翻译。此处完成错误传输与页面映射，不代替第 11 项完整中文扫描和双语页面验收。
- 90 项 Node 测试通过；新增真实 transport 编译后的假 fetch 测试，覆盖已知/未知/缺少/格式无效 code、HTTP 状态与原文保留，另覆盖光影参数、保留错误切换语言、旧 Runtime 取消详情与未知结果不重发。Svelte 0 错误/0 警告、production build、Core/CLI Clippy、rustfmt 与 diff 检查通过。
- Rust 串行离线回归：Core 159 项通过/4 项 ignored；Runtime 单测 33 项通过/2 项 ignored，CLI 20 项与健康/鉴权进程测试 1 项通过。新增 HTTP 错误字段兼容、Core 类型码独立于措辞、登录/清理及光影原因参数断言；健康/鉴权进程测试显式使用临时数据目录，仅请求 loopback /health。所有真实网络/JAR smoke 默认跳过。
- 隔离 Edge 验收已知计划过期、未知码、旧 Runtime 无码，以及原始技术详情保留，所有场景均未提交安装。英文账户失败按 authentication_failed 显示，不再从 fixture 中的 expired 猜测；原始说明仍可展开。查看 900×680 错误页截图。没有连接真实账户、下载游戏或操作维护者 dev 进程，原生 WebView/其他平台仍待实测。

## 第 9 项实现范围

- Shell 恢复入口具备静态中英文正文、重试和提示，分别标注 lang；外部 classic 脚本在正文前按系统语言设置 document.lang、标题与显示语言。匹配规则为有序 navigator.languages > 列表为空时 navigator.language > zh-CN，不读取 Desktop 偏好，脚本不可用时两种语言均可阅读。
- 重试从内联 onclick 迁到外部脚本，只响应显式点击，不自动轮询。核对锁定的 Tauri 源码与当前自定义协议处理链：自定义响应不经过内置 asset 的 CSP 注入，因此恢复页声明自身 meta CSP，维持 script-src 'self'，不放开 unsafe-inline 脚本；原 Tauri CSP 配置不改。
- 协议新增唯一保留的 /__anvil_shell__/recovery.js 内嵌资源，GET/HEAD 与 authority 校验先执行；不依赖活动 App，不被 App 包同名文件覆盖，不开放任意 bootstrap 路径。缺包、损坏、入口缺失、Shell API 不兼容和版本身份不匹配仍进入同一恢复提示，不从原始错误猜测具体原因，不改变激活/回滚与 Runtime 行为。
- 3 项 Node 脚本测试、8 项 Rust App 协议测试、Shell 全目标 Clippy、rustfmt 和 diff 检查通过。兼容性 fixture 已使用有效包版本加 >=99.0.0 的 Shell API 范围，与单独的包版本身份不匹配场景区分；各故障均验证双语页面与外部脚本引用。
- 隔离 Edge 覆盖英文、中文、有序系统回退、空列表时 navigator.language、存储不可用、唯一页面标题、实际 CSP 下点击重载且无违规，以及禁用 JS 的静态双语正文。查看 900×620 英文与 620×420 双语截图，无横向溢出；浏览器只读取内置静态文件，Rust 只操作临时包，未启动用户 dev/Tauri/Runtime。真实原生 WebView 和其他平台仍待实测。

## 第 10 项实现范围

- 补齐覆盖式滚动条的双语无障碍名称，首帧加载文字显式标注 lang；核对输入、按钮、链接、标题、语言选项和 sr-only 文本。日期/时间/数字格式化均显式使用窗口 locale，纯显示帮助函数统一引用 defaultLocale；验证器诊断走默认词典或技术详情，不在产品正文放硬编码中文。
- 修正英文 1 / 1 file 的单数，保留 0 和复数形式，并补数量分组回归。下载继续使用十进制 KB/MB，本地资源和任务保留原二进制 KiB/MiB 及精度；版本号、文件名、第三方日期原文和 CSS/ARIA 数字不当作文案改写。
- 页面原有 body 最小宽度 760px 与 Shell 的 620px 窗口下限不一致，已对齐为 620px；实例概览资源卡按内容宽度切换单列/三列，使最小窗口下中英文标题均完整显示，宽窗口保留三列。没有改变颜色、安装提交或导航行为。
- settings-language.browser.mjs 增加 zh-CN/en × 620×420/1280×800 矩阵：14 个页面与 4 个安装弹窗，共 60 个页面/弹窗组合，覆盖首页、实例库、设置分类、实例概览/模组/资源包/光影/设置/日志及在线模组/光影计划。检查横向溢出、可见控件名称、按钮裁切、资源卡标题；另确认安装弹窗 Escape 返回入口焦点、在线确认按钮留在视口和滚动条名称随语言显示。测试包含长实例名，均使用假 API，不保存设置、不提交安装。
- 91 项 Node 回归、Svelte 0 错误/0 警告、production build、矩阵浏览器与 diff 检查通过。查看英文 620px 概览/实例设置/在线计划与中文本地光影截图；仍不能替代原生 WebView、其他平台或最终全部文案守卫，第 11 项继续处理这些验收边界。本项没有 Rust 改动。

## 第 11 项自动化范围

文案守卫通过 npm run desktop:i18n-check 执行：递归扫描 Desktop src 文本源码，跳过词典目录；其他位置出现汉字则给出文件/行/列。app.html 仅允许与 common.loading 严格一致且各出现一次的双语启动 span；新加的中文、文案漂移或重复启动文案仍失败。当前没有其他允许列表。中英文词典递归检查键、值类型、数组长度及插值函数参数数量；四项测试覆盖正例与拒绝边界。

综合浏览器脚本通过 ANVIL_TEST_LOCALE 选择 zh-CN/en，保存独立截图，所有真实外网请求均拦截。两种语言各通过完整流程：Runtime 启动门禁/失败重试、合成头像、发现页滚动与历史、假安装离页接续、未知提交原 ID 重试、当前首页布局/假启动历史、空态和删除确认/冲突/丢失响应只读确认。fixture 的原始诊断保持原文；展开技术详情后再检查，不作为主文案。另复跑 60 组语言/布局矩阵，通过；查看英文 900px 首页截图，布局正常。

本项验证为本机无头 Edge 与隔离替身，未操作真实账户、实例或用户 dev 进程。95 项 Desktop Node、3 项 Shell Node、Svelte 0 错误/0 警告、production build 和 diff 检查通过。没有 Rust 产品代码改动，未重跑 Rust 测试；跨平台原生结果按下表独立保留。

| 平台 | 原生 WebView | 记录 |
| --- | --- | --- |
| macOS | 待手测 | 首帧、切换、关闭重启保留、系统语言回退、620px 英文布局 |
| Windows | 待手测 | 同上；本机 Edge 不作为 Windows WebView2 通过证据 |

## 验证边界

- 静态检查、类型检查和离线浏览器验收不等于英文文案完整或布局通过；守卫脚本、双语页面检查和真实 WebView 手测都完成后才宣称桌面界面已国际化。
- 不启动真实 Minecraft、不访问真实账户服务、不下载游戏或模组来验证文案；Runtime 错误码用隔离 fixture 和临时实例覆盖。
- 英文文案需要人工审校；机器翻译结果只能作为初稿，不能直接作为交付文案。
- 未实际运行的检查项不得写成通过；平台相关结果分别记录。
