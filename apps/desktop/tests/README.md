# 模组发现页离线检查

仓库根目录运行 Node 状态检查（PowerShell）：

```powershell
node --experimental-strip-types --test (Get-ChildItem -LiteralPath apps/desktop/tests -Filter '*.test.ts').FullName
npm run check --workspace @anvil/desktop
```

`play-stats.test.ts` 覆盖玩家卡的时长、相对时间、启动次数和“最常玩”展示辅助，只使用内存数据，不访问 Runtime。

浏览器验收使用构建后的页面、本地 HTTP fixture、独立临时 Edge 配置和假 Shell IPC。不会读取真实账户、连接真实 Runtime 或下载/安装模组。先构建，再运行：

```powershell
npm run build --workspace @anvil/desktop
# 如果 Playwright 不在普通 Node 模块路径，指定已经安装的包目录。
$env:ANVIL_PLAYWRIGHT_MODULE = '<Playwright 包目录>'
node --experimental-strip-types apps/desktop/tests/mod-discovery.browser.mjs
$env:ANVIL_TEST_LOCALE = 'en'
node --experimental-strip-types apps/desktop/tests/mod-discovery.browser.mjs
```

需要本机 Edge 和可用的 Playwright，测试不自动安装依赖。默认 CI 无需运行此脚本。成功后输出检查结果及截图目录；截图位于被 Git 忽略的 `apps/desktop/.svelte-kit/mod-discovery-qa/`。

脚本通过 `ANVIL_TEST_LOCALE` 选择 zh-CN（默认）或 en，选择器读取实际词典，截图按语言分目录保留。首页断言使用当前玩家卡/新闻首行与下方实例库结构；启动历史、空库与读取失败继续验证。所有外网请求均拦截，皮肤由合成 PNG 返回。

覆盖搜索、滚动追加/位置恢复、直接链接及历史、目标/来源切换、缺失/原版目标、来源未配置、900px 深色与宽窗口浅色，以及内存安装任务离页接续、提交结果未知后原计划重试和返回实例刷新。另用长说明和 20 个兼容版本验证底部安装栏始终可见、详情默认收起及正常状态隐藏重新生成入口。原生 WebView、真实平台下载及游戏内加载仍需独立验收。

账户头像验收使用本地账户资料和合成 PNG，拦截官方纹理 URL，不访问真实皮肤服务；覆盖头部/饰层显示、菜单与导航共用元数据请求，以及图片失败回退。Core 的 account_skin 测试验证公开资料解析、来源/身份校验和临时目录缓存恢复。

启动验收覆盖禁用 JavaScript 时的静态透明提示、Runtime 尚未就绪时阻止业务请求、health 就绪后进入主页面，以及失败提示/显式重试。假 Shell 包含 isTauri 和窗口元数据，以进入实际桌面分支；原生窗口首帧与玻璃背板仍需桌面实测。

首页验收共用该离线脚本的本地 fixture，覆盖模拟启动成功/失败与本机历史、宽窄布局、浅深色、空库/读取失败；不启动真实 Minecraft。最近游玩存储可单独执行 `node --experimental-strip-types --test apps/desktop/tests/recent-plays.test.ts`。

删除实例验收使用内存 fixture：取消不提交、冲突保留、确认删除后列表/游玩历史更新、响应丢失只读确认；不会删除真实实例。Core `client::removal::tests` 仅创建并删除临时实例，Windows 用临时 junction 验证链接拒绝和外部目标保留。

覆盖式滚动条验收包括显示/隐藏均零占位、横纵拖动、键盘 End、滑块滚轮转发和弹出列表拖动不关闭；首页同时检查玩家卡内的实例库入口与下方实例库管理链接。

## 国际化基础层

文案维护、术语与新增语言步骤见[国际化维护](../../../docs/i18n.md)。

```sh
node --experimental-strip-types --test apps/desktop/tests/i18n.test.ts apps/desktop/tests/locale-state.test.mjs
```

