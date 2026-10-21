# anvil-core tests

测试文件直接平铺在 `tests/`：

- `java.rs`：Java 安装事务、下载恢复、取消、清理及安装记录。
- `extract.rs`：tar/zip 解包、路径、链接、权限和资源边界。
- `storage.rs`：文件锁、JSON 存储边界与禁止覆盖。
- `temurin.rs`：版本输入与精确版本范围解析。
- `server.rs`：服务端安装、缓存、实例发布与失败清理。
- `server_metadata.rs`：Mojang 版本元数据校验与失败边界。

这些测试会被各自源码模块通过 `#[path = "../tests/<name>.rs"]` 引入，以便覆盖私有实现。Core crate 关闭 Cargo 对根目录 `.rs` 文件的自动集成测试发现，避免同一文件被作为独立集成测试再次编译；它们仍由 `cargo test -p anvil-core --locked` 编译和运行。

CLI 的进程级测试位于 `crates/anvil/tests/cli.rs`；Java 供应商验收脚本位于根目录 `scripts/java-smoke.py`。Core 测试仅使用本地 fixture，不下载真实 Java 或 Minecraft 文件；模拟 Java 子进程的用例仅在 Unix 运行。
