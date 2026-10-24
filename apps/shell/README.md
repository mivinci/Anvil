# Anvil Shell

Anvil Shell 是原生 Tauri 宿主。Rust crate 位于 `src-tauri/`，属于仓库的 Cargo workspace。桌面 SvelteKit 页面由同级的 `apps/desktop` npm workspace 独立维护。

开发模式由 Vite 提供桌面页面；生产 Shell 的 `frontendDist` 只包含 `bootstrap/index.html` 恢复入口。生产窗口通过 `anvil-app://localhost/` 加载本地 App 版本目录中的页面。协议只允许从活动版本 manifest 指定的 `web/` 目录读取资源，并根据文件类型返回 MIME；没有活动版本、manifest 损坏或页面入口缺失时显示恢复页。

在仓库根目录运行 `npm run desktop:dev` 可启动桌面开发模式。该命令使用单独的 Tauri 开发配置：macOS 保留窗口效果，并让主窗口通过 `devUrl` 加载 Vite 页面；开发时直接启动 `apps/desktop/baseline` 中本次构建的 sidecar，不使用已安装的活动版本，也不进行版本激活、回滚或后台更新检查，避免新页面连接旧 API。正式构建仍使用 `anvil-app://localhost/` 加载活动 App 版本。

Rust Runtime/Core 改动后，需要退出整个 `npm run desktop:dev` 命令再重新启动。Tauri 的自动重启只重编 Shell，不会重新执行 `beforeDevCommand` 中的 App 打包步骤；页面热更新或单独重启窗口也不会重建 Runtime。

Windows 主窗口启用透明窗口与 Acrylic 毛玻璃（开发和发行配置均生效），透明顶栏和导航区域透出系统模糊背景，主内容卡片保留主题底色。目前试验原生窗口按钮：`windows_frame.rs` 保留系统装饰，通过 Win32 子类过程扩展顶部客户区，并裁剪 WebView 子窗口的右上角区域，让 DWM 绘制和处理最小化、最大化/还原及关闭按钮；前端只预留 138 × 32 逻辑像素。窗口调整尺寸和 DPI 变化时重新裁剪，最大化时保留系统边框补偿。空白顶栏沿用 Tauri 拖动区域与双击最大化，macOS 保留原生交通灯与原有窗口效果。前端静态检查、Windows Shell 离线编译与 Rust 格式检查通过；此原生按钮方案尚未在实际 WebView 中验收，需检查按钮绘制/悬停/点击、Windows 11 分屏菜单、毛玻璃、拖动、边缘缩放、最大化/还原和多显示器 DPI 切换。

## 构建参数与 App manifest

Shell 构建使用 Cargo 的 target triple 作为目标平台。`ANVIL_DEFAULT_APP_ID` 默认为 `anvil.desktop`，`ANVIL_SHELL_API_VERSION` 默认为 `1.0.0`；`ANVIL_APP_MANIFEST` 可指定与目标平台匹配的 manifest，默认按 target 读取 `apps/desktop/manifests/<target>.json`。manifest 的页面和 sidecar 入口相对于 App 包根目录；`apps/desktop/app-manifest.json` 是 schema 示例。

本地默认构建目标为 `aarch64-apple-darwin`。交叉构建时，构建脚本按 Cargo target 自动选择对应 manifest；也可以显式指定：

```sh
ANVIL_APP_MANIFEST=../../desktop/manifests/x86_64-pc-windows-msvc.json \
  cargo check --manifest-path apps/shell/src-tauri/Cargo.toml \
  --target x86_64-pc-windows-msvc
```

Shell 构建脚本会校验 manifest schema、App ID、目标 triple、API 版本和包内相对入口。`npm run desktop:package` 按 Tauri 构建目标生成 SvelteKit 静态页面、匹配平台的 `anvil-desktop` sidecar 和 manifest，组成 `apps/desktop/baseline`；Tauri `bundle.resources` 将其随 `.app` 或安装包分发。首次启动时 Shell 校验包、复制到 staging、原子发布并激活。已有活动版本不会被基线包覆盖。

Rust 库中的 `app_storage` 提供按 App ID 隔离的本地版本目录、schema 2 原子状态文件（兼容读取 schema 1）、staging 发布和只读包入口解析；`app_protocol` 将活动版本页面安全地提供给 Tauri WebView。Shell 运行时从活动 App 包启动 sidecar、等待受限 ready 握手、监控进程退出，并在 Shell 关闭时请求有序停止。sidecar 仅监听动态 loopback 端口并提供带会话凭据的 `/health`。待启用版本在下次启动时进入健康确认；页面加载或 API 检查失败、Runtime 启动失败以及 30 秒超时都会回滚到上一个健康版本，异常退出后也会在下次启动恢复。首次安装、页面加载和本机健康请求的验收见 [Shell 执行清单](../../todos/shell-milestone.md) 第 7 项，激活和回滚见第 9 项。

## App 更新源

