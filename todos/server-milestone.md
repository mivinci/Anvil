# 原版服务端首条可用流程

状态：第 1–6 项已完成，下一项是扩展真实运行与平台覆盖。每一项完成后检查验收条件，再进入下一项。第一轮只做原版服务端，不加入客户端账户、模组加载器、后台守护进程或 MCP。

## 目标

打通创建服务端目录、安装官方服务端文件、准备所需 Java、明确接受 EULA、前台运行和优雅停止的流程：

```bash
anvil server create survival --version 1.21.1
anvil server start survival --accept-eula
```

`create` 准备可恢复的实例并安装服务端 JAR；`start` 检查 Java 与 EULA 后，在实例目录启动服务端并持有前台进程。进程退出后保留 world、server.properties、日志及其他服务端产生的数据。

首个完整验收样本使用 1.21.1：现有资料已核对其版本 JSON、Java 21 要求和服务端 bundler 布局。元数据解析仍按所选版本读取，不能把 1.21.1 的格式假设写成全部版本的规则。另一当前正式版本可在第一轮实跑后加入版本覆盖检查。

## 执行项

### 1. [x] 收敛服务端版本解析契约

**交付：** 在 `anvil-core` 增加服务端所需的最小 metadata 类型与解析函数；版本总清单按版本 ID 查找版本 JSON，校验父元数据声明的 SHA-1，再读取版本、Java 主版本及 `downloads.server`。仅解析服务端路径真正需要的字段，不提前建立客户端库和参数模型。

**失败处理：** 未知版本、JSON 哈希错误、缺少服务端下载项、无效长度或 SHA-1、非官方元数据来源，都返回稳定错误码；不得尝试客户端 JAR 或回退到猜测的 URL。通过共享 `q2q-http::Executor` 做有大小上限的元数据读取。

**验收：** 固定样本覆盖成功解析和上述错误；测试校验原始 JSON 字节的哈希；元数据请求不依赖独立 HTTP 客户端或运行时。已完成共享 `q2q-http` Executor 解析、1.21.1 Java/JAR 信息检查、哈希错误和缺字段测试；`anvil-core` 的 22 项测试通过（含本地 HTTP fixture）。真实 Mojang 1.21.1 元数据 smoke 已通过。

### 2. [x] 建立服务端安装计划

**交付：** 将解析出的服务端 artifact 和 Minecraft 版本映射成安装计划。artifact 携带 HTTPS URL、官方长度与 SHA-1；下载用已固定版本的 `q2q-http`，不再写 Range、并发或断点恢复实现。

**验收：** 已在 `anvil-core` 加入 `ServerInstaller`：从已解析的 artifact 生成 SHA-1 内容寻址计划，通过共享 `q2q-http` 下载，并由 q2q-http 检查长度和哈希后才发布到缓存。缓存可以复用；哈希错误和截断响应不会产生最终缓存文件，也不会写入实例记录。Core 的 26 项本地测试通过，覆盖下载、校验失败、截断和重复缓存使用。服务端缓存另有跨进程锁；若已发布文件后来损坏，会删除损坏副本并重新下载校验。

### 3. [x] 实现服务端实例创建与记录

**命令：**

```bash
anvil server create <name> --version <id>
anvil server list [--json]
```

**交付：** 增加最小的 Server 实例记录和稳定 ID；实例可变数据与共享缓存分离。创建时校验名称，获得跨进程实例锁，在同文件系统 staging 中准备文件后提交记录。已经存在的同名实例明确报告，不覆写 world 或用户配置。失败后可重试，不能将半成品标为已安装。

**目录：** 使用 Anvil 的用户数据根和现有 `--data-dir`。实例目录保存版本、server JAR 身份、Java 要求、EULA 状态及后续运行状态需要的字段；JAR 放在实例根目录，世界与配置放在 `game/`，均属于实例自身数据。

