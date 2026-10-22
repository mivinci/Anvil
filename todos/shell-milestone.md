# Anvil Shell 执行清单

状态：执行中。按顺序逐项实施；每项完成并通过验收后停下来，再开始下一项。当前下一项是第 3 项。设计决策见 [Anvil Shell 设计草案](shell-design.md)。

## 范围与既定约定

- Shell 是通用 Tauri 宿主，只负责 App 加载、生命周期、更新和自身的原生能力；Shell 不依赖 `anvil-core`，不实现 Minecraft 业务。
- Shell 原生能力通过受限、类型化的 Tauri IPC 暴露；桌面 App 业务请求由页面直连 App sidecar 的 loopback HTTP API。
- 桌面页面与 sidecar 属于同一个签名 App 版本包；更新后台下载，在下次 Shell 启动时激活，页面或 Runtime 未健康时回滚。
- 当前 App 目录为 `apps/shell` 和 `apps/desktop`；Rust crate 名 `anvil-shell`、npm 包名 `@anvil/desktop` 和 App ID `anvil.desktop` 保持不变。
- 当前不创建 `packages/`。只有多个 App 真实共享代码时，才抽取 npm workspace package。
- 首轮只实现桌面默认 App；移动端、第三方 App、App Store 和多个 App 同时运行不在本清单范围内。

## 执行项

### 1. [x] 移除 Shell 的 Core 依赖

**交付：** `apps/shell/src-tauri` 只保留 Tauri 宿主和通用 Shell 能力，移除 `anvil-core` 依赖及任何领域命令。保留 crate 名 `anvil-shell`。App 业务实现留给后续的 App sidecar。

**验收：** `cargo tree -p anvil-shell --offline` 不包含 `anvil-core`；`cargo check --workspace --offline` 通过。README 与架构文档准确描述 Shell 的边界。

### 2. [x] 定义构建参数与 App 包清单

**交付：** 定义目标平台、默认 App ID 和 App Runtime API 兼容范围的构建配置；定义带 schema 版本的 App manifest，描述 App ID、版本、平台、页面入口、sidecar 入口、HTTP API 版本和 Shell API 兼容范围。明确构建产物如何把默认 App 包嵌入 Shell 安装包。

**验收：** Shell 构建会解析并校验示例 manifest；临时配置验证了缺少必需字段、App ID 不匹配和目标平台不符都会以明确诊断失败。`cargo check --workspace --offline` 通过。构建参数使用 Cargo target triple、默认 App ID 和 Shell API 版本，不把 `desktop` 或 `mobile` 写成 Shell 内部的业务分支。

### 3. [ ] 实现本地 App 存储与版本状态

**交付：** 在系统应用数据目录中建立按 App ID 隔离的版本目录和状态文件，支持解析活动版本、待启用版本与上一个健康版本。状态更新使用临时文件加原子替换；包目录发布后不可变。

**验收：** 本地 fixture 覆盖初次无状态、活动版本解析、损坏状态拒绝、待启用状态保留和原子写入失败；路径穿越、符号链接及越界路径不能被当作 App 包内容访问。

### 4. [ ] 实现本地页面加载与恢复入口

**交付：** 将 Shell 自带的页面收敛为启动/恢复入口；通过受限的 Tauri 本地协议从活动 App 版本目录提供页面资源，前端页面不再嵌入 Shell 二进制。协议处理只映射到当前已校验的 App 目录，并安全处理 MIME、缺失文件和资源路径。

**验收：** Shell 能加载本地 fixture App；不存在、损坏或无法启动时显示恢复入口。路径穿越、绝对路径和符号链接不能读取 App 包外文件；生产页面资源不依赖公网 URL。

### 5. [ ] 固化并实现 Shell 原生 IPC

**交付：** 实现 App 身份与版本、Shell/App 更新状态、更新检查请求和 Runtime 状态等窄命令。用 Tauri capabilities 限定可调用的 WebView；不接受前端指定任意 App ID、路径、URL 或系统操作。更新进度通过 Tauri Channel 或明确的状态查询暴露。

**验收：** 桌面页面可获取绑定 App 的身份和状态；未授权 WebView 无法调用 Shell 命令。Shell IPC 中不出现账户、客户端、服务端等业务方法。

### 6. [ ] 实现 sidecar 监管与 HTTP 连接引导

**交付：** 将桌面 App 的 Rust sidecar 纳入 Cargo workspace，并让它自己依赖 `anvil-core`。Shell 负责启动、监管和停止 sidecar。sidecar 只监听 loopback 的动态端口；每次运行使用短期凭据。Shell 通过窄 IPC 将当前 Runtime 地址和凭据交给受信任页面，业务请求由页面直接发往 sidecar。

**验收：** Shell 能启动 sidecar、观察 ready、处理启动失败并在退出时有序停止；页面可用授权请求访问健康端点。非 loopback 监听、无效凭据和非允许来源请求被拒绝；凭据不出现在 URL、日志或持久化文件中。

### 7. [ ] 打通内置桌面 App 的首次启动

**交付：** 构建产物内置一份完整的 `anvil.desktop` 基线包，首次运行时将其安全发布到本地 App 存储，再加载页面和对应 sidecar。前端在 `apps/desktop/src/lib` 提供 Shell IPC 与本机 HTTP 的 typed client，不创建尚无第二消费者的共享 npm 包。

**验收：** 首次离线启动可从安装包基线打开桌面页面并连接 sidecar；页面调用 Shell 原生状态走 Tauri IPC，App 业务调用直达 HTTP API。页面、manifest 与 sidecar 的版本和 API 兼容信息一致。

### 8. [ ] 实现 App 更新下载与验证

**交付：** 按启动时及运行期间每 24 小时最多一次的频率检查已配置更新源；在后台将完整 App 包下载至 staging。复用仓库固定版本的 `q2q-http` 下载、校验和恢复能力。验证发行者签名、摘要、大小、目标平台、manifest、兼容范围和解包路径后，才发布不可变版本并标记待启用。

**验收：** 更新下载不中断当前版本；中断、取消、磁盘不足、签名错误、哈希错误、manifest 不兼容或归档越界时，活动版本不变且 staging 可清理或安全恢复。页面不能自行指定更新 URL 或发行者密钥。

### 9. [ ] 实现下次启动激活、健康确认与回滚

**交付：** Shell 在下次启动选择待启用版本，先确认旧 sidecar 已退出，再启动新 sidecar 和同版本页面。新版本须在 30 秒内通过 sidecar ready、页面加载、API 版本检查和页面 ready 确认；失败时恢复上一个健康版本。

**验收：** 新版本启动成功后成为活动版本并保留一个回滚点；页面加载失败、sidecar 启动/握手失败、兼容性错误或超时都会自动回退，失败版本不会无限重试。旧版本在新版本健康确认前始终可用。

### 10. [ ] 完成桌面发行与平台验收

**交付：** 打通 Windows、macOS、Linux 目标构建中的 Shell、默认 App 包和更新元数据；记录各平台的本地协议来源、CSP/CORS、可执行 sidecar 权限和签名要求。

**验收：** 在每个实际宣称支持的平台完成首次离线启动、App 更新、健康确认和回滚 smoke。未实测的平台只记录为待验证，不标记为已支持。

## 不纳入本轮

移动端 App、第三方 App、多个 App 同时运行、Shell 自身的自动更新、账户与 Minecraft 业务接口、远程页面加载。桌面 sidecar 的产品业务实现按客户端/服务端自己的执行清单推进；本清单只要求 Shell 能托管和连接它。
