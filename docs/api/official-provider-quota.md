# 官方提供商额度查询能力与单位

核对日期：2026-09-15。以下能力仅针对配置的官方域名与当前 API Key；不会跨区域试用凭据，不代表能够枚举账号下所有套餐。证据来自官方文档、官方客户端源码及官方前端资源，并以用户授权的 Key 对官方额度接口进行了只读联调。联调边界见下文，未执行模型推理请求。

## 已接入的查询

| 提供商 | 请求与认证 | 返回含义与单位 | 套餐识别边界 |
| --- | --- | --- | --- |
| DeepSeek | `GET https://api.deepseek.com/user/balance`，Bearer | `balance_infos[]` 按各自 `currency` 保留十进制字符串；`total_balance` 是当前可用余额，赠送和充值余额独立显示，无缩放 | 接口没有套餐名称/档位 |
| Moonshot 国内/国际 | 对应 `api.moonshot.cn` / `api.moonshot.ai` 的 `GET /v1/users/me/balance`，Bearer | 国内 CNY、国际 USD；原值就是货币主单位。使用 `available_balance`；现金可以为负，不能自行求和现金和代金券 | 无套餐档位；两站凭据独立 |
| SiliconFlow 国内 | 官方已于 2026-08-14 停用 `GET https://api.siliconflow.cn/v1/user/info`；刷新直接记录不支持，不再发送 Key | 2026-09-15 实测 HTTP 410、业务码 `20092`，与官方停用公告一致；未找到已公布的替代接口 | 不将停用解释为 Key 失效，不保留旧余额作为有效额度 |
| SiliconFlow 国际 | `GET https://api.siliconflow.com/v1/user/info`，Bearer；当前国际文档仍列出此接口，未使用国际 Key 实测 | `totalBalance`、`balance`、`chargeBalance` 不缩放，校验业务成功字段。USD 来源是国际定价，不是响应自报字段 | `role`、账户状态或管理员标志不是套餐 |
| OpenRouter | `GET https://openrouter.ai/api/v1/key`，Bearer | USD 的 **Key 消费限额**。只信任 `limit_remaining`；`usage` 是累计消费，不能从周期上限中减去它。可选 `free_model_daily_requests` 独立显示每日免费请求次数 | `limit=null` 只表示 Key 没设上限；账户余额未知。免费请求耗尽不扩大为整个账户的调用阻断 |
| 智谱 / Z.ai | 对应 `open.bigmodel.cn` / `api.z.ai` 的 `GET /api/monitor/usage/quota/limit`，`Authorization: <API Key>` | 旧 `TOKENS_LIMIT` 只显示比例；`CREDIT_LIMIT` 为积分；`TIME_LIMIT` 为独立 MCP 用量。5 小时与周窗口不相加 | 只读取实际返回的 `level` 等身份字段，不能按额度大小猜档位 |
| 智谱兼容来源 | 团队 `GET /api/monitor/usage/quota/limit?type=2`；余额 `GET /api/biz/account/query-customer-account-report` | 保留既有官方端点查询为独立来源；无权限、不适用或失败各自报告，不再失败后补零 | 这两项没有足够公开证据保证每把推理 Key 均可查询；成功仅代表当前 Key 返回的范围 |
| Kimi Coding | 对应 `api.kimi.com` / `api.kimi.ai` 的 `GET /coding/v1/usages`，Bearer | 基础 `usage` 是周额度，`limits[]` 是短窗口，默认显示比例。实际返回的 `usages` 可提供更精确的比例及额外月度窗口；钱包与月度限额金额使用不同缩放，见下表 | 仅展示实际返回的 membership/subscription/plan 字段，不按配额猜会员档位 |
| MiniMax 账户余额 | 对应 `api.minimaxi.com` / `api.minimax.io` 的 `GET /account/query_balance`，Bearer；与套餐接口独立查询，不按 Key 前缀排他选择 | 金额字符串按原值保留；没有已确认币种字段时明确显示币种未知，不将 credit_balance 当作套餐积分 | 无套餐档位字段；未知币种不能用于货币阈值判断 |
| MiniMax Token Plan | 对应官方区域的 `GET /v1/token_plan/remains`，Bearer | 独立额度池、短窗口/周窗口、百分比、加成、是否包含及不限额状态。含义变化的 `usage_count` 需要显式百分比辅助解析 | `model_remains` 是额度池列表，不是套餐档位列表；没有 Plus/Max/Ultra 字段时显示未知 |

