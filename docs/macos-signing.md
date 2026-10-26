# macOS Runtime 证书签名

当前已接入可选的固定证书签名；维护者本机已创建 `Anvil Development` 自签名身份，仓库的两个签名 Secret 已配置。原生钥匙串跨构建授权与远端发布构建仍待验收。未配置时继续使用既有 ad-hoc 签名；配置不完整、导入失败、签名或验证失败则终止构建，不静默降级。签名身份不替代现有 Ed25519 更新包验签，也不代表已经完成 Apple 公证。

## 证书与 GitHub 配置

开发内测可使用长期保留的自签名 Code Signing 证书；正式发行使用 Developer ID Application。两种证书应分开管理，切换身份可能需要用户再次授权钥匙串。不要每次构建生成新证书。

可运行 `python3 scripts/setup-macos-signing.py setup` 创建固定开发证书并导入当前用户的默认钥匙串；已有配置时保留身份，不重新生成。私有备份位于 `~/Library/Application Support/Anvil Development Signing/`（目录 700，文件 600），含加密 P12、其密码文件及公开证书；临时未加密私钥在导出后删除。这个目录应按私钥备份保护，不要删除后重新生成证书。也可自行使用 macOS「钥匙串访问」的证书助理创建 Code Signing 证书，保留证书及私钥，导出为带密码的 `.p12`。不要将证书私钥、P12 或密码提交仓库或发到聊天。

在 GitHub 仓库 Settings → Secrets and variables → Actions 配置：

- `ANVIL_MACOS_CERTIFICATE_P12`：P12 文件内容的 Base64，单行。
- `ANVIL_MACOS_CERTIFICATE_PASSWORD`：P12 导出密码。

本仓库的开发证书可通过 `python3 scripts/setup-macos-signing.py upload` 上传（需要本机 gh 仓库权限，不触发发布）。也可在本机通过 `gh secret set ANVIL_MACOS_CERTIFICATE_P12` 和 `gh secret set ANVIL_MACOS_CERTIFICATE_PASSWORD` 的交互输入配置，避免把密码写进命令行历史。P12 可用 `base64 -i /path/to/certificate.p12 | tr -d '\n' | gh secret set ANVIL_MACOS_CERTIFICATE_P12` 直接传入，不输出到日志。仓库 Secrets 仅供原有人工批准后的发布 workflow 使用，不在 PR CI 中导入。

Desktop 与 CLI 发布入口显式传递这两个 Secret。macOS 构建将证书导入独立临时钥匙串，保留原有搜索列表并临时追加该钥匙串；原始 P12 导入后立即删除。`always()` 清理步骤恢复搜索列表并删除临时钥匙串；强制终止/runner 丢失仍依赖 GitHub 托管临时 runner 的销毁。未改变默认钥匙串。

## 本机开发

使用 setup 脚本创建后，打包会自动读取本机 `identity.json`，无需每次 export；CI 禁用本机配置自动读取。自行管理证书时，将证书及私钥安装在本机钥匙串，并显式设置指纹：

```sh
security find-identity -p codesigning
export ANVIL_MACOS_SIGNING_IDENTITY='<证书的 40 位 SHA-1 指纹>'
npm run desktop:dev
```

如果使用独立钥匙串，另设置 `ANVIL_MACOS_SIGNING_KEYCHAIN` 为其绝对路径，并由维护者自行解锁。脚本不创建、安装证书或修改本机钥匙串授权。不要把身份设为 `-` 或证书显示名，避免退回 ad-hoc 或选中重名证书。

`desktop:package` 在复制到 baseline 后对 Runtime 签名，固定 identifier 为 `app.q2q.anvil.runtime`，验证证书与 identifier；不修改 Cargo 原始产物。CLI 发布对待分发的 Runtime 签名，再打包和计算摘要。Desktop CI 将同一证书传给 Tauri，避免外层打包重签时换回 ad-hoc，并校验安装包 App 内与独立 App 更新包 baseline 内的 Runtime 身份。开发证书签名不请求时间戳；正式 Developer ID 的时间戳、公证与发布策略需另行完善。

首次从 ad-hoc 切换或系统钥匙串锁定时仍可能出现授权框。需要维护者用同一证书连续构建两版，在首次「始终允许」后验证重编译、重启、更新均不会重复提示；已通过 15 项隔离脚本/发布回归，以及本机两份不同程序的签名身份一致性、CI 临时钥匙串导入/签名/验证/清理实测；开发 baseline Runtime 已原子更新签名。尚未触发远端发布 CI，未读取真实账户凭据，跨构建免重复授权仍待维护者实测。
