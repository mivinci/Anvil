# 发布指南

发布流程已合入 main，Desktop `0.1.4-alpha.7` 已通过审批、四平台构建和恢复入口完成发布，两个 feed 全部目标读回通过。CLI/Runtime 实际发布与其他失败路径的验收范围见[执行清单](../todos/release-process.md)。

## 版本规则

从 `26.0.0` 开始采用年度主版本，每年一个大版本（2026 年为 `26`，2027 年为 `27`），集中承载较大的产品和设计调整；年内功能更新递增 minor，修复递增 patch。内测沿用 `alpha.N`、`beta.N`、`rc.N`。Desktop 与 CLI/Runtime 独立递增并遵循同一规则，不为年度版本强行制造破坏性改动。长期协作约定见[.agent](../.agent/README.md)。

## 发布前提

GitHub Actions 使用默认只读 token，单个工作流显式申请必要写权限。仓库 Settings → Actions → General 需开启 **Allow GitHub Actions to create and approve pull requests**。GitHub 将创建和批准合为同一设置；准备脚本仅创建 PR，不批准或合并 PR。

Settings → Environments 中创建 `anvil-release`：

- Required reviewers：`mivinci`。
- 关闭 Prevent self-review，允许单人维护时自行批准。
- 关闭 Allow administrators to bypass configured protection rules。
- Deployment branches and tags：Selected branches and tags，仅添加 Branch `main`。

入口在进入 Environment 前检查保护规则；环境缺失、配置不符或 token 无法读取时会停止。不要通过删除门禁恢复发布。

仓库 Actions secrets/variables：

| 用途 | Secret | Variable |
| --- | --- | --- |
| Desktop 与 Shell | `ANVIL_DESKTOP_TEST_UPDATE_SIGNING_KEY` | `ANVIL_DESKTOP_TEST_UPDATE_PUBLIC_KEY` |
| CLI/Runtime | `ANVIL_RUNTIME_SIGNING_KEY` | `ANVIL_RUNTIME_UPDATE_PUBLIC_KEY` |

公钥为 32 字节十六进制值，私钥为对应 Ed25519 seed。现有客户端已经信任这些公钥，不要为迁移流程轮换密钥。私钥只存在于批准后的任务，不输出到日志。macOS 未配置证书时使用 ad-hoc 签名；已接入可选的固定证书 Runtime 签名，配置见[macOS 签名](macos-signing.md)。已配置内测自签名证书 Secrets，远端证书构建尚待验收，没有 Apple 公证。

## 准备版本

1. 在 Actions 选择 **Prepare release**，分支选 `main`。
2. 产品选 `desktop` 或 `cli`，填写不带前缀的版本，例如 `0.1.4-alpha.7`。
3. 审查生成的版本 PR，补齐面向用户的 Release notes，等待版本分支 Rust CI 成功后合并。
4. 等待合并后 main 的 **Rust** CI 成功。发布检查确切 SHA 的最新 main CI，PR 上的绿色结果不能替代它。文档路径过滤导致该提交没有 CI 时，手动在 main 运行 Rust，再等待成功。

版本支持 `X.Y.Z`、`X.Y.Z-alpha.N`、`X.Y.Z-beta.N`、`X.Y.Z-rc.N`，必须高于对应产品当前版本和已有标签。脚本创建 `release/<product>-v<version>` 分支，不覆盖已有同名分支。

Desktop 同步 Tauri 和全部平台 manifest 的 version，不修改 Rust workspace 版本。CLI 同步 Cargo workspace 与锁文件中的本地包版本，不升级第三方依赖。两类产品独立递增；Runtime 代码变化时应同步准备独立 Runtime 版本，不能依靠提高 Desktop 版本替代它。联合发布先准备 CLI/Runtime，再准备 Desktop。HTTP API 兼容版本始终是显式设计变更，不自动递增。

本地可只读检查或预览：

