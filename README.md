# Anvil

Anvil 是面向 Windows、macOS 和 Linux 的轻量 Minecraft Java 版启动器与服务端管理工具。目前项目仍在开发中，暂未提供可直接安装的发布包。

## 现在可以做什么

- 查询 Mojang 官方游戏版本列表。
- 安装、查看和移除 Anvil 管理的 Java 运行时，不修改系统 Java 或 `PATH`。
- 创建、查看和前台运行原版 Minecraft 服务端；启动时可由你明确接受 EULA，并支持控制台停止和 Ctrl-C 优雅退出。
- 安装和列出原版客户端，使用 Microsoft 设备码登录，并在前台启动客户端。

CLI 与 Slint 桌面版都已接入原版客户端安装、Microsoft 设备码登录、账号选择和客户端启动流程。桌面版通过后台 Tokio 运行时调用同一套 Core 服务，关闭窗口会取消并清理 Anvil 持有的操作。真实 Microsoft 登录、系统凭证库写入和客户端运行尚未验收；首个端到端目标是 Minecraft 1.21.1。服务端已在 Linux x86_64 上实测；Windows 和 macOS 的真实运行结果仍待维护者验证。

## 试用方式

目前需要从源码运行，并安装 Rust 1.89 或更新版本。Linux 桌面版还需要 `pkg-config` 和 Fontconfig 开发包；只运行 CLI 不需要这些桌面依赖。

启动桌面版：

```bash
cargo run --locked -p anvil-gui --features desktop
```

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

桌面版使用 Anvil 注册的 Microsoft 公共客户端 ID，不需要客户端密码；登录时在窗口内展示设备码和验证地址。客户端启动日志由 Minecraft 写入对应实例的 `game/logs/`。

脚本可使用 `--json` 获取机器可读结果，进度与游戏日志写入 stderr；长时间运行的服务端可使用 `--json-events` 获取 JSONL 生命周期事件。`--data-dir <目录>` 可指定 Anvil 数据目录，也可设置 `ANVIL_DATA_DIR`。

## 后续计划

客户端实现计划见[原版客户端首条可用流程](todos/client-milestone.md)。服务端当前实现与验证范围见[服务端首条可用流程](todos/server-milestone.md)。