桌面 App 更新由 Shell 在启动时检查，并在运行期间每小时触发一次限频检查；持久化时间戳保证每 24 小时最多请求一次。更新包在后台下载至按版本隔离的 staging，`q2q-http` 负责断点恢复与 SHA-256 校验。Shell 再验证 Ed25519 发布签名、目标平台、App manifest、兼容范围和 tar.gz 解包路径，验证通过后才发布不可变版本并将其设为待启用。活动版本在下载或验证失败时保持不变。

发行构建通过以下两个环境变量配置更新源；必须同时设置，且 manifest URL 必须使用 HTTPS：

```sh
ANVIL_UPDATE_MANIFEST_URL=https://updates.example.com/anvil/stable/aarch64-apple-darwin.json \
ANVIL_UPDATE_PUBLIC_KEY=<32-byte-ed25519-public-key-in-hex> \
  npm run desktop:build
```

更新清单是 JSON envelope：`payload` 是 UTF-8 发布 JSON 的十六进制编码，`signature` 是对 payload 原始字节做 Ed25519 签名后的十六进制值。发布 JSON 包含 schema 版本、`stable` 通道、App ID、SemVer 版本、目标 triple、HTTPS 包 URL、压缩包大小与 SHA-256。包是以 `manifest.json`、`web/`、`runtime/` 为根的 tar.gz。压缩包上限为 1 GiB，解包后上限为 100,000 个文件和 2 GiB。

Desktop 发布由 `.github/workflows/desktop-release.yml` 负责。推送 `desktop/vX.Y.Z` 会构建并发布 stable 安装包，推送 `desktop-test/vX.Y.Z` 会发布隔离的测试版及测试更新源；两者都要求 Tauri 配置和所有平台 App manifest 的版本与 tag 一致。工作流将 stable/test 的更新清单分别维护在 `desktop-updates-stable`、`desktop-updates-test` release tag 下，测试包只读取测试清单。Runtime/CLI 使用独立版本号和 `cli/vX.Y.Z` 发布流程。

仓库需要配置以下 GitHub Actions 密钥和变量：

- Secret `ANVIL_DESKTOP_UPDATE_SIGNING_KEY` 与 Variable `ANVIL_DESKTOP_UPDATE_PUBLIC_KEY`：Desktop stable 更新签名密钥对。
- Secret `ANVIL_RUNTIME_SIGNING_KEY` 与 Variable `ANVIL_RUNTIME_UPDATE_PUBLIC_KEY`：Runtime 更新签名密钥对。
- Secret `ANVIL_DESKTOP_TEST_UPDATE_SIGNING_KEY` 与 Variable `ANVIL_DESKTOP_TEST_UPDATE_PUBLIC_KEY`：Desktop 测试版专用密钥对；测试发布只读写 `desktop-updates-test` feed。

生成 Runtime 密钥对可运行 `cargo run -p anvil --example gen_update_key`。其他通道可将 Secret 和 Variable 名称作为参数传入，例如 `cargo run -p anvil --example gen_update_key -- ANVIL_DESKTOP_TEST_UPDATE_SIGNING_KEY ANVIL_DESKTOP_TEST_UPDATE_PUBLIC_KEY`。私钥只存 GitHub Actions Secret，公钥存 Variable；不要把私钥写入仓库或日志。

`cli/vX.Y.Z` 发布会将签名的 Runtime 索引更新到固定的 `runtime-updates` feed。发布脚本在签名前检查私钥与公钥匹配；缺少配置时会让发行失败，不会悄悄生成无法自更新的正式包。默认本地构建仍不注入发行密钥，更新状态会显示 `not_configured`。更新状态、进度、检查与取消均通过受限 Shell IPC 暴露。

测试发布时，先将 `tauri.conf.json` 和 `apps/desktop/manifests/*.json` 的 App 版本同步到新版本，确认对应的 Desktop Actions Variables/Secrets 已设置，再从包含这些改动的提交创建并推送 `desktop-test/vX.Y.Z` tag。验收测试包时使用隔离的数据目录；测试包只读写 `desktop-updates-test` feed，不会推进 stable feed。验证通过后，可从同一提交发布 `desktop/vX.Y.Z`。

## Shell IPC

`apps/desktop/src/lib/shell.ts` 提供桌面页面的 typed client：查询默认 App 身份、构建 manifest 基线版本与本地活动版本，查询更新状态、检查或取消更新，查询 Runtime 状态并在 Runtime ready 时取得连接信息。页面完成 sidecar `/health` 请求和 API 版本检查后，通过受限 IPC 确认启动健康；失败时报告启动失败以触发回滚。页面只展示由 Shell 配置的更新源状态，不接收页面传入的更新 URL、公钥或程序路径。`apps/desktop/src/lib/runtime.ts` 使用连接信息直接请求 sidecar 健康端点，不经 Shell 转发。

Tauri build 为 Shell 命令生成 ACL 权限，`capabilities/default.json` 将其授予 `main` 窗口，且没有配置远程 URL。命令自身还检查窗口 label 和本地 Shell/dev origin。CSP 限定页面连接到 Tauri IPC、loopback Runtime 和开发模式所需的 Vite HMR 来源。开发模式从 Vite 加载热更新页面，同时准备一份基线包供 sidecar 运行；发行模式从安装包内的基线包首次安装。

