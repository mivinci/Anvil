# 模组发现页离线检查

仓库根目录运行 Node 状态检查（PowerShell）：

```powershell
node --experimental-strip-types --test (Get-ChildItem -LiteralPath apps/desktop/tests -Filter 'mod-*.test.ts').FullName
npm run check --workspace @anvil/desktop
```

浏览器验收使用构建后的页面、本地 HTTP fixture、独立临时 Edge 配置和假 Shell IPC。不会读取真实账户、连接真实 Runtime 或下载/安装模组。先构建，再运行：

```powershell
npm run build --workspace @anvil/desktop
# 如果 Playwright 不在普通 Node 模块路径，指定已经安装的包目录。
$env:ANVIL_PLAYWRIGHT_MODULE = '<Playwright 包目录>'
node apps/desktop/tests/mod-discovery.browser.mjs
```

需要本机 Edge 和可用的 Playwright，测试不自动安装依赖。默认 CI 无需运行此脚本。成功后输出检查结果及截图目录；截图位于被 Git 忽略的 `apps/desktop/.svelte-kit/mod-discovery-qa/`。

覆盖搜索、滚动追加/位置恢复、直接链接及历史、目标/来源切换、缺失/原版目标、来源未配置、900px 深色与宽窗口浅色，以及内存安装任务离页接续、提交结果未知后原计划重试和返回实例刷新。另用长说明和 20 个兼容版本验证底部安装栏始终可见、详情默认收起及正常状态隐藏重新生成入口。原生 WebView、真实平台下载及游戏内加载仍需独立验收。

账户头像验收使用本地账户资料和合成 PNG，拦截官方纹理 URL，不访问真实皮肤服务；覆盖头部/饰层显示、菜单与导航共用元数据请求，以及图片失败回退。Core 的 account_skin 测试验证公开资料解析、来源/身份校验和临时目录缓存恢复。

启动验收覆盖禁用 JavaScript 时的静态透明提示、Runtime 尚未就绪时阻止业务请求、health 就绪后进入主页面，以及失败提示/显式重试。假 Shell 包含 isTauri 和窗口元数据，以进入实际桌面分支；原生窗口首帧与玻璃背板仍需桌面实测。

首页验收共用该离线脚本的本地 fixture，覆盖模拟启动成功/失败与本机历史、宽窄布局、浅深色、空库/读取失败；不启动真实 Minecraft。最近游玩存储可单独执行 `node --experimental-strip-types --test apps/desktop/tests/recent-plays.test.ts`。

删除实例验收使用内存 fixture：取消不提交、冲突保留、确认删除后列表/游玩历史更新、响应丢失只读确认；不会删除真实实例。Core `client::removal::tests` 仅创建并删除临时实例，Windows 用临时 junction 验证链接拒绝和外部目标保留。

覆盖式滚动条验收包括显示/隐藏均零占位、横纵拖动、键盘 End、滑块滚轮转发和弹出列表拖动不关闭；首页同时检查“全部实例”属于快捷入口卡片。
