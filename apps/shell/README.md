# Anvil Shell

Anvil Shell 是原生 Tauri 宿主。Rust crate 位于 `src-tauri/`，属于仓库的 Cargo workspace。SvelteKit 页面包由同级的 `apps/desktop` npm workspace 独立维护，并由 `tauri.conf.json` 加载。

在仓库根目录运行 `npm run desktop:dev` 可启动桌面开发模式。

## 构建参数与 App manifest

Shell 构建使用 Cargo 的 target triple 作为目标平台。`ANVIL_DEFAULT_APP_ID` 默认为 `anvil.desktop`，`ANVIL_SHELL_API_VERSION` 默认为 `1.0.0`；`ANVIL_APP_MANIFEST` 可指定与目标平台匹配的 manifest，默认按 target 读取 `apps/desktop/manifests/<target>.json`。manifest 的页面和 sidecar 入口相对于 App 包根目录；`apps/desktop/app-manifest.json` 是 schema 示例。

本地默认构建目标为 `aarch64-apple-darwin`。交叉构建时，构建脚本按 Cargo target 自动选择对应 manifest；也可以显式指定：

```sh
ANVIL_APP_MANIFEST=../../desktop/manifests/x86_64-pc-windows-msvc.json \
  cargo check --manifest-path apps/shell/src-tauri/Cargo.toml \
  --target x86_64-pc-windows-msvc
```

Shell 构建脚本会校验 manifest schema、App ID、目标 triple、API 版本和包内相对入口。发行时，Tauri `bundle.resources` 将完整默认 App 包作为安装包资源分发；资源装配和首次安装将在后续 Shell 执行项中完成。