2026-09-23 修复：官方查询执行层与端点校验复用同一域名规则，保持 HTTPS、443、无 URL 凭据的限制。MiniMax 同区域分别查询余额与套餐；仅套餐接口业务码 `2062` 视为套餐不适用，`2049` 仍为查询认证拒绝，均不推断模型调用状态。OpenRouter 免费次数仅接受非负整数，缺失或非法值保持未知；刷新失败保留该来源的历史数值并标记过期，成功响应不再返回该字段时清除旧来源。

## 容易算错的字段

| 字段 | 正确规则 |
| --- | --- |
| Kimi `boosterWallet.balance.amount/amountLeft` | 原值除以 `100000000` 得到货币主单位；例如 `10000000000` 是 `100`，不是一亿或一百万 |
| Kimi `monthlyChargeLimit.priceInCents` / `monthlyUsed.priceInCents` | 除以 `100`，币种来自对应 money 对象；不能套用钱包倍率 |
| Kimi `usages.*.used_ratio` | 已用比例，`0.239883` 表示已用 `23.9883%`；有效的 `limit_5h` / `limit_7d` 覆盖旧字段的整数比例，避免重复窗口。实际返回的 `limit_month_total` / `limit_month_code` 分别展示月度总量 / Code 比例，缺失时不生成；不把这两个新窗口的耗尽扩大为整个 Key 的阻断 |
| 智谱/Z.ai `percentage` | 是已使用百分比；转换成比例时除以 `100`。旧 `TOKENS_LIMIT` 原始计数不能直接宣称是真实 Token |
| 智谱 `CREDIT_LIMIT.currentValue/usage` | 分别为已用积分/积分额度；同单位下计算剩余，保持十进制精度，不做积分到 Token/金额换算 |
| OpenRouter `limit_remaining` | 服务端已计算的 Key 剩余消费限额。缺失时保持未知；不重建周期、BYOK 和累计消费计算 |
| MiniMax `*_usage_count` | 旧版含义为剩余，新版可能为已用；比较两种解释与 `*_remaining_percent`，仅在符合官方容差时显示绝对值。缺少百分比沿用官方旧版约定；明显非法输入保持未知 |
| MiniMax `weekly_boost_permille` | 展示倍率除以 `1000`；文本可超过 `100%`，只有进度条几何长度限制到 `100%`，不把展示倍率反乘原始计数 |
| MiniMax 两窗口 total=0 且 status=3 | 官方 UI 将此组合解释为套餐不含该额度池，不能显示无限额度；单窗口状态必须结合是否包含判断 |
| MiniMax 时间字段 | 上游时间戳和剩余时长为毫秒；写入统一快照的 reset_at 使用 Unix 秒 |
| 缺失、非法、空或失败值 | 保持未知，不能通过空值转数字生成 `0`、`0%` 或无限额度 |

## 快照与调度边界