## Rust 日志

Shell 与桌面 Runtime 通过 `anvil-logging` 初始化 tracing，Core 不初始化文件输出。开发模式同时写文件与 stderr，Runtime stderr 由 Shell 转发到 dev 终端；发行模式正常情况下只写文件。Runtime stdout 仅用于 ready 握手。

macOS 日志目录为 `~/Library/Logs/Anvil`；Windows 使用 Anvil 本地数据目录的 `logs`，Linux 使用 Anvil state 目录的 `logs`（不可用时回退本地数据目录）。目录解析由共享 crate 提供，Shell 将解析结果通过 ANVIL_LOG_DIRECTORY 传给 Runtime，后续“关于”页复用同一入口；该变量若显式设置必须为绝对路径。

文件为 `shell.log`、`runtime.log`，达到 10 MiB 后轮转，分别保留当前文件及 `.1`–`.4` 历史。采用 1024 条记录的有界异步队列，满时丢弃并在退出时向 stderr 报告数量。正常退出刷新队列；强制终止和断电不保证末尾记录。文件初始化/写入失败回退 stderr；同类进程同时运行时，未取得文件锁者使用 stderr，避免并发轮转。

默认应用模块 info、依赖库 warn；`RUST_LOG` 可开启指定应用模块的 debug，例如：

```sh
RUST_LOG=anvil_shell=debug,anvil_desktop_runtime=debug,anvil_core=debug npm run desktop:dev
```

HTTP 依赖的详细日志被限制，避免泄漏认证请求。应用日志不得记录令牌、验证码、密码、Authorization、请求头/请求体或原始握手数据。已接入启动、握手、更新状态、安装任务阶段及暂停/恢复/取消/结果、账户阶段等日志。安装使用 operation_id，登录使用 login_operation_id 关联；错误只记录稳定类别，API 路由模板/耗时的常规记录为 debug，5xx/超时为 warn，默认不记录轮询进度。设置「关于」提供日志目录、打开目录和 Shell/Runtime 文件记录状态；进入页面或手动刷新时读取，不定时轮询。打开目录 IPC 仅接受启动时确定的日志目录，页面不能指定任意路径。实现与验收范围见[桌面日志执行项](../../todos/desktop-logging.md)。已有 CLI 日志初始化保持原行为。

## 桌面版本目录缓存

桌面安装面板使用窗口级内存状态，Core/Runtime 对 Minecraft 与 Fabric 目录持久缓存 30 分钟，对 Modrinth 与 CurseForge 浏览目录持久缓存 5 分钟，并合并在途刷新。重启沿用磁盘快照原获取时间，过期先显示已有目录再更新；更新失败保留缓存，搜索旁可手动刷新。Modrinth 兼容版本整表按项目/游戏版本缓存，翻页不重新请求上游；两种模组来源各最多 16 个缓存键，文件下载另按内容哈希复用。没有定时轮询。Minecraft 目录接口为鉴权的 `/client/version-catalog`，原 `/client/versions` 数组接口保持兼容；实施和验收见[客户端执行项](../../todos/desktop-client-library.md)及[资源管理](../../todos/client-resources.md)。

App manifest 可配置 `sidecar_args`（默认空数组），Shell 对包内与 override Runtime 都使用同一组参数。Anvil 使用 `["serve"]` 以启动 CLI 的桌面 Runtime 子命令；参数由验证后的 App manifest 提供，不来自页面或 override pointer。

客户端详情使用鉴权的 `GET /clients/{name}` 单独读取实例与有效安装记录，概览展示游戏版本、Java 要求与记录目标平台；不联网、不触发安装或全资源完整性检查。实现与验收见[客户端资源设计](../../todos/client-resources.md)。

实例模组页已支持本地 JAR 导入持久内容仓库、资源引用读取、计划启用/停用与移除；当前不写游戏 mods 目录，加载器接入后才物化。单次导入最多 64 MiB，并发资源请求最多一个；导入内容去重、revision 冲突与恢复边界见[客户端资源设计](../../todos/client-resources.md)。

安装面板已接入 Fabric 官方加载器版本查询与新实例安装，复用 Core 平台计划和 q2q-http 校验下载，实例快照/列表/详情及重试保存所选 Fabric 版本。原版与 Fabric 环境不在已有实例间静默互换；其他加载器仍未开放。Fabric API 模组和资源入口物化另行实施，真实 Fabric 安装/运行尚待维护者验收。

Fabric 实例启用/停用/移除本地模组时，现已同步独立副本到 game/mods；共享原件不被修改。受管布局和事务日志支持失败/下次访问恢复，外部同名文件不覆盖。资源页可显式同步，Core/CLI 启动准备也会恢复并检查；依赖/兼容性求解及真实 Fabric 加载验收仍未完成。

WebView 的开发及发行 CSP 允许 HTTPS 图片用于 App 的远程公开图标；其他远程脚本和网络连接策略保持现有范围。具体图片来源、URL 校验与缺失占位由 App 负责，Shell 不识别模组平台。CSP 变更需要重新构建并启动 Shell 后生效。
