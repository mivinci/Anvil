# Anvil 项目方向与架构约定

本文记录已确认的产品方向和工程边界，不代替实现计划。当前功能和验证范围以代码、[README](../README.md) 及 `todos/` 中的状态为准。

## 产品方向

- Anvil 是面向 Windows、macOS 和 Linux 的轻量 Minecraft Java Edition 启动与服务端管理工具。
- `anvil` 是 CLI 入口，服务终端用户、脚本和后续 AI 工具；`anvil-gui` 是可选的 Slint 桌面入口。
- CLI 与 GUI 共享 Rust 业务核心。CLI 的 JSON 接口也作为自动化能力的基础；以后接入 MCP 时复用同一核心，不重新实现安装和启动规则。
- 资源目标是按需运行，不设无明确需要的全局常驻服务。下载、解包、日志和并发资源应有界；桌面与 CLI 不因空闲而轮询或长期占用高资源。
- 当前已实现托管 Java 管理和原版服务端创建、列表、前台启动。客户端安装/登录/启动、模组加载器、服务端后台托管和 MCP 仍未实现；不要将它们描述为现有能力。
- 只对实际验证过的 OS、CPU 架构及 Minecraft 版本宣称支持。Linux 成功不能替代 Windows 或 macOS 的运行证据。

## Workspace 与 crate 边界

```text
Anvil/
  Cargo.toml                # workspace、共享依赖和构建配置
  Cargo.lock
  crates/
    anvil-core/              # 公共业务、安装、存储、Java 与服务端生命周期
    anvil/                   # CLI 参数、终端交互、JSON/JSONL、退出码
    anvil-gui/               # 可选 Slint 桌面入口和 UI 桥接
  todos/                     # 方案、阶段任务及验证记录
```

- `anvil-core` 不依赖 Slint、CLI 参数解析、终端交互或 MCP，也不自行创建 Tokio runtime。由每个入口拥有 runtime 和业务上下文。
- `anvil` 与 `anvil-gui` 直接调用 core；GUI 的常规操作不通过启动 CLI 子进程实现。
- Slint 通过 `anvil-gui` 的 `desktop` feature 引入；默认 workspace 构建不应强制编译桌面依赖。
- 保持 crate 和模块按现有职责组织。新增 crate、公共 trait 或通用抽象前，先确认当前需求确实需要它。

## 下载与文件安全

- Anvil 通过固定 revision 的 `q2q-http` 复用 Q2Q 提供的 HTTP Executor、并发下载、哈希校验和恢复能力。不要另写 Range worker、重试队列或下载恢复格式。
- 官方元数据与 Artifact 的可信哈希决定内容身份。传输完成不等于校验通过；校验、原子发布和安装记录提交是不同阶段。
- 缓存放可验证、可重建的共享内容；实例目录放世界、配置和会话等可变数据。安装 staging 与最终发布应避免让半成品被识别为已安装。
- 解包、下载、元数据读取和日志缓冲设定上限；取消操作要等待它拥有的实际 I/O 与清理工作完成。

## CLI 与进程生命周期

- `--json` 为短操作输出单一 schema 版本化 envelope，保持 stdout 可解析；进度和诊断写 stderr。
- 长时间运行的 `server start` 可用 `--json-events` 输出 JSONL 生命周期事件。只有观察到服务端 Ready 且进程正常结束，才将启动记录为成功；服务端日志不混入 JSON stdout。
- `--accept-eula` 表达用户明确接受，不能自动接受。测试 EULA 仅用于隔离的临时验收数据。
- 服务端当前以前台方式运行，Anvil 持有进程和实例锁，并通过控制台 `stop` 或处理 Ctrl-C 请求优雅停止。不要在未设计后台进程所有权和跨命令 stop 前引入守护进程。
- 原版服务端不需要玩家微软账户。在线认证是否开启属于服务端配置；真实 smoke 默认保持 `online-mode=true`，测试端口只绑定 loopback。

## 验收与能力边界

- 普通 push/PR CI 运行本地测试 fixture，不下载真实 Minecraft 服务端。真实服务端跨平台验收单独手动触发并默认关闭。
- 记录 smoke 的精确 OS、CPU、Minecraft 版本、Ready、停止和保存结果。认证服务连通性单独说明；没有玩家登录验证时，不声称认证成功。
- 阶段执行清单和已验证结果见 [服务端实施项](../todos/server-milestone.md)。Java 安装来源和平台差异见 [Java 管理](../todos/java-management.md)，Minecraft 官方元数据和文件结构依据见 [安装设计依据](../todos/minecraft-installation.md)。
