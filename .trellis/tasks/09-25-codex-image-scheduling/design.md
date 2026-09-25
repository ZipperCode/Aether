# 设计

## 根因与外部证据

Codex 的文本模型目录不含 `gpt-image-2`，本项目把发现 ID 写入 Key.allowed_models，并据此协调 ProviderModel.is_available；两层模型过滤因此提前拒绝生图。Codex 固定模板已有 openai:image，现有同步/流式执行直达 /backend-api/codex/images/generations 或 edits。

sub2api 的 backend/internal/service/openai_images_direct.go 对 gpt-image-2 使用相同直调路径；codex-proxy 的 src/routes/shared/image-generation.ts 使用文本宿主 Responses + image_generation 工具。二者都不把文本发现列表视为完整生图能力证据。本次保持已有直调协议，不引入可能损失 size/quality 语义的隐式协议回退。

## 方案与边界

- 在 aether-model-fetch 新增单一共享的本地 Codex 生图能力补全函数，管理用 legacy models 增加 gpt-image-2（openai:image、supports_image_generation=true），按 ID 去重。
- 不改 bundled_codex_model_cards、原生 cached_models、版本化 /models 响应或 codex_models 不透明卡片元数据。
- 自动刷新：在原始元数据形成后对 association models 补全；用补全后的管理 ID 执行原有 locked/include/exclude 过滤并持久化；从提供商全部可用端点绑定准确 image endpoint_ids。
- 管理端：所有发现结果（新鲜、版本化旧缓存、preset fallback）都经相同能力补全和准确 Endpoint 绑定，禁止误绑 responses。Key 不支持 openai:image 或提供商无可用 image Endpoint 时不补全。
- 现有自动发现白名单通过一次强制刷新或下一次自动刷新恢复；不在读请求中篡改手工 allowed_models。关闭自动发现的手工白名单需显式允许 gpt-image-2，保留该权限边界。
- 不修改 scheduler/transport 的通用模型准入，不放宽非 Codex、显式 include/exclude、鉴权、额度或健康约束。

## 验证

回归覆盖：文本目录 + image endpoint → 白名单包含 gpt-image-2、模型恢复可用并绑定图片端点；显式排除仍拒绝；管理端缓存投影补齐但原生卡片不变。真实请求覆盖同步和流式 Codex 图片请求进入正常调度并保持原生 URL/参数，全部仅本机上游。

## 文件所有权

发现实现 owner：crates/aether-model-fetch/src 及 apps/aether-gateway/src/model_fetch/runtime.rs、model_fetch/tests.rs。
管理端实现 owner：apps/aether-gateway/src/handlers/admin/provider/query/models/mod.rs 及对应 provider_query 测试。
集成验证 owner：apps/aether-gateway/src/tests/ai_execute/{sync,stream}/image.rs。
主会话负责文档、验证与集成。任务前四个未提交文件保持不动。

## 验证揭示的必要修复

真实本机流式生图请求已通过模型准入与 OAuth 刷新，上游收到正确 `/images/generations` 请求并返回同步 JSON；客户端却得到 HTTP 200 的空 SSE。原有 runtime stub 测试未覆盖进程内实际中继。这是完成用户生图行为必须处理的同链路缺陷，不改变产品范围：由 ImageSchedulingRegression 接管最小 stream relay/bridge 修复，保留原生图片语义，不添加模型名特判；修复后重跑本机真实 HTTP 与既有流式桥接回归。

流式修复采用实际桥接来源标记：帧泵仅在同步 JSON 已转换为客户端 SSE 后设置内部 marker，入口先剥除上游伪造同名头，relay 读取后立即剥除。标记只跳过重复 provider normalizer/rewriter/observer，不代表字节已发送，不得触发丢弃后续 Data 帧的 local bridge flag。原生 Images SSE 由图片 rewriter 原样保留同族事件，原有 Responses→Images 转换维持。
