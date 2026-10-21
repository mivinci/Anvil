# anvil-core 测试

测试代码统一放在 `tests/`：

- `unit/java.rs`：Java 安装事务、下载恢复、取消、清理及安装记录。
- `unit/extract.rs`：tar/zip 解包、路径、链接、权限和资源边界。
- `unit/storage.rs`：文件锁、JSON 存储边界与禁止覆盖。
- `unit/temurin.rs`：版本输入与精确版本范围解析。
- `unit/server.rs`：服务端安装计划、q2q-http 校验、缓存复用、实例原子发布与失败清理。

这些用例需要访问模块私有实现，因此由对应源码模块的 `#[cfg(test)]` 和 `#[path]` 引入，仍按单元测试编译。`unit/` 子目录不会被 Cargo 当作独立集成测试入口，不需要为测试扩大公共 API。

CLI 的进程级契约位于 `crates/anvil/tests/cli.rs`；真实供应商安装验收脚本位于根目录 `scripts/java-smoke.py`。

```bash
cargo test -p anvil-core --locked
```

安装事务的本地 HTTP 夹具需要允许监听 loopback 端口；模拟 Java 可执行文件的流程测试仅在 Unix 运行。Windows 的真实安装由手动触发的三平台 live-java CI 验收。
