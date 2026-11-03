# 实例自定义头像

状态：第 1–5 项展示基础、Core 存储、Runtime 鉴权 API、通用裁剪组件与实例页面接入已本地实现；第 6 项离线 API/浏览器验收已通过，原生 WebView、系统文件选择器和真实游戏忙碌锁待维护者跨平台实测。已提交至 codex/instance-avatars 分支交接，尚未创建 PR。

## 产品与数据边界

实例卡片采用左侧正方形头像、右侧名称/版本/加载器的紧凑布局；详情标题旁使用同一头像，点击更换，设置提供更换与恢复默认。无自定义图或读取失败时，按稳定实例 ID 生成固定配色与方块图案，不使用账户皮肤，不随机改变。

本地 PNG/JPEG/WebP 选图后先正方形裁剪与缩放，确认后才上传。原文件最多 8 MiB、最多 1600 万像素，仅静态图片；Core 对输入及解码预算复核，将结果标准化为 256×256 PNG。文件属于实例元数据，放在实例根目录、game 之外。原子更换失败保留旧图，读取损坏回退默认；清除只移除头像，不影响其他实例内容。读写沿用实例路径与锁的规则，拒绝链接/越界；不使用任意外部 URL。

整合包图标的初始化与导出兼容另行确认，不在首版宣称支持。用户图片不上传远端，不建立在线图库，不支持动画。

## 组件边界

- 通用 Avatar 只负责图片显示、加载失败回退与可访问名称，支持调用方提供 fallback，不了解实例/API。
- InstanceAvatar 组合通用 Avatar 与实例 ID 默认图案，不保存图片或发起写请求。
- 通用图片裁剪组件负责本地选图、正方形裁剪/缩放、预览及确认/取消，输出裁剪结果，不绑定实例接口。实例业务层负责上传、缓存失效、错误和恢复默认。
- Core 拥有验证、标准化、实例文件读写；Runtime 负责鉴权与请求限额，UI 校验不能替代 Core 校验。

## 执行项

1. [x] 规格与展示基础：通用 Avatar、实例默认图案、稳定 ID/加载失败回退。
2. [x] Core：有界解码/静态格式、标准化 PNG、原子替换/清除、路径/锁及失败测试。
3. [x] Runtime：鉴权图片读取/上传/清除、受限 DTO、错误码和资源更新标识。
4. [x] 通用图片选择/裁剪：PNG/JPEG/WebP、大小提示、缩放/移动、正方形输出、取消与对象 URL 清理。
5. [x] UI 接入：卡片/详情/设置共享头像与编辑入口、恢复默认、刷新与双语文案。
6. [ ] 验收：离线服务/API/浏览器，损坏/超限/失败保留、图片刷新、键盘与窄窗；原生选图跨平台实测单独记录。

## 第 1 项验证

通用 Avatar 支持来源变化、图片失败回退、统一裁切与可访问名称；InstanceAvatar 以 ID 生成镜像方块图案和固定色相，名称仅用作可访问文字，不影响身份图案。两个离线测试覆盖 Unicode/不同 ID、固定布局、镜像与中心可见；Svelte 检查 0 错误/0 警告、文案守卫与 diff 检查通过。组件尚未接入现有卡片，实际图片失败/刷新与视觉验收归后续项。无 Core/Runtime 变更或真实文件操作。

## Core 当前边界与验证

新增 ClientInstaller 的 avatar/save_avatar/clear_avatar，文件固定为实例根目录 avatar.png，不更改 instance.json schema 或游戏目录。返回标准 PNG 字节与内容 SHA-256 revision，供后续 API 缓存失效。读取不存在/损坏/非标准尺寸图片返回 None；链接、特殊目标和无效实例身份返回错误，不读取外部文件。清除缺失头像可重复执行。

使用 image 0.25.10，仅启用 PNG/JPEG/WebP，Cargo.lock 固定依赖。输入最多 8 MiB、1600 万像素，单边最多 16000，解码分配限额 128 MiB，沿用 Context 最多两个阻塞任务；先应用方向信息，再中心裁剪并缩放到 256×256 RGBA PNG，剥离原始元数据。页面后续上传已裁好的正方形，直接 API 调用则使用中心裁剪。不支持 APNG/动画 WebP。标准文件读取最多 1 MiB。

读操作只取得设置锁，游戏运行时仍可读取；写入/清除按删除相同顺序取得实例锁与设置锁，运行/安装忙时拒绝修改。目标和目录拒绝符号链接/Windows reparse point，Unix 另拒绝多硬链接目标，规范路径确认仍属于管理根目录；用既有 replace_file 的临时同级文件与原子替换发布。无 UI/HTTP 的隐式写入。

11 项头像测试通过：三种格式、标准化与 revision、方向信息、动画/超限/损坏、失败保留、清除隔离、读写锁、越界/身份/特殊文件，以及 Unix 符号/硬链接；另 4 项实例删除回归通过。Core 全目标 Clippy、rustfmt 和 diff 检查通过；仅使用合成图片与临时实例，不下载真实图片、不操作用户实例。Windows 分支未在本机执行，后续平台验收独立记录。

## 第 3 项实现与验证

- `GET /clients/{name}/avatar`：鉴权读取标准 PNG，返回 `image/png`、内容 SHA-256 `ETag` 和私有 `Cache-Control`；没有头像返回 404 `avatar_not_found`，`If-None-Match` 命中返回 304。
- `PUT /clients/{name}/avatar`：原始 PNG/JPEG/WebP 请求体，路由上限 8 MiB；Core 复核格式/像素并标准化为 256×256 PNG；返回 `{ revision }`。写入用独立单并发门禁，忙时 429。
- `DELETE /clients/{name}/avatar`：清除头像，204；缺失头像重复清除仍 204。
- 错误沿用 Runtime 结构化错误码；实例忙锁、损坏/超限、越界和链接拒绝由 Core 返回 4xx。
- 回环测试覆盖未鉴权 401、无头像 404、非法图片 400、上传/读取/ETag、304、清除与幂等清除；测试使用临时实例和合成 PNG。

