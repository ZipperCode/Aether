# 远端与源码证据摘要

## 远端运行态

- 目标只读检查：`154.217.246.184`，容器 `aether-app`，镜像 `ghcr.io/zippercode/aether:0.7.29`，revision `3475f21b982692f52cf24a077f3d6778b4a19f5c`。
- 主机约 2 GiB RAM；容器未设置内存、swap、CPU 或 PID 限额。
- 数据库有 6,124 个启用 Key：Antigravity 6,021、Kiro 52、Custom 27、Codex 19、Gemini CLI 5。
- 当前进程观测到 `VmHWM=1,800,732 kB`、cgroup peak 约 1.729 GiB；匿名内存占主体，页面缓存很小，且进程曾有约 0.9-1.2 GiB swap。
- account self-check 以 `selected=200, concurrency=4` 批处理。两个独立窗口都出现批次进行时 RSS 约 1.44-1.49 GiB、批次完成后降至约 0.52-0.83 GiB。
- OAuth worker 日志显示 `KeysByProviderIds` / `KeysByIds` catalog load 超过 10 秒。约 114 秒内 `provider_api_keys` 顺序扫描增加 7 次、读取增加 18,372 行、索引读取增加 1,212 次、更新增加 357 次，app RX 增加约 250 MiB。
- 当前容器现场可稳定复现高内存，但本次实例尚未 OOM；dmesg 仍保留 24 次旧 `aether-gateway` OOM kill。旧容器元数据已删除，不能把每次历史 OOM 精确归到当前镜像。
- 全局 PII 脱敏配置没有数据库覆盖，按默认关闭；本次峰值不能归因于 PII 请求缓存。

## 已证实的源码因果链

- `perform_oauth_token_refresh_once` 一次读取全部 Provider、Endpoint、完整 Key，再按 Provider 分组并逐 Key 刷新。
- provider catalog cache 返回缓存值时会深克隆 `Vec<StoredProviderCatalogKey>`；逐 Key 凭证/额度写入会清空整个 catalog cache，并使在途加载失效。
- account self-check 与 OAuth worker 是两个独立 singleton。前者并发写入时可持续使后者的全量读取失效重试，符合远端 DB/RX/内存时间序列。
- `master` 的上述 OAuth、account self-check 和 catalog cache 路径与部署 revision 相同；该主因尚未在本地分支修复。
- 最新 `upstream/main@d1cb0ebec` 仍在 OAuth 启动时立即执行全量扫描，仍调用 `list_provider_catalog_keys_by_provider_ids`，逐 Key 写入仍清空整个 catalog cache；上游的 malformed/legacy row 隔离提交没有解决该内存链。

## 请求体与候选纠偏

- 生产普通文本候选使用 `LocalCandidatePreselectionPageCursor` 和 dynamic attempt source，页大小 256；候选对象不包含请求体。
- executor 每次只将一个候选构造成 body-bearing attempt；dynamic source 的 drain 会直接清空未物化候选，不为剩余 Key 构造请求体。
- `v0.7.29` 的单个活跃 attempt 仍可同时存在原始 body、provider envelope、report context body，以及取消 Guard 的深克隆。当前 `master` 已通过 `fc620ecbb` 所含 lifecycle 修复移除 Guard 的 body 副本；最新 upstream 有等价修复。
- 最大请求体 `727,371 bytes x 6,021` 与进程 `VmPeak` 接近，只能作为相关性，不能证明 6,021 份请求体同时驻留。回归测试应锁定真实不变量：首个 attempt 只调用一次 body builder，停止/drain 不构造剩余候选。

## 安全边界

- 远端检查未修改文件、配置、容器或进程。
- 任务不得记录或输出 SSH 密码、Provider 密钥、Token、Cookie 或完整认证配置。
