# Anvil Shell 设计草案

状态：设计草案，动态 App 包管理和本机业务 API 尚未实现。当前代码完成了 Tauri 宿主与 SvelteKit 页面拆分；页面仍由 Tauri 构建流程打包加载。`anvil-shell` 已不再依赖 `anvil-core`，其余动态加载与版本管理能力仍待实现。

## 目标与职责

Anvil Shell 是通用的原生 App 启动器和版本管理器。发行构建指定目标平台与默认 App ID（例如 `anvil.desktop`）；运行时 Shell 按 App ID 定位、启动和维护对应 App 包。App 包可以属于桌面、移动端或未来的其他 Anvil App。

Shell 负责 App 清单、入口、兼容性、健康状态、版本生命周期与 Shell 自身的原生能力。它不拥有 Minecraft 业务，不依赖 `anvil-core`，也不定义安装、账户、Java、客户端或服务端命令。每个 App 自己拥有界面、用户流程和业务 API；需要公共规则时，由 App 的业务组件调用 `anvil-core`。

## Runtime 结构

```mermaid
flowchart LR
    Build["构建参数\n平台 + 默认 App ID"] --> Shell["anvil-shell\n通用 Tauri 宿主"]
    Shell --> Registry["App Registry\n活动版本 / 待启用版本"]
    Shell -->|加载本地页面资源| UI["SvelteKit App 页面"]
    UI -->|Tauri IPC\nShell 原生能力| Shell
    UI -->|本机 HTTP\nApp 业务 API| API["App 专属 sidecar"]
    Shell -->|启动 / 监管| API
    API --> Core["anvil-core\n由 App 复用"]
    Shell -. 后续 .-> Mobile["anvil.mobile App 包"]
```

- `apps/shell/src-tauri` 负责通用启动、App 包发现、更新检查/下载/校验/切换/回滚，以及 App 进程生命周期。
- `apps/desktop` 拥有桌面 SvelteKit、shadcn-svelte、Lucide 界面和产品流程。其业务 API 与实现属于该 App。
- 桌面 App 包包含静态页面资源与 App 专属 Rust sidecar。sidecar 依赖 `anvil-core` 并提供本机 HTTP API；Shell 启动和监管 sidecar，但不转发或解释业务请求。
- sidecar 是 Shell 管理的 App 进程，可在 App 运行期间常驻；首版不让它脱离 Shell 成为系统级 daemon。Shell 退出时请求 sidecar 有序停止，超时后结束进程。
- `apps/mobile` 以后仍使用 SvelteKit，但拥有不同的导航、页面布局和交互。移动原生能力由该 App 自己的 Tauri plugin 提供，不复用桌面 sidecar 生命周期。
- desktop 与 mobile 可以共享 API schema、纯类型和无界面逻辑，不共享页面布局。只有出现第二个消费者时再抽取 `packages/shell-api` 等共享前端包。

## 页面与 Rust 通信

桌面页面有两种明确的通信路径，按能力归属选择：

1. **Shell 原生能力走 Tauri IPC。** 页面通过显式、类型化的 `invoke()` 命令访问 Shell 拥有的能力，例如 App 身份、版本、更新状态、更新检查和 Runtime 生命周期。更新进度等 Shell 事件可通过 Tauri `Channel<T>` 返回。命令由 Shell 实现，并用 Tauri capabilities 限定可调用窗口。
2. **App 业务 API 走 sidecar 本机 HTTP。** 页面直接请求当前 App sidecar，不经 Shell Rust 转发。账户、客户端、服务端等业务接口由 sidecar 定义和实现；Shell 只负责启动、监管和提供连接引导。

