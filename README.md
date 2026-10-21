# Anvil

Anvil 是面向 Windows、macOS 和 Linux 的轻量 Minecraft Java 版启动器与服务端管理工具。目前项目仍在开发中，暂未提供可直接安装的发布包。

## 现在可以做什么

- 查询 Mojang 官方游戏版本列表。
- 安装、查看和移除 Anvil 管理的 Java 运行时，不修改系统 Java 或 `PATH`。
- 创建、查看和前台运行原版 Minecraft 服务端；启动时可由你明确接受 EULA，并支持控制台停止和 Ctrl-C 优雅退出。

客户端文件安装、登录和启动尚未实现。桌面版目前只有基础窗口。服务端已在 Linux x86_64 上实测；Windows 和 macOS 的真实运行结果仍待维护者验证。

## 试用方式

目前需要从源码运行，并安装 Rust 1.89 或更新版本。Linux 桌面版还需要 `pkg-config` 和 Fontconfig 开发包；只运行 CLI 不需要这些桌面依赖。

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

脚本可使用 `--json` 获取机器可读结果，长时间运行的服务端可使用 `--json-events` 获取 JSONL 生命周期事件。`--data-dir <目录>` 可指定 Anvil 数据目录，也可设置 `ANVIL_DATA_DIR`。

## 后续计划

客户端实现计划见[原版客户端首条可用流程](todos/client-milestone.md)。服务端当前实现与验证范围见[服务端首条可用流程](todos/server-milestone.md)。
