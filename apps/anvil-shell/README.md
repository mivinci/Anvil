# Anvil Shell

Anvil Shell 是原生 Tauri 宿主。Rust crate 位于 `src-tauri/`，属于仓库的 Cargo workspace。SvelteKit 页面包由同级的 `apps/anvil-desktop` npm workspace 独立维护，并由 `tauri.conf.json` 加载。

在仓库根目录运行 `npm run desktop:dev` 可启动桌面开发模式。
