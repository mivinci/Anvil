# anvil-core tests

测试文件直接平铺在 `tests/`：

- `java.rs`：Java 安装事务、下载恢复、取消、清理及安装记录。
- `extract.rs`：tar/zip 解包、路径、链接、权限和资源边界。
- `storage.rs`：文件锁、JSON 存储边界与禁止覆盖。
- `temurin.rs`：版本输入与精确版本范围解析。
- `server.rs`：服务端安装、缓存、实例发布与失败清理。
- `account.rs`：账户资料校验、设备码事件、凭证存储替身、令牌轮换、身份一致性及敏感值脱敏。
- `account_store.rs`：共享账户资料的原子存储、当前选择、移除及令牌隔离。
- `client_removal.rs`：只删除临时目标实例、保留共享缓存与其他实例，运行/设置锁及身份校验；Windows junction/Unix 符号链接拒绝并保留外部目标。不会删除用户实例。
- `resource_transfer.rs`：下载返回路径与目标的规范化比较，覆盖 Windows verbatim 路径、不同文件、缺失文件和目录拒绝；Unix 另验证符号链接拒绝。仅使用临时文件，无真实下载。
- `shader_packs.rs`：旧资源清单兼容、光影 ZIP 结构及路径/链接/大小边界、总引用数、公共导入/移除、跨实例共享与副本隔离、运行锁/外部文件冲突、模组及资源包保留、启动前修复和事务恢复。仅使用内存 ZIP 与临时实例，不安装或启动真实游戏。
- `shader_environment.rs`：受管 Fabric ID/版本/启停、必需依赖与反向不兼容、重复/损坏元数据、嵌套依赖/路径/深度/数量/字节预算、内置库内容去重、别名及双向冲突、实例副本完整性及读取预算；不启动游戏或请求在线平台。
- `shader_install.rs`：Iris 的受限必需依赖计划、缓存提交、实际 ID/依赖冲突、禁用条目、取消、陈旧 revision 及光影引用保留。普通用例仅用临时实例和内存 JAR；指定官方 Iris/Sodium JAR 的验收默认 ignored，文件路径通过 `ANVIL_SHADER_SMOKE_IRIS` / `ANVIL_SHADER_SMOKE_SODIUM` 提供，测试核对固定 SHA-1，不下载或启动游戏。
- `account_microsoft.rs`：本地 HTTP fixture 覆盖 Microsoft 设备码轮询、用户拒绝/取消、Xbox/XSTS 与 Minecraft profile 交换和刷新令牌轮换；不访问真实账户服务。
- `client.rs`：客户端文件缓存、q2q-http 并发/校验/取消、实例原子提交与失败重试，以及结构化启动参数和 natives 会话生命周期。
- `metadata.rs`：Mojang 版本清单、客户端与服务端版本 JSON 校验。
- `client_plan.rs`：官方 1.21.1 平台库/native/日志/assets 计划，以及旧 classifier、安全路径和不支持条件。
- `discovery_cache.rs`：模组全局搜索与项目摘要的缓存键兼容、来源/筛选隔离、磁盘恢复、身份/来源链接/图标 CDN 校验、旧图标字段兼容及条目数上限；仅使用临时目录和离线 DTO，不请求 Modrinth/CurseForge。

`fixtures/1.21.1.json` 和 `fixtures/1.21.1-assets-17.json` 是通过 Mojang 官方版本清单 SHA-1 校验的离线样本，供客户端计划测试使用。这些测试会被各自源码模块通过 `#[path = "../tests/<name>.rs"]` 引入，以便覆盖私有实现。Core crate 关闭 Cargo 对根目录 `.rs` 文件的自动集成测试发现，避免同一文件被作为独立集成测试再次编译；它们仍由 `cargo test -p anvil-core --locked` 编译和运行。

CLI 的进程级测试位于 `crates/anvil/tests/cli.rs`；Java 供应商验收脚本位于根目录 `scripts/java-smoke.py`。Core 测试仅使用本地 fixture，不下载真实 Java 或 Minecraft 文件；模拟 Java 子进程的用例仅在 Unix 运行。