**验收：** 已实现 `anvil server create` 和 `anvil server list`。名称限制为 48 字节内的 ASCII 字母/数字及 `._-`，大小写映射到稳定 `server-<name>` ID。创建持实例锁，在实例根目录的同文件系统 staging 中复制并再次校验缓存 JAR、写入含 `type: server` 的 schema 1 `instance.json`，最后原子发布目录；实例目录含 `game/`，EULA 初始为未接受。同名及大小写冲突明确报错，不覆盖世界；损坏缓存、取消和并发创建不会发布半成品。CLI list 返回单一 schema-versioned JSON envelope。CLI/Core 测试共 38 项通过（6 CLI、32 Core），另有 1 项 live Mojang smoke 按需忽略。

### 4. [x] 接入 Java 选择与 EULA 决定

**命令：**

```bash
anvil server start <name> --accept-eula
```

**交付：** `anvil-core` 的 `prepare_start` 以实例版本元数据中的 Java 主版本匹配托管 Java。复用当前 OS/CPU 架构上适用且可运行的已安装运行时；没有可用匹配项时通过 `JavaManager` 安装，并验证运行时的版本、架构与可执行性。服务端创建和运行不需要微软账户。

`--accept-eula` 是明确决定：没有之前的接受记录且未提供时，预检以 `eula_required`（退出码 3）结束，不安装 Java、不启动 JVM；提供后将接受状态及官方条款 URL 原子写入实例记录，并生成 `game/eula.txt`。用户将该文件改回 `eula=false` 后，需要再次明确接受。EULA 状态不混入共享缓存。

**验收：** core 预检测试覆盖缺少接受决定时不产生 EULA/Java 文件、显式接受及持久化、匹配 Java 的真实可执行探测与复用、重复预检，以及 EULA 文件撤回后要求再次明确接受。CLI 前台生命周期见第 5 项。该阶段 Core/CLI 测试 41 项通过（6 CLI、35 Core），1 项 live Mojang smoke 按需忽略；`cargo clippy --all-targets -D warnings` 与 rustfmt 通过。

### 5. [x] 实现前台启动、输入与正常退出

**交付：** 以 Tokio process 直接传递 argv，在实例目录运行官方 bundler JAR，参数为 `-jar <server.jar> nogui`。不通过 shell 字符串启动，不套用客户端 mainClass、assets、account 或 classpath。启动准备对象持有实例锁直到前台子进程退出。

CLI 转发用户控制台输入并持续排空子进程 stdout/stderr 到 CLI stderr。Ctrl-C 请求向服务端 stdin 发送 `stop`，等待世界保存及进程退出；30 秒后提示进程仍在运行，并继续持有子进程和实例锁，不误报已停止。实例目录的 `last-session.json` 记录启动、运行、启动失败及退出结果，不凭裸 PID 误发信号。`--json` 的 stdout 留给最终机器结果，服务端输出不会混入其中。

**验收：** CLI 集成测试用伪 Java 验证准确 argv、控制台转发、服务端日志转发、干净 JSON stdout、持久化退出记录、非零退出和 EULA 前置错误；core 测试验证准备对象持锁、并发 start 返回 busy、释放后可重新准备。真实 Minecraft 1.21.1 Linux x86_64 启动、Ready、世界保存和正常退出 smoke 已在第 7 项记录；Ctrl-C 终端信号与 Windows/macOS 行为仍待验证。

### 6. [x] 固定自动化接口和错误语义

**交付：** `--json` 保持单一 schema 1 结果 envelope；`server start` 的 `--json-events` 使用独立 schema 1 JSONL 状态事件，stdout 每个事件立即 flush。事件状态为 `preparing`、`starting`、`running`、`ready`、`stopping`、`exited`、`failed` 和 `cancelled`；服务端输出留在 stderr。`--json-events` 只支持 `server start`。启动进程成功创建只产生 `running`，只有观察到 Minecraft 的 `Done ... For help` 日志才产生 `ready`；进程未 Ready 就退出会以 `server_not_ready` 失败。错误事件包含稳定错误码、说明和 Anvil 退出码；服务端非零退出另带 `process_exit_code`。