```mermaid
sequenceDiagram
    participant UI as SvelteKit 页面
    participant IPC as Tauri IPC
    participant Shell as Anvil Shell
    participant API as App sidecar HTTP API
    participant Core as anvil-core

    Shell->>API: 启动 sidecar
    API-->>Shell: loopback 地址与 ready 状态
    UI->>IPC: shell_get_runtime_connection()
    IPC-->>UI: 本次运行的 API 地址与会话凭据
    UI->>API: HTTP 请求 / SSE 或 WebSocket 事件
    API->>Core: 执行业务逻辑
    Core-->>API: 结果与进度
    API-->>UI: HTTP 响应 / 流式事件
    UI->>IPC: shell_get_update_status()
    IPC-->>UI: Shell 更新状态
```

### Shell IPC

前端集中封装 Shell API，例如 `shell_get_app_info`、`shell_get_update_status`、`shell_check_for_updates` 和 `shell_get_runtime_status`。另提供窄的 `shell_get_runtime_connection` 引导命令，让当前受信任的 App 页面取得正在运行的 sidecar 地址与本次会话凭据。该命令只提供连接信息，不代理业务方法。

Shell IPC 不接受页面指定的 App ID、程序路径、URL 或任意系统操作。命令按创建该 WebView 时绑定的 App 身份工作；IPC 类型与兼容范围属于 Shell Runtime API。

### App 本机 HTTP API

sidecar 在 loopback 地址（仅 `127.0.0.1` / `::1`）监听动态端口，不绑定公网或局域网网卡。Shell 为每次 App 运行生成短期会话凭据并传给 sidecar；页面通过受限的 Shell IPC 获取连接信息，再使用 Authorization header 调用 API。凭据只存于内存，不写入 URL、日志或持久化配置；App 退出时失效。

业务 API 使用带版本的 schema。短操作使用 HTTP 请求/响应；长操作的进度使用 SSE 或 WebSocket，取消通过明确的 operation ID 接口完成。sidecar 负责业务参数校验、授权与错误映射，前端 typed client 根据 App 自己的 schema 调用它。Shell 不需要理解账户登录、安装或服务端启动等领域方法。

