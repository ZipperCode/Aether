# 设计

## 已复核根因

- `model_fetch/logic.rs` 只选择一个文本模型发现端点，投影仅包含该端点格式；Codex 的新型号因此缺少 Compact、Search 的绑定。官方源码确认原生卡片的 `supports_search_tool` 表示工具发现而非 Alpha Search 权限，不能用其真假决定网页搜索准入。
- 图片补全绑定 `CODEX_OPENAI_IMAGE_DEFAULT_MODEL`，管理员配置的新图片型号未进入自动白名单。旧 migration 还把多个文本模型绑定到 Images，不能仅凭存在图片绑定将文本型号转换为图片型号。
- 管理模型测试把 `allowed_models=[]` 当作无限制，与正式调度不一致；本次统一为 None 表示继承/无限制，显式空列表禁止。
- Free 生图限制有官方文档依据，不能用 Pro/Plus 排序代替能力准入。已有账号模型目录和额度快照继续作为独立事实。

## 数据流

1. 保留原生 Codex cards 和版本化缓存不变。仅在管理/关联投影中补充已经实现的 Endpoint 能力。
2. 成功发现的 Responses 文本模型可关联 active 且 Key 格式允许的 Compact 与原生 Alpha Search；依据固定 Codex Provider 已配置的协议能力，表示可尝试路由而非保证账号上游授权。`supports_search_tool` 的缺失/false 不得错误禁止网页搜索。保留人工停用绑定以及上游真实错误；不猜测套餐专属 Search 限制。
3. Images 从提供商已启用模型配置、准确 Endpoint 绑定和已有图片能力/家族语义生成，不枚举版本。采用配置启用状态而非动态 `is_available`，避免旧白名单导致永久无法恢复。不得将旧迁移的纯文本模型自动标为图片能力。
4. 管理发现、新鲜后台刷新、旧缓存投影共用实现，结果仍执行 existing include/exclude/locked 规则。人工白名单不被读请求改写，批量按 Provider 一次协调可用性。
5. 复用 provider-transport 的纯能力判定：规范化 plan 来源，Free 的 Images 返回独立套餐拒绝原因。管理模型测试和正式候选准入都调用该规则，不绕过账户额度、OAuth、健康或模型限制。不添加没有证据的 Alpha Search 模型能力拒绝。
6. 现有 Search 原生同步 URL/请求体/响应语义保持；以本机真实 HTTP 验证其不是 Responses 替代。Live 继续 OAuth WebRTC 与现有凭据能力边界，不因文本发现无证据地自动绑定所有型号。Responses HTTP/SSE/WS 和 Compact 回归保护。

## 边界与实现约束

- 无数据库 schema 迁移，无生产写入，无新外部依赖，无全文 Key 扫描；模型维护的 compact projection 继续隔离认证材料和完整运行态。
- 正式与管理测试共享准入原因；新原因同步现有前端诊断映射。只记录模型/格式/套餐和内部 ID，不记录凭据或内容。
- 套餐标识未知不能当成 Free；没有上游证据的其他套餐限制不猜测。已知 APIKey 类型或其他 Provider 不受 Codex OAuth 专属限制影响。
- 默认图片型号常量仅用于请求缺省值，不能决定发现/支持范围。未来同协议且管理员已配置的图片型号不再要求代码升级。

## 文件所有权

- Discovery implementer：`crates/aether-model-fetch/**`、`apps/aether-gateway/src/model_fetch/**`、`handlers/admin/provider/query/models/mod.rs`、`tests/control/admin/provider_query.rs`。
- Runtime implementer：provider-transport 能力判定与公共候选准入、`models/model_test.rs` 及其子模块、gateway 相关执行回归、前端既有原因映射。新增共享 helper 的签名立即告知 discovery implementer。
- 主会话：任务/研究/规格文档、集成审阅、统一验证、本地中文提交。共享文件变更先沟通，不并行覆盖。

## 验证与发布

针对真实本机上游 HTTP 覆盖动态图片型号、Free 拒绝/Plus-Pro 可尝试、Search 独立于工具发现标志、旧缓存恢复、手工白名单边界，以及 Compact/Live 的既有协议回归。检验现有额度阻断未被绕过。

此次不部署；后续发布需遵守 exact-SHA CI gate。生产修复部署后需要强刷自动模型发现以恢复现存白名单和绑定，部署不等于真实账号生图验收。