**验收：** CLI 子进程测试验证旧 `--json` 单结果格式、JSONL 生命周期、事件在进程退出前可消费、子进程日志不混入 stdout、EULA/服务端退出错误、Ready 前退出失败及 stdin EOF 不挂起。CLI 测试 18 项、Core 测试 35 项通过；Clippy 和 rustfmt 通过。终端 Ctrl-C 真实信号仍待第 7 项跨平台实跑。

### 7. 扩展真实运行与平台覆盖

**交付：** 完成 1.21.1 各目标平台完整安装/运行 smoke；添加一个当前正式版的元数据解析与最小启动用例，确认差异后更新支持边界。触发现有 Linux/Windows/macOS CI，并在各系统跑安装和基础运行验证；按结果修复文件锁、JRE 布局、进程信号与打包差异。

**当前实跑记录：** 2026-10-21 在 Linux x86_64 临时数据目录中下载并启动官方 1.21.1 服务端。使用托管 Temurin 21，服务端仅绑定 `127.0.0.1`；约 14 秒到达 `Done`，通过控制台 `stop` 后三个维度均报告保存完成，进程 exit code 为 0，生成了世界和 `last-session.json`。首次 smoke 关闭了在线认证，因此有 offline/insecure 警告。随后保持默认 `online-mode=true` 重跑，约 13 秒 Ready，正常保存退出且没有该警告；运行环境两次都拒绝了 Mojang 公钥外连并记录一条错误，但不影响 Ready 和保存退出。此次没有验证 Ctrl-C，也不代表 Windows/macOS 或其他游戏版本已通过。

2026-10-21 再次运行新 smoke 脚本：Linux x86_64 上 1.21.1 通过控制台 stop 和 SIGINT 两种方式到 Ready、保存世界并以 0 退出；Mojang 当前正式版 26.3 也成功完成元数据解析、安装、Ready、存档和正常退出。两版均使用 `online-mode=true` 与 `server-ip=127.0.0.1`。本执行环境仍拒绝 `api.minecraftservices.com` / `discovery.minecraftservices.com` 外连，因此没有验证玩家登录；认证服务告警没有阻断启动与保存。脚本结束后临时实例及下载数据已自动删除。

**跨平台实跑准备：** `.github/workflows/rust.yml` 增加了仅 `workflow_dispatch` 可触发的 `live-server` job，输入 `live-server` 默认关闭，因此普通 push/PR 不会下载服务端或 Java。任务在临时数据根创建并删除实例，明确接受临时测试 EULA，配置 `server-ip=127.0.0.1`、保持 `online-mode=true`，检查 Ready、干净退出及世界文件。`scripts/server-smoke.py` 在 Ubuntu 额外测 Ctrl-C，并解析 Mojang manifest 选取当前正式版做一次安装与最小启动。该工作流尚未触发；当前仓库没有 Git remote，Windows/macOS 与当前正式版结果仍待实际运行。

**验收：** 只对实际通过的 OS/CPU/游戏版本组合宣称支持。Anvil 当前尚无 Git remote；配置远程 CI 前，本地 Linux 检查不能代替 Windows/macOS 结果。

## 首轮不包含

客户端登录、客户端参数和 natives、Fabric/Forge/NeoForge、服务端后台托管、跨命令 stop、远程控制台、MCP、自动世界备份、服务端更新覆盖。每项需要独立的实例数据或进程生命周期设计。

## 依据

- [整体架构](architecture.md)：crate 边界、进程所有权、数据布局与 AI/JSON 契约。
- [Minecraft 文件安装与启动依据](minecraft-installation.md)：总清单校验、server artifact、Java 版本、bundler 入口和未验证项。
- [Java 管理](java-management.md)：Temurin 安装、运行时核验与验收范围。
