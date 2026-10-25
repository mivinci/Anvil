# 桌面国际化

状态：范围与决策已确认，尚未开始实现；等光影页开发合入、工作区回到干净基线后再开始第 1 项。支持简体中文（zh-CN，源语言）和英文（en）；手动选择优先，默认跟随系统语言，无法匹配时回退 zh-CN。Runtime 返回给桌面 UI 的可见文案纳入本轮。光影页及其专用代码仍在开发，本轮不迁移，待 [实例光影管理](client-shaders.md) 第 4 项合入后补做。逐项推进，每项结束汇报并确认下一项。

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
- Anvil 的 Runtime 目前直接返回中文句子，需要补稳定 code 和参数，UI 按 code 翻译；Q2Q 由服务端返回错误 key，思路相同。

## 执行项

0. [x] **范围与决策**：zh-CN + en；已保存偏好 > 系统语言 > zh-CN；Runtime 可见文案纳入；CLI 不纳入；光影页暂缓。回退语言集中在单一常量，后续调整不扩散。
1. [ ] **词典与 locale 基础层**：新增 `apps/desktop/src/lib/i18n/`，包含 `index.ts`（`locales`、`localeNames`、`isLocale`、`resolveLocale`、`messages`）、`zh-CN.ts`（源语言与 `Messages`）、`en.ts`（`: Messages`），按 `common`/`nav`/`home`/`clients`/`instance`/`mods`/`settings`/`account`/`errors` 拆分子文件。新增 `locale.svelte.ts`：`$state` 单例、`setLocale`、`localStorage` 键 `anvil.locale`、同步 `document.documentElement.lang`；读取或写入失败要有明确降级，不能吞掉。`resolveLocale` 支持 `zh`/`zh-Hans`/`zh-CN`/`en`/`en-US`，`navigator.languages` 为空时回退 `navigator.language`。首帧通过 `static/` 外部脚本设置 `lang` 与启动提示，CSP 不放开 `unsafe-inline`。新增 `formatBytes`/`formatCount`/`formatDate`/`formatDateTime`/`plural`，全部接收 locale；保留现有十进制 KB 与二进制 KiB 的语义差异，只本地化数字格式，不借机改单位。新增 `apps/desktop/tests/i18n.test.ts` 覆盖回退、格式化和键一致性。
2. [ ] **设置入口与语言切换**：在 `/settings` 新增“通用”分类（语言与后续偏好），或与维护者确认放在“外观”下；语言选择使用下拉/单选，语言名带 `lang`，切换即时生效、不重载、不丢失当前页面状态。设置搜索关键词与占位说明同步进字典。`localStorage` 不可用时提示“本次选择仅在当前窗口生效”。
3. [ ] **公共壳层与导航**：导航、标题栏、启动状态、更新按钮、错误边界、通用动作（取消/保存/重试/刷新/搜索/关闭/导入/移除）、空态、ComingSoon，以及所有页面 `<title>` 模板。壳层完成后应无字典外的中文。
4. [ ] **首页、实例库、安装与启动**：`/`、`/clients`、实例卡片、安装弹窗与进度、Java 进度、启动控制、最近游玩、新闻、删除实例。把 `client-install-state`、`client-library-state`、`client-launch-state`、`client-removal-state`、`recent-plays` 等状态模块的文案迁出。时间、文件大小、下载速度按 locale 格式化。
5. [ ] **实例详情、设置、日志、资源包**：概览计数、实例设置、日志与崩溃报告、资源包页面。资源包页面可以迁移可见文案；`local-pack.ts` 的校验错误本轮不改，因为它与光影页共用且仍在开发，英文界面暂时会看到中文错误，记入暂缓项统一处理。光影页不在本项内。
6. [ ] **模组发现与安装**：`/mods`、搜索与筛选、详情、兼容版本、计划确认、窗口级任务进度、项目统计与更新时间。`mod-plan-warnings.ts` 依赖中文原文折叠例行提示，必须改为 Runtime 返回的 code（依赖第 8a 项），否则改一个字就会失效。
7. [ ] **设置、账户、关于、更新**：`/settings` 分类与搜索、外观设置、关于页、账户菜单/登录弹窗/离线测试入口、更新按钮。账户错误与状态全部可双语。
8. [ ] **Runtime 可见文案结构化**：分两步。
   - 8a（页面迁移前）：把安装计划的 `warnings: Vec<String>` 改为稳定的 warning code + message（`modrinth.rs`、`curseforge.rs` 及 Runtime DTO）；启动、资源任务等失败原因补 code。光影环境的 `reason` 与 shader 相关字段随光影页暂缓。
   - 8b（收尾前，且光影改动合入后）：`ApiError` 的 JSON 增加稳定 `code`，为现有构造点逐一赋码；操作 DTO 增加 `errorCode`；前端 `RuntimeRequestError` 暴露 code，按字典翻译；未知 code 使用通用文案，原始 detail 放进可展开的“技术详情”，不直接作为英文主文案。字段只增不改，旧 Runtime 缺 code 时回退；不提高 `ANVIL_HTTP_API_VERSION`。`runtime.ts` 当前有光影未提交改动，8b 需等其合入，避免冲突。
