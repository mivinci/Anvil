# 桌面 Rust 日志执行项

开发和发行共用 tracing 配置；开发同时输出文件与终端，Runtime stdout 保留 ready 握手用途。Core 只产生事件，不持有日志文件或初始化全局 subscriber。

1. [x] 日志基础设施：Shell/Runtime 共用独立日志 crate；系统应用日志目录；各进程类型独立文件；10 MiB 轮转、当前加 4 份历史；有界异步队列、退出刷新、文件失败回退 stderr；开发终端转发 Runtime stderr；应用模块支持 RUST_LOG。
2. [x] 关键流程事件：Shell 启动/握手/退出/更新，Runtime API/安装/暂停继续/取消/账户阶段；任务 ID 关联，不按下载帧刷日志，不记录凭据或请求体。
3. [x] 关于页入口：实际日志目录、打开目录与文件不可用状态；完成静态检查并记录维护者验收范围。

每项完成后停下来由维护者查看。不得自动启动/关闭 dev、登录、下载游戏或清理用户日志。

## 输出与限制

- macOS：`~/Library/Logs/Anvil`；Windows：应用本地数据目录的 `logs`；Linux：应用 state 目录的 `logs`，无 state 时回退应用本地数据目录。
- 日志命名：`shell.log` / `runtime.log`，历史后缀 `.1` 到 `.4`。文件轮转以大小触发，单条事件可使文件略超过阈值。
- 终端也写 stderr，不使用 Runtime stdout。默认应用模块 info、依赖库 warn；RUST_LOG 可调整应用模块，依赖库详细 HTTP 日志不开放，避免暴露认证数据。
- 异步队列有界；满时允许丢弃，退出报告丢弃数量。正常退出尽力刷新，强制杀进程/断电不保证末尾记录。
- 同一进程类型重复启动时锁定日志写入权；无法获得锁的进程回退 stderr，避免并发轮转同一文件。
- 文件输出失败时报告具体阶段、错误类别及系统错误码，区分打开文件失败、锁被其他进程持有及写入/刷新失败；日志失败继续回退 stderr，不阻止 WebView 或 Runtime 启动。
- 不记录 access/refresh token、Bearer、密码、设备验证码、请求头/请求体；关键事件优先记录阶段、任务 ID 和稳定错误类别，详见第二项。

## 第一项实现与检查

已新增被 Shell/Runtime 实际复用的 `anvil-logging` crate，不依赖 Core。复用 tracing-subscriber、tracing-appender 的有界异步写入与 rolling-file 的大小轮转；没有自行实现日志队列或轮转算法。Unix 日志目录限制为用户私有，固定文件名及文件锁保护轮转。文件不可用状态由日志 guard 提供查询能力，UI 接入留在第三项。

Shell 在 Tauri `RunEvent::Exit` 回调内完成 Runtime/更新服务清理后主动 drop 日志 guard（Tauri 会直接退出进程）；Runtime 使用 ExitCode 正常返回而非 process::exit，保证正常路径能刷新队列。Shell 的 env_clear 启动环境额外传递 RUST_LOG、受控的 ANVIL_LOG_STDERR 开关和已解析的 ANVIL_LOG_DIRECTORY，确保清空环境后仍使用相同目录；不透传其他环境信息。Runtime stderr 转发父进程 stderr，不落盘原始协议或请求。

检查：日志 crate、Shell、Runtime 离线编译通过；Shell custom-protocol 发行路径编译通过；Runtime release 构建、Rust 格式检查通过。新依赖从官方 crates.io 下载并匹配 Cargo.lock 校验。未运行自动化测试，未主动启动/重启 dev、写用户日志目录或模拟真实轮转/磁盘写入故障；文件生成、开发终端输出和正常退出落盘待维护者重启后确认。关键业务埋点见第二项。

维护者首次运行检查：shell.log 与 runtime.log 已创建但均为 0 字节。定位为 EnvFilter::from_env 在 RUST_LOG 未设置时成功返回仅 ERROR 的过滤器，导致默认 info 初始化事件被过滤。现已显式为未设置/空值选择应用默认指令；隔离临时目录探针验证两种情况均能写入初始化记录，探针及临时目录已删除。未触碰正在下载的进程；实际 dev 需在下载完成后重启才加载修复。业务阶段日志已在第二项接入。

## 第二项实现与检查

Runtime 合入 CLI 后默认 info 过滤目标已改为实际的 `anvil` 模块，移除旧 `anvil_desktop_runtime` 目标，避免默认配置下丢失启动阶段日志。启动失败以 warn 记录 launch_id、失败阶段和稳定错误码；Unsupported 另记录元数据/占位值兼容性原因，不输出启动 argv、访问令牌或原始认证错误。

Shell 记录 Runtime 启动、ready 握手、状态/失败类别、异常退出和强制终止，以及更新检查和更新阶段/失败类别。Runtime 记录 API 就绪、关闭清理、客户端任务接受、安装尝试、观察到的阶段切换、暂停请求/停止完成、恢复请求、取消请求/清理完成、成功或失败；使用 operation_id 关联，完成记录包含耗时、收到字节数与文件数。阶段通过已有 watch 进度观察，快速连续切换可能合并，不宣称它是逐事件审计流。

登录以 login_operation_id 记录开始、授权阶段、保存、完成/取消/失败；账户切换和注销只记动作与结果。安装错误记录 Core 稳定 error_code，授权错误记录稳定类别，保留现有 UI 错误详情但不将其原文写入日志。API 只记录路由模板、方法、状态和耗时：普通请求 debug，5xx/超时 warn；轮询默认不刷日志。未授权请求不落请求数据，日志不含实例名、账号身份、URL/query、验证码、请求体、请求头或认证凭据。

检查：Runtime/Shell 离线编译和 Rust 格式检查通过；更新 Runtime release 基线。未运行自动化测试、未启动或重启 dev、未执行真实下载/登录；维护者下载完成并重启后验证实际业务日志。下一项为关于页日志入口。

## 第三项实现与检查

设置「关于」增加日志目录、打开目录、Shell/Runtime 文件记录状态和手动刷新。状态来自日志 writer 共享的原子标记，初始化失败或后续异步写入失败后显示不可用；不以日志文件存在与否推断工作状态。目录为 Shell 启动时保存的解析结果，未能解析时显示目录不可用；Runtime 状态读取使用受认证的 health 字段（旧 Runtime 无字段显示状态不可用），不触发启动确认或回滚，HTTP 读取 5 秒超时。无定时轮询。

新增两个本地 Shell IPC，沿用 main 窗口与本地 origin 校验并添加 ACL；打开目录不接受页面传入路径，使用系统文件管理器，不创建/删除目录。读取失败与打开失败在日志区内提供提示，浏览器预览显示仅桌面端可用。现有 Runtime health 增加可选 fileLoggingAvailable 字段，不改变握手协议。

检查：Svelte/TypeScript 检查 0 错误、0 警告；Shell/Runtime 离线编译、Shell custom-protocol 编译与 Rust 格式检查通过，Runtime release 基线已更新。未运行自动化测试，未操作用户 dev 或实际打开文件管理器；页面视觉、真实目录打开、写入失败/恢复显示和跨平台行为待维护者验收。
