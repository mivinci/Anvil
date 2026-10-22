# Anvil 项目方向与架构约定

本文记录已确认的产品方向和工程边界，不代替实现计划。当前功能和验证范围以代码、[README](../README.md) 及 `todos/` 中的状态为准。

## 产品方向

- Anvil 是面向 Windows、macOS 和 Linux 的轻量 Minecraft Java Edition 启动与服务端管理工具。
- `anvil` 是 CLI 入口，服务终端用户、脚本和后续 AI 工具；`apps/shell` 是通用 Tauri 启动器，按 App ID 启动和维护版本；`apps/desktop` 是独立 App，拥有桌面界面和产品流程。桌面 Runtime 已提供版本目录、客户端安装/列表及账户管理 API；UI 重构后已接入窗口框架、外观设置、账户菜单/登录弹窗，以及客户端列表、安装、进度、暂停/继续、取消和显式重试；完成后联动刷新实例列表。客户端启动和真实下载、登录验收仍待完成。
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
    anvil/                   # CLI 参数、终端交互、JSON/JSONL、退出码
    anvil-desktop-runtime/   # 桌面 App 专属 sidecar 可执行程序
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
- `anvil-core` 不依赖 UI 框架、CLI 参数解析、终端交互或 MCP，也不自行创建 Tokio runtime。由每个入口拥有 runtime 和业务上下文。`anvil-desktop-runtime` 是桌面 App 的独立进程入口，复用 Core；它留在 Cargo crates 树中，与 npm 前端包解耦。
- `apps/mobile` 未来使用独立的 SvelteKit 页面、布局和交互，不复用 `apps/desktop` 的页面结构；可以共享 API schema 和无界面逻辑。
- CLI 与 App 自己的业务组件可调用 core，不通过启动 CLI 子进程实现常规操作。Shell 只负责通用 App 生命周期和版本维护，不直接调用 core。前端使用 SvelteKit 静态输出，避免依赖 Node 服务端运行时。
- 保持 crate 和模块按现有职责组织。新增 crate、公共 trait 或通用抽象前，先确认当前需求确实需要它。

## 桌面日志

Shell 与 Runtime 入口共用 `anvil-logging`，开发模式写终端和文件，发行模式写文件；Core 仅产生 tracing 事件，不持有日志输出。文件轮转、队列与历史数量有界，Runtime stdout 保留为握手协议，日志使用 stderr。不得记录认证凭据、验证码或请求体。实施状态与验收边界见[桌面日志执行项](../todos/desktop-logging.md)。

## 下载与文件安全

- Anvil 通过固定 revision 的 `q2q-http` 复用 Q2Q 提供的 HTTP Executor、并发下载、哈希校验和恢复能力。不要另写 Range worker、重试队列或下载恢复格式。
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