- `status_snapshot.quota.sources[]` 只保存来源身份、套餐元数据、查询结果和刷新时间。额度仍唯一存放在顶层 `balances[]` / `windows[]`，通过 `source_id` 关联。
- 查询状态为 `not_queried`、`ok`、`unsupported`、`permission_denied`、`not_applicable`、`error`。它们不代表 OAuth 或推理调用状态。
- 同一 Key 的任意来源成功即计为该 Key 刷新成功；失败来源独立标记，历史值只能作为 stale 数据展示。已成功的兄弟来源不被覆盖。
- 已确认的“不适用 / 不支持”是能力结论，不制造部分失败或错误退避；相应来源不再展示旧数值。网络、权限和解析失败仍保留上次成功值与时间。
- 缺少 sources 的历史快照仍可显示；历史 OpenRouter 反推金额、智谱错误补零或未能证明套餐归属的旧官方订阅快照，不作为新的自动阻断依据，等待正常刷新替换。
- 已确认的纯货币来源继续采用各币种独立的 `1.0` 调度阈值，不相加、不做汇率换算。当前调度识别 CNY/USD，未知币种只展示；临界十进制字符串保留精确比较，不因浮点舍入把 `1.000000000000000001` 阻断。套餐、钱包与金额混合时，未获证实的抵扣关系保持未知。
- 一个套餐内部多个调用窗口共同限制用量；独立套餐不能以任一耗尽推断整个 Key 耗尽。MCP/其他模型用量不会扩散为账号级模型限制。
- Kimi 额外用量及 MiniMax 已购积分可能补充基础套餐；信息不足时不能据套餐耗尽阻断整个 Key。MiniMax 首轮仅作观察。
- 额度刷新不清除 `status_snapshot.scheduling` 中的人工恢复阻断，也不修改人工启停、OAuth、模型探测和其他兄弟状态。
- 官方订阅不再把快照耗尽复制为号池的持久账号耗尽状态；调度直接检查来源、有效性和重置时间。正常刷新只清理历史额度查询产生的号池阻断标记，人工阻断仍受原有强校验保护。
- 额度区只展示额度数值与查询结果，不展示模型调用可用性；不支持、不适用、未查询不使用 Key 失效语义，不把额度成功等同于模型可调用。
- 保留真实采样/最后成功时间，不展示数据过期或历史快照文字提示；真实 freshness 数据与调度判断不变，不引入新的调度 TTL。
- 号池桌面与移动端的额度区提供单 Key 刷新图标，复用现有额度接口、冷却规则与快照合并；加载时禁用重复刷新，切换提供商后丢弃旧响应，不触发全页重载。
- 历史 `custom` 提供商仅在启用端点精确匹配唯一官方类型时提示转换，通过现有编辑表单由管理员确认。不会自动迁移生产配置。本地月度预算明确标记“本地配置”。

## 只读联调范围

2026-09-15 使用用户授权的 9 把 Key 进行了 11 次官方额度 GET 查询，并将脱敏响应回放到当前解析器、来源合并逻辑和前端共用展示函数。未把 Key 写入项目、未调用模型、未将凭据跨区域重试；这是接口与数据处理验证，不是完整网关部署集成测试。

- DeepSeek、OpenRouter、智谱、Z.ai 和两把 Kimi Key 返回了可解析数据。智谱个人套餐与余额结果同时保留；团队接口返回空对象，维持未知/查询失败，不猜测团队订阅状态。
- Kimi 两把 Key 均返回精确比例及两个额外月度窗口，未返回套餐档位或额外钱包。钱包 `10^8` 与月度金额 `10^2` 的倍率仍以官方源码及样例回放验证，不能宣称这两把 Key 覆盖了钱包联调。
- SiliconFlow 国内返回接口已停用；按官方公告撤除国内查询请求，国际能力保留并明确未经实测。
- Moonshot 国内返回 HTTP 401；MiniMax 国内返回 HTTP 200 但业务码 `2049` 表示认证拒绝。两者归入无查询权限，不生成零额度、不标记调用失效。尚未验证其他区域，也未获取这把 MiniMax Key 的实际额度池数据。

金额、账户标识和原始响应未收录到能力文档；样例与真实响应分别标明验证来源。

2026-09-23 排查补充：在用户授权实例内存中使用代表 Key 查询，确认公共 origin 策略曾在出站前阻断官方请求；确认硅基流动国内额度接口返回 410 但同一 Key 可以生成；确认 MiniMax 四个旧格式 Key 套餐返回 2062 而同区域余额查询成功。模型调用另有 401、402、1113 等真实失败，不能从查询失败统一推断。修复验证采用本地 Rust 查询/持久化回归与实际 Vue 组件交互，不部署线上，不把原始响应或凭据写入仓库。

## 未作为普通推理 Key 能力接入的接口