9. [ ] **Shell 恢复页与启动提示**：`apps/shell/bootstrap/index.html` 按系统语言显示中/英文案并设置 `lang`；恢复页与 App 页面可能不同源，不能依赖 `anvil.locale`。先确认 Tauri CSP 对内联脚本和内联事件的实际效果，优先使用外部脚本或静态双语排版，不放开 CSP。覆盖包缺失、包损坏、版本不兼容三种恢复场景。
10. [ ] **格式化、复数、布局与无障碍**：替换硬编码的 `zh-CN`/`en-US` 调用；英文复数与中文量词；英文文案通常更长，检查固定高度按钮、168px 设置侧栏、卡片截断和窄窗口；`aria-label`、`title`、`placeholder`、`alt`、`sr-only` 全部进字典，语言名用 `lang` 标注。
11. [ ] **测试与验收**：更新 `apps/desktop/tests` 中依赖中文的断言（含 `mod-discovery.browser.mjs` 约 500 处汉字断言），浏览器验收脚本至少用英文跑一次布局 smoke。新增守卫脚本：扫描 `apps/desktop/src` 中 `lib/i18n/` 之外的 CJK，允许列表只包含测试 fixture、暂缓的光影文件与确属第三方内容的字符串；同时校验 zh-CN 与 en 的键完全一致，可挂 CI。按改动范围运行 `npm run desktop:check`、桌面构建、Node 单测、浏览器脚本、`cargo test`/`cargo fmt --all -- --check`/Clippy。真实 Tauri WebView 手测首帧、持久化、切换和英文布局；Windows 与 macOS 分别记录，未跑的不能写成通过。
12. [ ] **文档与状态**：README 增加“国际化”段落（对照 Q2Q platform README）；`.agent/architecture.md` 记录桌面语言契约；`apps/desktop/tests/README.md` 更新验收命令；本文件随进度更新，并补翻译术语表与新增语言步骤。

## 暂缓项：光影页

以下文件与文案随光影页开发一起暂缓，本轮不修改：

- `apps/desktop/src/routes/clients/[name]/shader-packs/+page.svelte`
- `apps/desktop/src/lib/client-shader-pack-state.svelte.ts`
- `apps/desktop/src/lib/shader-packs.ts`
- `apps/desktop/src/lib/local-pack.ts`（与资源包共用；本轮资源包页面只迁移自身文案）
- `apps/desktop/src/lib/runtime.ts` 的光影类型与调用
- `crates/anvil-core/src/resources/shader_environment.rs`、`shaders.rs` 及对应测试

待 [实例光影管理](client-shaders.md) 第 4 项合入后补做：页面与状态文案迁移、`shaderEnvironmentLabels` 改为 code 映射、环境摘要 `reason` 改为 `reasonCode` + 参数、`local-pack.ts` 改为返回错误 key 并同时更新资源包与光影消费者、光影测试与守卫允许列表。在此之前英文界面在光影页和本地资源校验错误上仍会显示中文，这是明确的暂缓边界，不当作已交付。

## 验证边界

- 静态检查、类型检查和离线浏览器验收不等于英文文案完整或布局通过；守卫脚本、双语页面检查和真实 WebView 手测都完成后才宣称桌面界面已国际化。
- 不启动真实 Minecraft、不访问真实账户服务、不下载游戏或模组来验证文案；Runtime 错误码用隔离 fixture 和临时实例覆盖。
- 英文文案需要人工审校；机器翻译结果只能作为初稿，不能直接作为交付文案。
- 未实际运行的检查项不得写成通过；平台相关结果分别记录。
