# 154 服务器只读证据（2026-09-25）

服务器当前镜像 v0.7.38，revision `25bb23b1b2118ea2f705c268135e12dd54b3737f`。未修改生产数据或配置，未触发生图、搜索或额度刷新。SQL 会话强制 `default_transaction_read_only=on`。

- Codex 模板的五个 Endpoint 均 active：Responses、Compact、Search、Images、Live。
- Search 模型绑定：`gpt-5.5`、`gpt-5.6-luna/sol/terra` 的来源是 migration；`gpt-6-sol` 来源是 manual。缺少 `gpt-6-astra`。
- 最近两天 Search 的 `gpt-6-astra` 请求有 40 次 HTTP 503，provider=unknown；最近六小时没有 Codex Search candidate 行，故失败发生在上游请求之前。
- 账号保存的原生 cards 中，Plus、Pro、ProLite 的 `gpt-6-astra` 均 `supports_search_tool=true`。75 张原生卡都有该字段；没有 `supports_realtime` 或 `supported_api_formats` 字段。
- Compact 仅有四个旧型号 migration 绑定。Live 只有 `gpt-6-sol` manual 绑定；不能据此推出所有模型均支持 Live。
- Images 绑定含三个真正图片型号，也含五个文本型号；`supports_image_generation` 均为 NULL。补全不能把旧迁移产生的所有图片 Endpoint 绑定都当成图片模型能力。
- 10 个账号：Plus 4、Pro 4、ProLite 2。8 个开启自动模型发现，白名单没有任何 GPT Image；另 2 个 Plus 手工模式且无白名单。
- 10:17 `gpt-image-2.5-flare` 测试：4 Pro、2 ProLite、2 Plus 因 `key_model_not_allowed` 跳过；1 Plus 额度耗尽跳过；另 1 Plus 实际上游返回 429 `The usage limit has been reached`。
- 4 Pro 中两个额度快照耗尽、两个未耗尽；因此套餐权限、发现白名单和额度必须独立判定。
- 最近一天 Codex 的成功 usage 均是 Responses；这个统计不能证明 Compact/Live 的传输实现有缺陷，仍需源码与本机协议回归审计。

## 本地基线

开始实施前工作区干净，HEAD=`1b8c974b0`。已有 `b5cf3d4d0` / `94b6c11a3` 仅补全 `gpt-image-2`，此次将替换其固定型号补全机制。
