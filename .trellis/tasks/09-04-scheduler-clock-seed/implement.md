# 实施清单

1. 读取任务、调度/路由/Provider Pool 规范及共享思考指南，确认当前调用方和测试基线。
2. 引入 `CandidateSchedulingContext`，沿 gateway scheduler 与 planner state 入口分离真实时间和排序种子。
3. 更新普通预选、分页预选及 passthrough 调用方；分页等待只刷新时间。
4. 将解析后排序从 `current_unix_ms()` 改为批次级 `request_distribution_seed()`，保留现行路由策略与 sticky 合同。
5. 添加时间/seed 错位回归及排序种子检查；不恢复 `max_retries` 行为。
6. 运行 `cargo test -p aether-scheduler-core`、gateway 精确候选测试、`cargo check -p aether-gateway`、`cargo fmt --all --check`、`git diff --check`。
7. 通过独立 Trellis 检查后更新规范并提交产品代码；记录并清理已取消的 NAS 构建产物。
8. 推送精确 release SHA，等待 `Rust CI`；创建正式补丁 tag，等待 `Release Aether` 与 GHCR 镜像完成。
9. 备份线上 `APP_IMAGE` 值，拉取正式 tag 镜像并仅重建 app；验证健康、revision、启动日志和 active-open Key 候选。
10. 验收失败立即恢复旧镜像配置；成功后提交任务证据并完成 Trellis 收口。

## 风险点

- 所有新增 `now_unix_secs` / seed 传递必须使用具名字段，禁止再增加相邻裸时间参数。
- 不在服务器现有脏源码目录构建或执行 Git 清理，服务器不承担 npm/Cargo/Docker source build。
- 不运行会修改源码的 formatter；只执行 `cargo fmt --all --check`。

## 执行证据

- 产品修复提交：`ca29b728ad4b44388b065ccfe4a696e207570b66`；调度规范提交：`20faefe13e097d27f3955ebe08bee6220bd007dd`。
- GitHub `Rust CI` 首次运行 `33849608433` 暴露旧并发测试依赖错误时间种子绕过门禁；契约修正提交为 `3475f21b982692f52cf24a077f3d6778b4a19f5c`。
- 新 SHA 的完整 `Rust CI` 运行 `33851770345` 成功，共 23 个任务，无失败。
- 已发布注解标签 `v0.7.29`，精确指向 `3475f21b982692f52cf24a077f3d6778b4a19f5c`；`Release Aether` 运行 `33853225963` 7/7 成功。
- GitHub Release 已发布 4 个资产：amd64/arm64 tarball、`install.sh` 与 `SHA256SUMS`；NAS 已成功拉取 `ghcr.io/zippercode/aether:0.7.29`。
- 已终止 NAS 构建 PID `2695820`、`2695821`，删除 78MB 临时目录和任务镜像标签；Docker builder prune 回收 5.294GB，构建缓存由 5.38GB 降至 86.06MB。
- 线上已只重建 `aether-app`：镜像 `ghcr.io/zippercode/aether:0.7.29`、revision `3475f21b982692f52cf24a077f3d6778b4a19f5c`、容器 healthy、`/health` 返回 200、启动错误行 0；PostgreSQL/Redis 容器 ID 未变化，服务器源码目录未修改。
- 部署后连续 12 次、约 9 分钟只读采样中，服务始终 healthy；最终出现 4 条其他 Key 的真实执行候选，7 个目标 Key 的执行候选仍为 0。
- 线上所有 open-circuit Key 的 `next_probe` 当前都已到期，缺少“探测时间未到”的自然样本；未修改生产 Key/数据库造样本。该窗口由 `scheduling_context_keeps_runtime_windows_on_real_time` 精确回归与完整 CI 证明。
