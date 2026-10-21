# Anvil

面向 Windows、macOS 和 Linux 的轻量 Minecraft Java 版客户端与服务端管理工具。

采用 Cargo workspace：`anvil` 提供 CLI，`anvil-core` 实现公共业务，`anvil-gui` 提供可选 Slint 桌面入口。当前 CLI 可列出 Mojang 版本目录、管理托管 Java，并创建、列出和前台启动原版服务端实例。服务端启动会准备 EULA 与匹配的托管 Java，支持控制台输入和 Ctrl-C 优雅停止；Linux x86_64 上已实跑 Minecraft 1.21.1 并验证世界保存退出，其他平台结果仍待维护者实测。客户端文件安装与启动尚未实现，GUI 目前只有基础窗口。

## 开发与使用

需要 Rust 1.89 或更新版本。Linux 桌面构建还需要 pkg-config 和 Fontconfig 开发包（Debian/Ubuntu 为 `libfontconfig1-dev`；CLI 不需要）。提交的 Cargo.lock 固定依赖；`q2q-http` 固定到 Q2Q Git revision，不依赖本机相邻仓库。

```bash
cargo build -p anvil --locked

# 查询官方版本目录
cargo run -p anvil -- version list --json

# 大版本：该版本最新稳定补丁；精确版本：含 build 号
cargo run -p anvil -- java install --version 21
cargo run -p anvil -- java install --version 21.0.8+9
cargo run -p anvil -- java list
cargo run -p anvil -- java uninstall <runtime-id>

# 创建原版服务端实例并下载经校验的官方 JAR
cargo run -p anvil -- server create survival --version 1.21.1
cargo run -p anvil -- server list --json
cargo run -p anvil -- server start survival --accept-eula

# 前台自动化：stdout 为 JSONL 生命周期事件，日志仍写 stderr
cargo run -p anvil -- --json-events server start survival --accept-eula

# 自动化：--json stdout 只输出一个结果 envelope，错误也具有稳定 code
cargo run -p anvil -- --json java install --version 21

# CLI 与未来 GUI 使用相同的数据根约定
cargo run -p anvil -- --data-dir ./anvil-data java list
# 也可通过 ANVIL_DATA_DIR 设置

# 可选桌面入口；CLI 构建不编译 Slint
cargo run -p anvil-gui --features desktop --locked
```

Java 默认发行版为 Eclipse Temurin，优先 JRE，无匹配 JRE 时尝试 JDK。只安装到 Anvil 的用户目录，不修改系统 Java、PATH 或 JAVA_HOME。详细行为、平台范围及限制见 [Java 管理](todos/java-management.md)。

## 技术方向

- 无全局常驻服务，入口拥有运行时，公共核心按需工作。
- 复用 Q2Q 的 `q2q-http` 连接池、并发下载、哈希校验与恢复能力。
- 下载、解包、Java 探测与进度缓冲有界，CLI 使用单线程 Tokio runtime。
- CLI 为脚本和 AI 工具提供带 schema 版本的 JSON 结果及稳定退出码。

## 设计文档

- [架构方案](todos/architecture.md)：CLI/GUI 与共享核心、安装与运行生命周期及实施阶段。
- [Java 管理](todos/java-management.md)：当前实现、命令、安装事务和后续边界。
- [原版服务端首条可用流程](todos/server-milestone.md)：服务端从元数据解析到 EULA、前台启动与跨平台验收的分步执行项。
- [Minecraft 安装与启动依据](todos/minecraft-installation.md)：官方文件清单、版本差异、Java 与参数生成规则。