```sh
python3 .github/scripts/release.py check --product desktop
python3 .github/scripts/release.py plan --product desktop --version 0.1.4-alpha.7
python3 .github/scripts/release.py check --product cli
```

## 批准发布

1. 在 Actions 选择 **Desktop release** 或 **CLI release**，分支选 `main`，运行工作流；版本直接读取该提交，无需再次输入。
2. 查看 validate job 的 summary，确认产品、版本、完整 SHA、CI、平台及更新源。
3. 在 **Review deployments** 中批准 `anvil-release`。批准后先核对签名密钥，再创建标签并构建；所有后续 checkout 固定同一 SHA。
4. 等待全部平台和发布任务成功。检查公开 Release 的完整附件，以及 summary 中每个更新源目标的读回结果。

不要手动推送 `desktop/v*` 或 `cli/v*` 作为发布入口。工作流批准后创建标签，已有标签只能指向同一 SHA；绝不移动标签。

构建矩阵为 Linux x64、macOS Intel/Apple Silicon、Windows x64。Desktop 预发布生成两份 DMG、DEB、NSIS 和四份 App 更新包；稳定版额外生成 AppImage/MSI。CLI 生成四份压缩包、四份 Runtime 裸二进制、archive 索引和签名 Runtime 索引。两个产品均提供 SHA256SUMS。

Desktop 只更新 `desktop-updates-stable`、`shell-updates-stable`；CLI 只更新 `runtime-updates`。预发布和稳定版共用这些 feed，客户端依自身版本过滤预发布。

## 失败恢复

| 失败位置 | 操作 |
| --- | --- |
| 准备阶段已有同名分支 | 使用工作流返回的分支检查现有改动，不强推覆盖 |
| PR 创建或 CI dispatch 失败 | 分支已保留，补开 PR 或在该分支手动运行 Rust |
| CI 或审批策略校验失败 | 补齐条件后重新运行，不跳过门禁 |
| 构建或草稿上传失败 | 在原运行选择 Re-run all jobs，保留原 SHA；草稿允许修复 |
| Release 已公开、feed 部分失败 | 重跑原运行；跳过构建，下载公开附件校验后继续同步 feed |
| 公开版本本身有问题 | 修复并准备更高版本，不覆盖旧附件或移动标签 |

产物或临时网络失败时，恢复应重跑原运行，保留原事件 SHA；在 main 已推进后新发起运行会选择新的提交，不能拿新提交复用旧标签。恢复仍检查 CI 和审批条件；若原版本已被更高标签取代，门禁可能拒绝旧运行，不应放宽门禁强行回退。

版本附件必须完整、非空，草稿上传后逐文件下载核对摘要，才可公开。已公开版本不会覆盖或重建；Runtime 恢复使用公开的已签名索引，Desktop/Shell feed 基于公开包生成。同一 feed 的更高版本会保留，同版本不同内容会拒绝覆盖。

feed 多文件无法原子更新。summary 会分别显示版本公开、附件校验及各目标上传/读回结果；中途失败表示发布未全部完成。固定 concurrency group 只保护本流程，维护者手动修改 feed 不受其锁约束。

### 发布脚本本身需要修复时

原运行固定旧提交，重跑不会加载合入 main 的脚本修复。使用 **Recover Desktop release**，选择 main，输入原 Desktop 发布 run ID（alpha.7 为 `40258564717`）。入口核对原运行来自 main 手动发布、审批及全部平台构建成功、原 SHA 的 CI 成功且标签仍指向该 SHA；再经过 `anvil-release` 人工审批。

恢复任务下载原运行的 `desktop-*` artifacts，使用原提交的产品契约和 main 已审查的发布脚本，不重新构建或移动标签。公开版本只恢复 feed。草稿恢复要求原 artifacts 尚未过期（当前保留 7 天）；缺失或附件不完整时停止，不拿其他提交的产物替代。