页面资源仍由 Shell 从已校验的本地 App 包加载，不将 sidecar HTTP 服务用作远程页面源。Tauri CSP 只允许连接到当前 Runtime 所需的 loopback API；sidecar 只接受 Anvil App 对应的 Tauri origin，并拒绝不需要的方法和来源。CORS 是浏览器的跨源访问控制，不是客户端身份验证；本机其他进程可以绕过浏览器 CORS，因此每个 API 请求仍需会话凭据验证，不能只依赖 Origin 或 CORS。[Tauri Content Security Policy](https://v2.tauri.app/security/csp/)、[MDN CORS 指南](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/CORS)。

Tauri 官方 sidecar 指南将 localhost、stdin/stdout 和本地 socket 作为可选通信方式，并指出它们各有安全考量；本设计选择本机 HTTP，使桌面页面直接访问 App 业务服务，而 Shell 原生能力继续保留 IPC。[Tauri sidecar 指南](https://v2.tauri.app/learn/sidecar-nodejs/)、[Tauri 前端调用 Rust](https://v2.tauri.app/develop/calling-rust/)。以上连接引导和本机 API 均只是设计，尚未实现。

## 构建与 App 选择

构建参数分开表达目标平台、默认 App ID 和当前 Shell API 版本；发行包内置一份匹配的完整官方 App 版本，支持首次离线启动和回滚：

1. **目标平台**：使用 Cargo 的 target triple（例如 `aarch64-apple-darwin`），由 `cargo --target` 决定；同一发行包内的 sidecar 必须匹配这个 target。
2. **默认 App ID**：例如 `anvil.desktop`，由 `ANVIL_DEFAULT_APP_ID` 指定，默认值为 `anvil.desktop`。它决定发行包首次启动的 App。
3. **Shell API 版本**：由 `ANVIL_SHELL_API_VERSION` 指定，默认值为 `1.0.0`；App manifest 声明兼容范围，构建时必须覆盖当前 Shell API 版本。

`ANVIL_APP_MANIFEST` 可指定目标 App manifest；未指定时按 Cargo target 读取 `apps/desktop/manifests/<target>.json`。`apps/desktop/app-manifest.json` 保留为 schema 示例。Shell 构建脚本会拒绝缺少必需字段、未知 schema 版本、默认 App ID 不匹配、目标 triple 不匹配、无效 API 版本或不兼容范围。页面和 sidecar 入口是相对 App 包根目录的正斜杠路径；manifest 解析阶段拒绝绝对路径和 `..`。

manifest schema v1 使用以下字段，版本号采用 SemVer，`shell_api_compatibility` 使用 SemVer version requirement：

```json
{
  "schema_version": 1,
  "app_id": "anvil.desktop",
  "version": "0.1.0",
  "target": "aarch64-apple-darwin",
  "page_entry": "web/index.html",
  "sidecar_entry": "runtime/anvil-desktop",
  "http_api_version": "1.0.0",
  "shell_api_compatibility": "^1.0.0"
}
```

App ID 是稳定身份，不把 `desktop` / `mobile` 当作 Shell 内部的业务分支。Shell 保持通用；构建流水线按默认 App ID 选择页面资源和平台业务组件。桌面发行包内含对应 OS/架构的 sidecar，移动原生 plugin 则由移动 App 构建目标链接进安装包。

Tauri 会将配置为目录的 `frontendDist` 嵌入应用二进制，不能用它单独替换页面。目标方案由 Shell 提供稳定的启动与恢复入口，从已验证的 App 版本目录加载页面资源；页面代码不从公网 URL 执行。

发行构建先生成完整的默认 App 包目录，根部包含 `manifest.json`，并包含 manifest 指向的页面和 sidecar 文件；Tauri 发行配置通过 `bundle.resources` 将该目录作为安装包资源随 Shell 分发。首次启动再把内置资源复制到用户数据目录中的 App 版本存储。App 页面不编译进 Shell 二进制，也不依赖安装后联网下载。当前仓库仅定义并校验 manifest 与构建参数；资源目录装配、Tauri bundling 和首次复制将在后续清单项实现。

## App 包与更新

Shell 与 App 包是两个更新单元：

- **Shell 更新**：原生宿主、通用 Runtime 或安全策略变化时，更新 Shell 安装包。
- **桌面 App 更新**：静态页面与 sidecar 组成一个带 OS/架构信息、签名和版本号的不可变版本包。App 启动时检查一次，运行期间每 24 小时最多检查一次；Shell 在后台静默下载，不打断当前版本。验证通过后标记为待启用，App 可通过 Shell IPC 展示更新状态。

桌面 App 包目录示意：

```text
应用本地数据目录/
  apps/
    anvil.desktop/
      0.1.0/
        manifest.json
        web/                   # 静态页面资源
        runtime/               # App 专属 sidecar
      active.json              # 当前活动版本
      pending.json             # 已验证、等待下次启动启用的版本
  staging/                     # 下载、解包和验证中的暂存内容
```

manifest 至少包含 schema 版本、稳定 App ID、App 版本、目标平台、页面入口、sidecar 相对路径、App HTTP API 版本、Shell IPC 兼容范围、下载大小、内容摘要和发行者签名。Shell 只信任内置发行者公钥。下载到 staging 后，Shell 校验签名、摘要、大小、平台、API 兼容范围和解包路径，再写入不可变版本目录；拒绝绝对路径、路径穿越、符号链接和超预算文件。激活时通过原子更新 `active.json` 切换版本，不覆盖当前目录。

更新按以下流程生效：

1. **检查**：Shell 使用该 App 已配置的更新源；页面不能临时指定任意 URL。
2. **准备**：后台下载新包至 staging，校验签名、哈希、manifest、平台和解包边界。失败时清理 staging，继续运行旧版本。
3. **待启用**：完整验证后写入 `pending.json`。当前页面与 sidecar 继续运行，不覆盖正在使用的文件。
4. **下次启动**：Shell 确认旧 sidecar 已退出后，选择待启用版本，启动新 sidecar，并加载同一版本的静态页面。sidecar 在 loopback 上开始监听后向 Shell 报告 ready；页面通过 Shell IPC 获取连接信息，完成 API 版本检查后报告页面 ready。
5. **健康确认与回滚**：页面加载、sidecar API ready、App API 兼容性检查和页面 ready 信号必须在 30 秒内完成。任一项失败时，Shell 结束新 sidecar，将活动版本恢复到上一个已知可用版本并重新启动旧版。失败版本保留诊断信息，避免无限重试。
6. **清理**：新版本健康后保留最近一个旧版本作为回滚点；更旧版本按磁盘空间策略回收。

运行中的页面资源与 sidecar 可执行文件均不热替换。下载和验证在后台完成，页面与 Rust 业务组件作为同一 App 版本包，在下一次 Shell 启动时一起切换。离线或更新中断时活动版本保持不变。App manifest 不能自行授予系统权限；新增 Shell 能力必须由受审查的 Shell IPC 提供。

## 安全与平台边界

- 一个运行实例绑定一个稳定 App ID、版本和目标平台；不同 App 不共用一个可任意切换身份的高权限页面上下文。
- Shell IPC 只暴露 App 生命周期和版本管理等通用操作，不暴露 Minecraft 安装、账户、进程或文件操作。
- App 本机 HTTP API 只监听 loopback，采用每次运行的短期凭据、精确的 Tauri origin/CORS allowlist 和受限的 CSP。Origin 与 CORS 不能替代 API 身份验证；不得绑定 `0.0.0.0`、使用通配来源或将凭据写入 URL/日志。
- sidecar 由 Shell 持有生命周期：关闭 App 时先停止接收新操作并请求清理，之后有序退出；超时后由 Shell 结束进程。首版不允许 sidecar 独立于 Shell 留驻。
- 只有官方 App 包进入首期范围，不提供任意第三方插件或 App Store。
- desktop 与 mobile 可以共享 schema 和无界面逻辑，但分别维护 SvelteKit routes、布局、样式和组件。

### 移动端更新限制

移动端 App 仍用 SvelteKit，但默认不依赖桌面式的任意页面包热更新。Apple 审核规则限制下载会引入或改变 App 功能的代码；Google Play 对 WebView 中运行的 JavaScript 有解释执行场景的区别，但仍要求动态加载内容不得违反 Play 政策。因此移动安装包应包含完整可用的页面和功能；文案、内容数据和远程配置可独立更新，页面代码、功能与原生能力更新默认走商店发布。若要启用移动 JS bundle OTA，需针对具体平台分发方式和更新内容先完成合规评估。[Apple App Review Guidelines 2.5.2](https://developer.apple.com/app-store/review/guidelines/)，[Google Play Device and Network Abuse policy](https://support.google.com/googleplay/android-developer/answer/16313518)。

## 实施顺序

逐项交付、验收和当前进度见 [Anvil Shell 执行清单](shell-milestone.md)。按清单顺序逐项推进；本文集中记录架构边界与默认方案，不重复维护执行状态。

## 已确认的默认值与待完成设计

- 构建包内置目标 App 的完整基线版本，供首次启动、离线运行和更新回滚使用。
- 桌面 App 启动时检查更新，运行期间每 24 小时最多检查一次；更新在后台静默下载。
- 页面与 sidecar 作为同一个签名版本包更新，在下次 Shell 启动时生效；页面与 API Runtime 在 30 秒内完成健康握手，否则回退。
- 桌面业务 API 使用 sidecar 提供的本机 HTTP；Shell 原生信息、更新与生命周期能力通过类型化 Tauri IPC 暴露。
- 后续实现前需确定 HTTP API schema、端口与凭据引导、来源校验、长操作进度/取消、sidecar 有序退出，以及各平台 Tauri origin 对应的 CSP/CORS 配置。
- 官方更新源仍需确定 stable/beta 通道、签名密钥轮换与撤销机制，以及下载带宽/磁盘上限和用户可见状态。
- 移动端按 iOS/Android 商店政策划定可 OTA 的页面更新范围；新增功能和 native bridge 变化走商店版本。