11 项离线测试涵盖语言回退、首帧脚本与应用规则一致性、词典键和格式化，以及实际 Svelte rune 编译后的语言状态、存储失败/恢复。仅使用 VM/内存存储与临时编译目录，不启动 Runtime 或浏览器；真实 WebView 首帧、语言设置和页面翻译仍需后续验收。

语言设置的 opt-in 浏览器检查（先构建；通过 ANVIL_PLAYWRIGHT_MODULE 指定已有 Playwright 包，默认使用 Edge，可用 ANVIL_BROWSER_CHANNEL 调整）：

```sh
npm run build --workspace @anvil/desktop
node apps/desktop/tests/settings-language.browser.mjs
```

只启动临时 loopback fixture 和无头浏览器，阻止外网请求，不连接真实 Runtime 或账户。覆盖语言选择、刷新持久化、系统默认、双语搜索、section 链接和存储被禁用的提示；截图写入已忽略的 .svelte-kit/settings-language-qa。900×680 的本机 Edge 检查不能替代原生 Tauri 与跨平台验收。

同一浏览器脚本还覆盖公共壳层的英文导航、后退、任务空态、ComingSoon、页面标题、404 错误页与 Runtime 连接失败提示；业务进度与账户子菜单覆盖见下文；原生平台结果仍独立记录。

首页、安装与启动迁移的回归检查：

```sh
node --experimental-strip-types --test apps/desktop/tests/*.test.ts apps/desktop/tests/locale-state.test.mjs apps/desktop/tests/modpack-import.test.mjs apps/desktop/tests/modpack-export.test.mjs apps/desktop/tests/client-flow-state.test.mjs apps/desktop/tests/runtime-request.test.mjs apps/desktop/tests/i18n-guard.test.mjs
```

当前共 95 项，新增客户端流程测试使用编译后的真实 Svelte 状态与假 API，验证阶段/失败/删除回执的语言切换及技术详情保留。compile-locale.mjs 是这些测试的编译辅助，不运行真实 Runtime。settings-language.browser.mjs 另覆盖英文首页、官方原文保留、Fabric 安装三步选择和中文实例搜索；只选配置，不提交安装或删除。

实例管理迁移后，同一语言浏览器脚本覆盖英文详情侧栏、实例设置、本地 ZIP 校验、光影环境和空态、日志原文、崩溃报告空态与整合包导出选项；不提交设置、导入或导出。导出状态测试覆盖语言切换后的阶段及错误详情，shader-packs 测试覆盖稳定 ZIP issue、双语校验和原二进制大小语义。

在线资源迁移后，语言浏览器脚本还覆盖英文模组计划（稳定警告码、未知警告、可选依赖和文件复数）、CurseForge 未配置、在线光影确认和 Iris 共用安装确认。只生成本地假计划，不提交安装；展开详情的请求计数保持不变。Node 检查覆盖旧 Runtime/未知警告、英文项目统计与资源任务已有错误切换语言不重提。CI 运行全部 .test.ts 以及编译后的 Svelte 状态测试。

Rust 改动验证使用 `cargo test -p anvil-core --lib --offline -- --test-threads=1`；需要临时 loopback fixture 监听权限，不请求真实上游。警告测试验证两个来源都保留 warnings 并新增 warningDetails，HTTP API 版本不变。首次并行全库运行出现已有 shader fixture 锁冲突，串行完整运行通过；真实网络/JAR 用例默认 ignored。

设置与账户迁移后，同一浏览器脚本增加英文外观预设/颜色校验、假登录验证码/取消/失败详情、账户切换子菜单、账户与全量游玩记录清除确认、关于页版本/日志和 Shell 兼容性更新提示。清除只打开并取消确认，断言无 DELETE；登录与 Shell 只使用替身，不打开真实授权网站。900×680 截图仍不能替代原生 WebView 与开发态离线入口实机验收。

