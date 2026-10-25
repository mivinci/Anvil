# Anvil

Anvil 是面向 Windows、macOS 和 Linux 的轻量 Minecraft Java 版命令行启动器与服务端管理工具。目前项目仍在开发中，桌面内测安装包见 [Releases](https://github.com/mivinci/Anvil/releases)。

## 现在可以做什么

- 查询 Mojang 官方游戏版本列表。
- 安装、查看和移除 Anvil 管理的 Java 运行时，不修改系统 Java 或 `PATH`。
- 创建、查看和前台运行原版 Minecraft 服务端；启动时可由你明确接受 EULA，并支持控制台停止和 Ctrl-C 优雅退出。
- 安装和列出原版客户端，使用 Microsoft 设备码登录，并在前台启动客户端。

CLI 已提供以上功能。桌面端使用 Tauri + SvelteKit + shadcn-svelte + Lucide，Runtime 已提供版本目录、客户端安装/列表和 Microsoft 设备码账户 API；首页提供紧凑的实例启动、快捷入口与本机最近游玩记录（确认运行后记录），并按需读取 Minecraft 官网 RSS 的最近八条动态；桌面 UI 正在重构，当前已接入窗口框架、账户菜单、登录弹窗及设置页（外观模式、自定义浅深色背景/文字/强调色、版本信息和项目入口），游戏/下载/账户设置仅有分类占位，客户端列表和安装版本选择面板已接入，安装提交、进度、暂停/继续、取消和窗口级下载任务已接入，安装完成自动刷新列表、失败/取消后显式重试及只读实例详情已接入，实例卡片启动、运行状态和 Runtime 进程管理已接入；安装 Minecraft 时会复用或补装所需托管 Java；启动时若 Java 缺失仍需显式确认补装；另有仅开发构建显示的 AnvilTester 离线测试入口，无需登录，真实启动待验收；实例日志页可按需查看 latest.log 与本次 Runtime 会话内保留的 Java 输出，也可浏览当前实例的崩溃报告。实例设置可独立保存最大/初始内存和游戏窗口尺寸，桌面与 CLI 启动共用；桌面实例设置另提供明确确认后的删除入口，移除当前实例数据并保留共享缓存。Minecraft 目录与 Fabric 兼容版本列表持久缓存 30 分钟，Modrinth 与 CurseForge 浏览目录持久缓存 5 分钟；重启按原获取时间复用，过期先显示旧数据再更新，支持手动刷新。桌面安装尚未在 WebView 中完成真实下载 smoke，Microsoft/Xbox 登录已进入真实联调，Minecraft Services 返回应用注册 403，等待 API 访问审核；完整登录与系统凭证库写入尚未验收；首个端到端目标是 Minecraft 1.21.1。服务端已在 Linux x86_64 上实测；Windows 和 macOS 的真实运行结果仍待维护者验证。

资源包本地管理正在分步接入：当前实现 Core ZIP 校验、共享存储、实例引用与目录事务恢复；Runtime 已提供列表、上传与移除 API，桌面资源包页已接本地 ZIP 确认导入、搜索、同步与移除，概览显示受管资源包数量；游戏内选择和排序仍由玩家操作，真实导入及游戏识别待验收。

光影页已接本地 ZIP 导入、搜索、同步与移除，以及受管 Iris/Sodium 环境提示，概览显示真实光影数量。缺少环境时也可先添加 ZIP；启用和参数设置仍在游戏中完成。已接 Iris 兼容版本选择、依赖计划确认和窗口级安装任务；无法确认的依赖环境拒绝提交。指定官方 Iris/Sodium JAR 的隔离缓存提交验收通过，在线下载全流程、游戏加载与渲染待验收。

首页已接入按账户的玩家卡：显示总游玩、近 7 天、启动次数、最常玩和上次游玩，并在运行中显示当前实例。统计由 Runtime 在游戏进入运行阶段后记录，保存在本机 Runtime 数据目录，不上传；900px 宽度下实例与动态并排、玩家卡与继续游玩在上，≥1200px 动态回到右侧栏。真实游玩记录、原生 WebView 首帧和不同窗口宽度的视觉验收仍待完成。

## 试用方式

桌面内测包按平台与 CPU 架构下载。macOS 打包配置使用免费的 ad-hoc 签名，尚未经过 Apple 公证，首次启动仍可能需要在“系统设置 → 隐私与安全性”中手动允许。该配置用于后续重新构建的版本，旧版 `desktop/v0.1.4-alpha.3` 的 DMG 存在签名问题，重新下载该版本不能修复。

从源码运行需要安装 Rust 1.90 或更新版本，以及 Node.js 22.17 或更新版本和 npm。

安装桌面端前端依赖并启动桌面开发模式：

```bash
npm install
npm run desktop:dev
```

根目录的 npm workspace 管理 `apps/desktop` 前端；Cargo workspace 管理 Rust crates 和 `apps/shell/src-tauri` 宿主。共享 TypeScript 编译选项位于根目录 `tsconfig.json`，前端项目通过 `extends` 继承。

桌面 Rust 日志在开发模式下同时写入终端与文件；macOS 路径为 `~/Library/Logs/Anvil`，分别保存 `shell.log` 和 `runtime.log`，按大小轮转。当前已接入日志基础设施，关键流程埋点与关于页入口待完成，见[日志执行项](todos/desktop-logging.md)。

Fabric 实例的“添加模组”面板默认使用 Modrinth，无需 API Key，也可切换到 CurseForge。两者均接入项目搜索、兼容版本、必需依赖计划确认、共享校验下载和实例事务启用；窗口下载任务可查看进度/速度与取消，资源任务暂停/恢复尚未接入。Modrinth 搜索按查询/游戏版本/页缓存 5 分钟，兼容版本按项目/游戏版本缓存完整列表并在本地分页；最多 16 个磁盘与内存缓存键，重启沿用原获取时间，过期先返回旧数据并更新，失败保留旧结果，支持手动刷新。安装计划独立读取当前元数据，不使用过期浏览快照。仅验证了真实只读元数据、临时实例计划和缓存恢复；实际下载及游戏内加载待验收。

CurseForge 仍需在启动 Shell 的环境中配置 `ANVIL_CURSEFORGE_API_KEY`，Shell 会将其传给 Rust Runtime；未配置时仅该来源显示等待申请提示，不请求上游。不要使用 `VITE_` / `PUBLIC_` 前缀，不将密钥放入前端、提交到仓库或发送到聊天；配置后需要由维护者重新启动开发进程。CurseForge 生产分发的凭据策略与真实联调仍待完成，详见[客户端资源设计](todos/client-resources.md)。

模组发现页迁移已完成，离线验收通过：Runtime 已提供不依赖实例的项目搜索/摘要 API，可选 Minecraft 版本并固定 Fabric，复用现有 5 分钟有界持久缓存；离线缓存及 loopback API 检查通过，真实上游联调待进行。nav 的 `/mods` 页面已提供来源选择、真实目标实例选择/清除、URL 上下文校验和返回实例入口；项目搜索/滚动加载、项目图标（懒加载及缺失占位）、下载量/更新时间、摘要与来源链接、所选 Fabric 实例的兼容版本已接入，支持宽窗口分栏和窄窗口列表/详情切换，来源和实例筛选使用与实例页共用的可收起侧栏；页内版本选择、依赖/缓存复用预览及确认安装已接入，任务复用窗口级下载中心，真实页内安装待验收。实例页“添加模组”已改为携带当前实例跳转到发现页，旧弹窗已移除；本地导入和已安装模组管理仍留在实例页。

查看游戏版本：

```bash
cargo run --locked -p anvil -- version list
```

安装 Java 21 并查看 Anvil 管理的运行时：

```bash
cargo run --locked -p anvil -- java install --version 21
cargo run --locked -p anvil -- java list
```

创建并启动 1.21.1 原版服务端：

```bash
cargo run --locked -p anvil -- server create survival --version 1.21.1
cargo run --locked -p anvil -- server start survival --accept-eula
```

启动命令中的 `--accept-eula` 表示你明确接受 Minecraft EULA。服务端在前台运行；输入 `stop` 或按 Ctrl-C 可请求保存并正常退出。

安装客户端、登录 Microsoft 账户并前台启动：

```bash
export ANVIL_MICROSOFT_CLIENT_ID=b374e556-655b-41b1-aee9-45821ad631fe
cargo run --locked -p anvil -- client install survival --version 1.21.1
cargo run --locked -p anvil -- account login
cargo run --locked -p anvil -- account list
cargo run --locked -p anvil -- client launch survival
```

也可以在 `account login` 和 `client launch` 命令上显式传 `--client-id <GUID>`。登录会在 stderr 显示 Microsoft 设备码；刷新令牌只写入操作系统凭证库，公开的 Minecraft profile 单独保存在 Anvil 数据目录。`client launch --account <UUID>` 可选择已保存的账户。客户端首次启动可能还需安装匹配版本的托管 Java；启动时会重新读取 Mojang 元数据与 asset index 校验启动计划，不会修复或下载缺失的游戏文件。

客户端启动日志由 Minecraft 写入对应实例的 `game/logs/`。

脚本可使用 `--json` 获取机器可读结果，进度与游戏日志写入 stderr；长时间运行的服务端可使用 `--json-events` 获取 JSONL 生命周期事件。`--data-dir <目录>` 可指定 Anvil 数据目录，也可设置 `ANVIL_DATA_DIR`。

## 后续计划

客户端实现计划见[原版客户端首条可用流程](todos/client-milestone.md)。服务端当前实现与验证范围见[服务端首条可用流程](todos/server-milestone.md)。
