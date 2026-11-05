# 执行项总览

本目录区分功能实现、验收和历史依据。勾选的**实现项**表示代码已交付，不代表所有平台/游戏版本都通过实测；未完成的实机或故障场景单独保留。当前架构约定见 [项目架构](../.agent/architecture.md)。

## 待开发

| 主题 | 剩余内容 | 入口 |
| --- | --- | --- |
| 实例头像 | 通用 Avatar、默认图案与 Core 图片存储已本地实现；API/裁剪和页面接入待做 | [实例头像](client-avatars.md) |
| 首次启动欢迎页 | 首次识别、欢迎页与启动入口已本地实现；离线故障/键盘/48 组布局矩阵通过，原生验收待做 | [欢迎页](desktop-welcome.md) |
| 国际化 | zh-CN/en 基础层、壳层、首页、实例管理、在线资源与账户/设置已本地实现；Runtime 错误映射已本地实现；Shell 恢复页已本地实现；双语窄窗布局已离线验收；全量守卫已接入 CI、双语综合回归通过；待最终原生验收 | [桌面国际化](desktop-i18n.md) |
| 新闻服务 | 独立 Worker、RSS 同步、文章封面、缓存与桌面迁移；部署位置和资源尚需决定 | [新闻服务](news-service.md) |
| 资源下载与维护 | 本期功能已实现，手动清理通过 macOS 验收。真实平台端到端验收与完整依赖求解待推进 | [客户端资源](client-resources.md) |
| 账户 UI 扩展 | 菜单已有登录、切换和登出；独立账户管理弹窗未实现，不阻塞现有登录 | [账户](desktop-account-login.md) |
| AI 玩家 | 跟随目标契约及离线测试已完成；下一项固定真实游戏版本与工具链，接入确定性跟随闭环 | [AI 玩家](ai-player.md) |
| 后续产品能力 | 其他加载器、服务端后台托管/跨命令管理、MCP、移动端与其他 App；需先明确各自范围 | [整体方案](architecture.md)、[Shell 设计](shell-design.md) |

CurseForge 已有提供者、只读真实接口联调和凭据接入代码；按当前会话要求暂缓继续对接。隐藏来源/跨平台合并/自动选源仍是讨论方向，没有作为已确认实施项。Apple Developer ID/公证暂不推进，不属于当前发版阻塞。

## 已实现，仍需验收或补录证据

| 主题 | 当前实现 | 剩余验收 |
| --- | --- | --- |
| 安装与启动 | Core/CLI、桌面安装任务、账户与进程状态 | 按 OS/CPU/游戏版本记录安装、退出、取消和凭证库行为；[客户端](client-milestone.md)、[页面](desktop-client-library.md) |
| 模组、资源包与光影 | Fabric、本地资源管理、平台计划/安装、Iris 与在线光影 | 真实下载/游戏渲染、事务恢复和跨平台文件行为；[资源](client-resources.md)、[光影](client-shaders.md) |
| 整合包 | `.mrpack` 导入、导出与窗口级任务；真实“海岛生存”导入已有用户测试记录 | 导出→重新导入、存档开关、异常路径与真实 WebView/游戏；[导入](client-modpacks.md)、[导出](client-modpack-export.md) |
| Shell 与更新 | 生命周期、签名更新、激活回滚、重装提示、能力兼容及 macOS 标题栏 | 各平台恢复/回滚、安装包首次显示、缩放/全屏；[Shell](shell-milestone.md)、[兼容性](shell-compatibility.md) |
| 发布流程 | 联合版本准备、自动计划、统一审批、平台 artifact 复用及 ID 发布 | 按真实运行核对恢复/权限/复用/feed 场景，标签存在不代表全部成功；[发布简化](release-simplification.md) |
| 首页与统计 | 玩家卡、内置动态、官方 RSS、统计与清除 | 原生布局与真实交互；[首页](desktop-home.md) |
| Java 与服务端 | 托管 Java、原版服务端前台生命周期，已有 Linux 真实 smoke | 其他平台/版本与认证范围；[Java](java-management.md)、[服务端](server-milestone.md) |

Microsoft 设备码登录已由维护者确认完成；不再列“等待应用审核/Client ID 联调”为当前阻塞。具体平台尚无记录的验收仍保留。

## 已完成的基础工作与参考文档

- [桌面日志](desktop-logging.md)：基础设施、关键流程与目录入口均已实现。
- [Runtime 合入 CLI](runtime-into-cli.md)：四个代码阶段已完成；自更新验收仍以实际运行记录为准。
- [安装协议依据](minecraft-installation.md)：元数据、文件、账户和平台规则参考，早期检查记录不作为当前能力清单。
- [整体方案](architecture.md)、[Shell 设计](shell-design.md)：设计边界；当前实现摘要优先于早期阶段文字。
- [旧发布方案](release-process.md)：历史记录，操作入口已由统一 Release 流程取代；当前操作见 [发布指南](../docs/releases.md)。

## 维护方式

新增工作放到对应主题，避免复制多份清单。代码合入后更新实现状态；实机结果注明平台、版本及证据，不把 fixture 或编译通过写成游戏验收。保留有价值的阶段验证记录，但明确历史语境，删除过期的分支/提交禁令和环境临时方案。
