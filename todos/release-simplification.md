# 发布流程简化

## 目标与边界

版本 PR 合并后，在确切 main 提交的 CI 成功后自动进入发布；维护者只审批一次，所选产品并行构建，最终统一汇总。Desktop 和 Runtime 保持独立版本，可单发或联合发布。命令名继续为 `anvil`，Runtime 包含 CLI 功能和桌面服务。

新 Runtime 标签使用 `runtime/v<version>`，旧 `cli/v*` 标签与公开附件保留。版本递增和发布准入同时检查两类标签，禁止旧版本换前缀重发。`runtime-updates` 地址、签名产品标识与密钥保持兼容。

## 执行项

1. [x] Runtime 命名迁移：工作流、产品参数、标签生成、签名索引及文档；覆盖旧标签递增检查。
2. [x] 联合版本准备：单个入口和 PR，Desktop/Runtime 版本分别可选，审查发布说明。
3. [x] 自动发起与统一审批：版本变化检测，等待确切 main CI，通过一次审批后并行调用产品构建；普通提交不发版，保留手动恢复能力。
4. [x] 产物复用与恢复：同一产品/版本/SHA 下复用已成功平台产物，仅重试未完成部分；公开附件不重建不覆盖；汇总各产品和 feed 的实际状态。
5. [ ] 验收与耗时核对：离线边界测试、最近发布运行耗时分析，以及维护者授权后的真实发布验收。

## 当前状态

第 1–4 项已在本地实现。Prepare release 提供可选的 desktop_version、runtime_version 和发布说明；至少填写一个版本，联合准备只创建一个提交、一个 PR 并 dispatch 一次 CI。先校验全部产品，再检查分支和写入文件；已有分支不覆盖，PR/CI 创建失败保留远端分支供恢复。

自动入口 `release.yml` 接收本仓库 main 的 Rust 成功事件，按该提交相对父提交的 Desktop/Runtime 版本递增生成计划。没有变化时跳过；手动入口可指定原 main SHA 重试。准备流程仍只接受 main 手动事件。发布的 SHA、main 归属、最新可信 CI 和审批策略在批准前后校验；批准后验证全部选中产品的签名身份，再原子推送不可移动标签。

`release-approved.yml` 共用一次 Environment 审批与发布并发锁，并行调用两类产品构建，汇总分别显示产品结果。普通无版本提交不进入锁；旧 Desktop 恢复入口使用同一锁，保留历史恢复能力。平台产物以产品、版本、SHA 和 target 命名，保留 30 天；构建前查询可信 Release 运行中未过期且带摘要的 artifact，仅缺少的平台进入矩阵，全部可复用时跳过构建。发布收集新旧产物时验证 artifact ZIP 的 GitHub SHA-256、平面文件集合、CRC 和大小，再复用既有发布校验。缺件、过期或摘要不符停止；下一次重试可重建缺失目标。已公开版本仍只读公开附件恢复 feed。

45 项离线脚本回归通过，涵盖旧标签序列、联合准备、普通提交跳过、独立版本、版本回退、自动事件来源与 SHA 边界、手动原 SHA 重试、缺失密钥、全部签名校验先于标签写入和联合标签原子推送，以及 artifact 来源/版本身份、复用矩阵、文件集合、解压路径、大小、摘要不符与过期缺件边界；完成只读版本预览、Python 与工作流 YAML 语法检查。没有提交、触发远端工作流或发布版本；GitHub 审批/嵌套工作流权限与真实并行发布仍待远端验收。旧版流程记录见 [release-process](release-process.md)。

## 第 5 项：整体检查与耗时分析

本地 45 项脚本测试、Python/YAML 语法、actionlint（表达式、工作流调用与配置；未启用外部 shellcheck/pyflakes）及 diff 检查通过。只读核对远端 anvil-release 仍要求维护者审批、允许自行审批、禁止管理员绕过、仅 main；没有修改仓库设置或触发发布。

### 既有发布耗时

数据来自 GitHub run/jobs API，按首次 attempt 和最终发布重试区分；时间间隔包含调度，不能全视为人工审批或计算时间。所有产品均曾只重跑失败发布 job，成功平台没有重编译；新 artifact 方案主要扩展到全量重跑和指定原 SHA 的新运行。

| 运行 | 总墙钟时间 | 校验结束至审批任务开始 | 首次运行结束 | 首次失败至发布重试开始 | 发布重试耗时 |
| --- | --- | --- | --- | --- | --- |
| [Desktop alpha.2](https://github.com/mivinci/Anvil/actions/runs/40725831569) | 7 小时 50 分 | 37 秒 | 约 27 分钟时失败 | 7 小时 22 分 | 42 秒 |
| [Runtime alpha.2](https://github.com/mivinci/Anvil/actions/runs/40722898015) | 17 分 23 秒 | 48 秒 | 约 15 分 39 秒时失败 | 57 秒 | 46 秒 |
| [Desktop alpha.1](https://github.com/mivinci/Anvil/actions/runs/40303617288) | 1 小时 54 分 | 91 分 13 秒 | 约 1 小时 51 分时失败 | 2 分 42 秒 | 32 秒 |
| [Runtime alpha.1](https://github.com/mivinci/Anvil/actions/runs/40303611658) | 19 分 21 秒 | 65 秒 | 约 8 分 54 秒时失败 | 9 分 55 秒 | 32 秒 |

Desktop alpha.2 的 Intel macOS 构建 job 为 25 分 21 秒，其中安装包构建步骤为 21 分 8 秒；ARM macOS 相比其他目标晚约 10 分钟开始，属于调度延迟，不能归为 Rust 编译。Runtime alpha.2 也出现 ARM macOS 约 10 分钟的延迟。统一并行发版减少产品之间的手动接力，但不保证消除 hosted runner 排队或 Intel 编译耗时。

读取 Desktop alpha.2 首次失败日志确认错误为 `Cannot locate created draft; refusing to upload assets`：创建草稿后按标签/列表查询没有找到它。代码已改为使用创建 API 返回的 Release ID，并按 Release/asset ID 上传、读回和公开；拒绝不匹配的 ID/tag，继续保留公开附件不可覆盖规则。新增 5 项回归覆盖创建响应、错误身份、ID 读回、附件 ID 下载及公开版本只恢复 feed 不重建。未实际创建新草稿验证写 API。

只读查询旧 Runtime artifact 元数据确认 digest/expired 字段存在。另尝试下载一份已有 artifact（12832411573）至临时目录验证摘要与文件集合，下载在 120 秒超时，临时目录已自动清理，未执行二进制；真实 artifact 下载及摘要验证尚未通过，不能据此声称远端复用已验收。

### 尚待远端验收

- [ ] 合入后，联合版本 PR 的机器人 CI、workflow_run 自动计划与确切 SHA。
- [ ] 嵌套 reusable workflow 的 GitHub token 权限和一次 Environment 审批。
- [ ] 首次新版本的多平台构建、签名、按 ID 发布和 feed 读回。
- [ ] 单平台失败后原运行/新运行复用、artifact 过期与不可信来源拒绝。
- [ ] 部分产品成功、公开版本 feed 恢复及统一 summary 的真实结果。

本项保持未全部完成；远端写操作需要后续提交/交付和维护者明确授权的版本准备与发布，不能把离线检查当成真实发版验收。
