# Minecraft 丰富新闻服务（Cloudflare Workers）

## 决策与状态

2026-10-24，维护者确认后续由独立 TypeScript 服务为 Anvil 提供整理后的 Minecraft 新闻与封面图。计划使用 Hono + Zod，部署在 Cloudflare Workers；通过定时任务同步 Minecraft 官方 RSS，对文章做有限度的图片补充，再向 Anvil 提供更丰富的 Feed。

**状态：计划中，尚未创建服务、Cloudflare 资源或生产端点。** 当前桌面 Runtime 仍直接读取 Minecraft 官方 RSS，并使用本地快照缓存；在服务上线、接口验收并完成 App 迁移前，不把计划写成已交付能力。

## 已知数据源特征

- 官方源：`https://www.minecraft.net/en-us/feeds/community-content/rss`
- 2026-10-24 实际读取到 HTTP 200，响应头为 `application/rss+xml; charset=utf-16`；正文是无 BOM 的 UTF-16LE。不能假定 `Response.text()` 能按上游声明正确解码，应根据响应头和 XML 字节签名解码后再解析。
- Feed 是 RSS 2.0；文章包含标题、摘要和日期，链接以 Atom 命名空间的 `<a10:link href="…"/>` 提供。Feed 本身没有文章海报、`enclosure` 或媒体缩略图。
- 现有 [首页 RSS 排查记录](desktop-client-library.md#首页官方新闻-rss) 记录了直接集成和 UTF-16/Atom 链接兼容问题。上游格式变化时应保留最近一次有效快照，不能让单次同步失败清空 Feed。

## 建议执行项

### Phase 1：确定服务边界

- [ ] 决定服务放在独立仓库还是 Anvil 仓库的独立目录；确认 Worker 名称、生产域名、Cloudflare 账号和预览环境。
- [ ] 创建 TypeScript Worker 项目，采用 Hono 路由和 Zod schema；将服务的锁文件、脚本、Wrangler 配置与 Rust workspace 分开维护。
- [ ] 选择持久化方式：建议用 D1 保存去重后的文章元数据、同步状态和图片元数据；只有决定缓存/自托管图片后再绑定 R2。避免为了单份快照同时引入不需要的存储。

### Phase 2：定时同步并规范化

- [ ] 使用 Cloudflare Cron Trigger 调用 `scheduled()`；初版每小时同步一次，间隔可依据官方 Feed 的更新频率调整。
- [ ] 若上游提供 ETag 或 Last-Modified，则使用条件请求；对 304 不重写数据，对超时、非 2xx、解析失败和空 Feed 记录有用但不含令牌/响应正文的诊断。
- [ ] 用 XML 解析器处理 RSS，不用正则解析 XML；支持 UTF-8、BOM 和已观察到的无 BOM UTF-16LE，并按 XML 命名空间本地名读取 Atom `link href`。
- [ ] 以规范化文章 URL 生成稳定 ID，去重并限制 Feed 字节数、文章数、字段长度、请求时长和重定向次数；校验标题、摘要、日期、分类及 HTTPS 原文链接。
- [ ] 使用 Zod 校验规范化记录和对外响应；只在整批新快照校验成功后发布，任何同步失败都保留最后一次成功数据。

### Phase 3：补充文章封面

- [ ] 对新文章或缺少封面的文章，有限并发请求 Minecraft 官方文章页，优先读取 `og:image`，再尝试 `twitter:image`；只处理 Feed 中已校验的官网文章链接，不允许任意 URL 触发抓取。
- [ ] 校验图片 URL 的 HTTPS、允许的官方/CDN 主机、响应类型、尺寸和文件大小；不把文章正文、脚本或任意第三方图片作为封面。
- [ ] 默认保留图片原始来源与文章原文链接。若要把图片复制到 R2 或通过 Worker 代理，先确认可再分发条件，并设置缓存、格式/大小限制及失败回退；图片缺失不影响文章入库。
- [ ] 封面缺失、提取失败或图片源失效时，保留新闻条目并回退到无图卡片，不阻塞整次同步。

### Phase 4：发布 Feed 并接入 Anvil

- [ ] 提供版本化只读端点：丰富版 RSS（含摘要、分类和 `media:thumbnail`/封面字段）及供 Anvil 选择的结构化 JSON；给条目稳定 GUID 和规范日期，保留官方原文链接与图片来源。
- [ ] 读端点提供 `ETag`、`Last-Modified`、合理的 `Cache-Control` 和最后成功快照；公开只读接口不在桌面客户端内放服务端密钥。任何人工强制同步入口必须单独鉴权。
- [ ] 更新 Anvil Core/Runtime 从自建服务读取并缓存丰富字段（摘要、封面 URL、分类、日期、原文链接），前端按需展示图片并保留文字降级；确认数据契约后再决定 Runtime 消费 RSS 还是 JSON。
- [ ] 迁移时保留现有本地快照与离线显示行为；自建服务不可用时显示缓存，不让首页等待上游 Minecraft 网站。

### Phase 5：验证与上线

- [ ] 用脱敏 RSS fixture 覆盖 UTF-16LE 无 BOM、UTF-8/BOM、Atom 链接、重复文章、损坏 XML、非法 URL、异常大字段和空 Feed。
- [ ] 本地触发 `scheduled()`，验证去重、条件请求、图片抓取上限、最后成功快照回退、D1/R2 绑定和 RSS/JSON 输出。
- [ ] 预览环境通过后再配置生产 Cron、域名和存储；记录 Worker 部署、Cron 触发和上游失败状态，确认缓存头与 Anvil 端的跨平台图片加载。
- [ ] 完成端到端验收后更新 README 与首页 RSS todo，把当前“直读官网”状态改为实际的服务接入状态。

## 实现参考

- [Cloudflare Cron Triggers](https://developers.cloudflare.com/workers/configuration/cron-triggers/)：由 `scheduled()` 处理定时同步，Cron 按 UTC 配置。
- [Hono 验证指南](https://hono.dev/docs/guides/validation)：Hono 可通过 `@hono/zod-validator` 使用 Zod schema。
- [Cloudflare D1](https://developers.cloudflare.com/d1/)：Worker 可通过绑定读写结构化新闻记录。
- [Cloudflare R2 Workers API](https://developers.cloudflare.com/r2/get-started/workers-api/)：仅在选择缓存图片对象时使用。