| 平台 | 已确认边界 |
| --- | --- |
| OpenRouter 账户 credits | 当前 `/api/v1/credits` 要求 Management Key，不是普通 Key 查询失败后的兜底 |
| 阿里云百炼 | Coding Plan 文档指向控制台；未确认普通套餐 Key 的额度查询契约。BSS `QueryAccountBalance` 需要 RAM 授权与云凭据 |
| 火山方舟 | `GetPersonalPlan`、`GetAFPUsage`、`GetUsageDetails` 已公开，但要求云 Access Key 签名；AFP 也不是 Token |
| 腾讯云 | 通用与 Hy Token Plan 可以独立并存，新版积分与旧 Coding 调用次数不同；本轮未确认套餐 Key 查询接口。Billing 金额单位“分”不能套用其他提供商 |
| 百度千帆 | Token Plan 有 Token 制与积分制，文档仅确认控制台查看；本轮未确认套餐 Key 查询契约 |
| Fireworks | Key 可查询账户/月消费限额，但需要账户选择与权限处理，属于消费上限而非余额或 Coding Plan，本轮未新增预置 |
| Together | Billing Usage 为按组织开通、可能延迟的消费报告，不是余额或剩余套餐额度 |
| xAI | 普通推理 Key 信息接口不返回余额；账单 Management API 使用独立管理凭据 |
| Groq / Mistral | 本轮未找到已确认的官方余额/剩余套餐接口，不等同于宣称永久不支持 |

## 官方证据

- [DeepSeek balance](https://api-docs.deepseek.com/api/get-user-balance)
- [Moonshot CN balance](https://platform.moonshot.cn/docs/api/balance.md)、[Global balance](https://platform.moonshot.ai/docs/api/balance.md)
- [SiliconFlow 国内停用公告（2026-08-11 发布，2026-08-14 停用）](https://docs.siliconflow.cn/docs/release-notes/overview)、[Global user info](https://docs.siliconflow.com/en/api-reference/userinfo/get-user-info.md)、[Global pricing](https://www.siliconflow.com/pricing)。旧版 OpenAPI 的国内 user/info 定义已被停用公告取代。
- [OpenRouter current key](https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key.md)、[Account credits](https://openrouter.ai/docs/api/api-reference/credits/get-remaining-credits.md)
- [Z.ai official quota query](https://github.com/zai-org/zai-coding-plugins/blob/main/plugins/glm-plan-usage/skills/usage-query-skill/scripts/query-usage.mjs)、[BigModel quota revision](https://docs.bigmodel.cn/cn/coding-plan/notice/usage-revision)、[Team plan](https://docs.bigmodel.cn/cn/coding-plan/team)
- [Kimi official usage parser](https://github.com/MoonshotAI/kimi-code/blob/main/packages/oauth/src/managed-usage.ts)
- [MiniMax endpoints](https://github.com/MiniMax-AI/cli/blob/main/src/client/endpoints.ts)、[Response types](https://github.com/MiniMax-AI/cli/blob/main/src/types/api.ts)、[Quota interpretation](https://github.com/MiniMax-AI/cli/blob/main/src/utils/quota.ts)、[Quota rendering](https://github.com/MiniMax-AI/cli/blob/main/src/output/quota-table.ts)、[Token Plan FAQ](https://platform.minimaxi.com/docs/token-plan/faq)
- [Alibaba Coding FAQ](https://help.aliyun.com/zh/model-studio/coding-plan-faq)、[BSS balance](https://help.aliyun.com/zh/user-center/developer-reference/api-bssopenapi-2017-12-14-queryaccountbalance)
- [Volcengine plan](https://www.volcengine.com/docs/82379/2546382)、[AFP usage](https://www.volcengine.com/docs/82379/2479847)
- [Tencent Token Plan](https://cloud.tencent.com/document/product/1823/130060)、[Usage entry](https://cloud.tencent.com/document/product/1823/130119)
- [Baidu Token Plan](https://cloud.baidu.com/doc/qianfan/s/Dmrabu8b6)
- [Fireworks account quotas](https://docs.fireworks.ai/guides/quotas_usage/account-quotas.md)、[Together billing usage](https://docs.together.ai/reference/billing-usage.md)、[xAI management authentication](https://docs.x.ai/developers/management-api-guide.md)

这些链接是本次实现的证据记录，不能替代对应账号的实际权限验证。后续上游字段变化应在对应解析器内修正，不通过通用倍率或 UI 猜测补偿。