## 第 4 项实现与验证

- `components/ui/image-crop-dialog.svelte`：接收已选择的 `File`，输出 512×512 PNG `Blob`，不请求实例/API；文件选择入口留给实例层。
- `image-crop.ts`：校验 8 MiB、1600 万像素和 16000 单边限制；cover 缩放、平移边界、源图裁剪矩形和缩放限制均为纯函数。
- 交互：正方形预览、拖动、滚轮/滑块/按钮缩放、重置、取消/应用；方向键平移、`+/-` 缩放、`0` 重置；窄窗和短窗口自适应。
- 预览使用 `object URL`，换图/卸载/关闭时回收；最终 Canvas 输出 PNG；动画格式由 Core 最终拒绝，UI 不宣称已识别动画。
- 文案已加入中英文 `common.imageCrop`，通过 i18n guard；未接入实例卡片、详情或设置。
- 验证：6 项裁剪几何/文件校验测试通过；Svelte 检查 0 错误/0 警告；桌面生产构建通过。

## 第 5 项实现与验证

- `ClientAvatarState` 为窗口级共享状态，卡片、详情、设置读取同一头像；对象 URL 在替换、删除、实例移除和销毁时回收。
- 首页卡片改为左侧正方形头像、右侧名称/版本/加载器和紧凑启动按钮；详情标题旁使用可点击头像；设置新增头像区，支持更换和恢复默认。
- `ClientAvatarEditor` 接文件选择、裁剪对话框、`PUT` 上传和 `DELETE` 恢复；上传/清除失败保留旧图并显示双语错误，409/429 提示实例忙碌，400 提示图片不可用。
- `runtime.ts` 新增鉴权 blob GET、PUT 和 204 DELETE；请求测试覆盖 ETag、认证头、上传 method/Content-Type、204 与 404。
- 验证：runtime-request 3 项、裁剪 6 项通过；i18n guard、`svelte-check` 0 错误/0 警告、桌面生产构建通过；Runtime 头像回环测试继续通过。
- 尚未在真实 WebView 中检查头像加载、裁剪交互、错误回退和窄窗布局；也未启动真实游戏验证忙碌锁。

## 第 6 项离线验收

- `instance-avatar.browser.mjs` 使用 loopback fixture 和假 Shell IPC，覆盖卡片默认图案、详情上传、刷新后 ETag 读取、设置恢复默认、8 MiB 超限被前端拒绝、409 busy/400 invalid 失败保留旧图、读失败回退默认，以及 620×420 窄窗裁剪弹窗键盘操作和无横向溢出。
- 浏览器验收使用本机 Edge 和临时 Playwright 包；截图位于被忽略的 `apps/desktop/.svelte-kit/instance-avatar-qa/`。
- Runtime 回环 API 测试覆盖鉴权、非法/超限请求、PNG/ETag/304、清除幂等和 8 MiB 路由上限。
- 开发态 Rust 端点改动后必须退出整个 `npm run desktop:dev` 再启动；仅页面热更新会继续使用旧 Runtime。PUT 返回 404/405 时 UI 明确提示重启，并保留旧头像。
- 尚未验证原生 WebView 的文件选择器、macOS/Windows 实际窗口尺寸、真实游戏运行时的忙碌锁；这些由维护者跨平台实测，不以浏览器结果替代。

## 接续入口

第 6 项离线部分已完成：Runtime/API/浏览器、失败保留、超限、读失败回退、键盘与窄窗均已验收；原生 WebView、系统文件选择器和真实游戏忙碌锁由维护者跨平台实测。当前授权仅提交/推送交接分支，没有创建 PR、合并或发布授权。

- Core 实现：crates/anvil-core/src/client/avatar.rs；公开导出见 client.rs；对应测试 client_avatar.rs。头像上传与清除目前遵守实例忙锁，读取只取设置锁。
- 展示组件：components/ui/avatar.svelte 为通用 fallback 容器；components/clients/instance-avatar.svelte 组合稳定图案；client-avatar.svelte 读取窗口级 blob 状态；client-avatar-editor.svelte 负责选图、裁剪和上传。
- Runtime 实现见 serve/client_avatar.rs，沿用 client_settings.rs/client_resources.rs 的鉴权、限额和错误模式；GET 返回 blob + ETag，PUT/DELETE 使用写入门禁。标准 PNG revision 仅作缓存标识，不是修改事务版本。
- 裁剪组件不绑定实例 API；实例层负责请求、错误与缓存失效。更新文案后运行 npm run desktop:i18n-check。
- 测试命令：cargo test -p anvil-core --offline client::avatar -- --test-threads=1（11 项）；同命令筛选 client::removal（4 项）；cargo clippy -p anvil-core --all-targets --offline -- -D warnings；cargo fmt --all -- --check；node --experimental-strip-types --test apps/desktop/tests/instance-avatar.test.ts（2 项）；npm run desktop:check。
- 以上验证已在本机通过；Runtime/UI 已集成，离线浏览器已验收。未做原生 WebView、系统文件选择器、跨平台 Core 特殊文件和真实游戏忙碌锁验收，不把这些写成已通过。

## 剩余事项

功能执行项 1–5 已完成，第 6 项只剩原生跨平台验收：macOS/Windows 系统文件选择器、原生 WebView 裁剪/刷新/失败回退、真实游戏运行时忙锁，以及 Windows reparse point/特殊文件行为。上述完成后再创建 PR、合并和发布；当前分支没有 PR。
