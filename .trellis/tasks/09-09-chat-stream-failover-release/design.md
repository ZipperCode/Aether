# 设计与证据边界

动态候选源 → candidate loop → execution runtime 响应头/首段分类 → Response 返回 → body finalizer。

StreamCommitPolicy 对同格式 Chat SSE 通常选择 ResponseHeaders。普通非 2xx 走既有 retry；交付后识别的 provider error 走 Terminal，无法返回候选循环。归档头被重建，不能代表原始线头。

先用现有 fixture 还原前位 HTTP 400、后位 HTTP 200/SSE provider error、第三位成功的链。证实失败后，在拥有提交边界的共享位置复用首段分类及既有 failover，使首段错误在 response handoff 前处理。保持原始字节、未知字段及既有分块解析机制；有效事件后的错误不再换 provider。

不改配置语义或总次数，不新增调度器。模拟覆盖三 provider 真正执行、sticky retry、分块错误、正常透传、输出后终止，遵守 stream-attempt-lifecycle 单终态合同。

Root 独占任务/spec/Git/发布；代理不提交。发布前核对最新应用 tag，预期 v0.7.32 但不提前占用，只指向必需 CI 成功的精确 SHA，不移动 tag、不部署。