client-flow-state.test.mjs 还覆盖账户已保留错误与过期登录切换语言不重发、离线入口回调、清除结果未知时阻止重复请求及读回恢复；appearance 与 shell-compatibility 的 .test.ts 覆盖主题名称与重装说明双语。原始账户/Runtime 错误保留在详情，Runtime 稳定错误码已在第 8 项迁移，缺码时继续使用上下文回退。

Runtime 错误契约回归新增 runtime-errors.test.ts 与 runtime-request.test.mjs。后者编译实际 runtime.ts，以假 fetch 验证 code/旧 error/HTTP 状态，不监听端口；helpers 和实际 Svelte 状态覆盖未知/缺码回退、取消清理详情与结果未知不重复提交。语言浏览器脚本补已知 plan_expired、未知码、旧 Runtime 无码及原文详情，均不提交安装。

Rust 检查：`cargo test -p anvil --offline -- --test-threads=1` 与 `cargo test -p anvil-core --lib --offline -- --test-threads=1`。需要临时 loopback fixture 权限；新 support/runtime_errors.rs 和 account_errors.rs 验证应用错误增量字段、类型码与登录清理，shader 测试覆盖 reasonCode/reasonParams。runtime_server.rs 的健康/鉴权子进程显式使用临时数据目录，不读取用户账户或启动真实游戏。CLI 及日志保持原文；HTTP API 版本不变。框架拒绝等缺码响应仍走兼容回退。

Shell 恢复页另有 `node --test apps/shell/tests/recovery.test.mjs`（3 项）与 `cargo test -p anvil-shell --test app_protocol --offline`（8 项）。浏览器 opt-in 为 `node apps/shell/tests/recovery.browser.mjs`，共用已有 ANVIL_PLAYWRIGHT_MODULE/Edge；覆盖系统语言及回退、存储不可用、CSP 下显式重试、唯一标题、禁用 JS 的双语正文，截图写入忽略的 target/shell-recovery-qa。协议用临时包验证缺失/损坏/真实 Shell API 不兼容及保留脚本，浏览器用内置文件，不操作真实 App；原生 WebView 尚待验证。详见 [Shell README](../../shell/README.md)。

格式化与布局回归：Node 增加单文件/复数与分组计数；同一语言浏览器脚本扩展到 zh-CN/en × 620×420/1280×800，共 60 个页面/安装弹窗组合。检查页面横向溢出、可见控件名称、按钮与资源卡标题裁切，确认安装弹窗键盘关闭后的入口焦点、在线安装确认按钮可见，以及窄窗滚动条双语名称。覆盖 14 个页面，使用长实例名和假 API，不提交安装或设置；截图位于原 settings-language-qa 的 layout-* 文件。容器内日志允许保留自身滚动，不把用户/第三方原文截断作为翻译错误；此检查不是完整 WCAG 审计或原生 WebView 验收。

## 文案守卫与最终验收

```sh
npm run desktop:i18n-check
node --test apps/desktop/tests/i18n-guard.test.mjs
```

守卫递归扫描 Desktop src 的 TS/JS/Svelte/HTML/CSS/SVG/JSON，词典目录之外发现汉字即失败并报告位置；同时比较中英文词典的嵌套键、类型、数组长度和函数插值参数数量。启动 HTML 的双语 span 必须各出现一次且与 common.loading 完全一致，仅屏蔽这两段已验证文字，不豁免整个文件。当前不需要其他允许列表。CI 在 Svelte 检查前运行守卫，并运行其四项拒绝边界测试。

浏览器结果仅代表本机无头 Edge + 本地 fixture。原生 Tauri WebView 仍需在 macOS/Windows 分别记录：首次进入时语言/闪烁、设置即时切换与页面状态、关闭重启后的语言保留、系统语言回退，以及 620px 英文布局。未执行的平台保持待验收，不将本地浏览器结果登记为原生通过。
