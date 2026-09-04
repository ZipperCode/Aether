# 调度时间与排序种子分离设计

## 边界

变更只落在 gateway 候选预选、运行态筛选和解析后排序。HTTP、数据库、前端及公开 JSON 合同不变；`aether-scheduler-core` 继续消费明确传入的时间与排序上下文。

## 内部合同

- 新增 crate 内部具名值 `CandidateSchedulingContext`，字段为 `now_unix_secs` 与 `load_balance_seed`。调用方使用字段名构造，避免位置相同的 `u64` 再次错位。
- 候选运行态快照、熔断、RPM、并发、OAuth 和额度判断只读取 `now_unix_secs`。
- `rank_scheduler_candidates` 只读取 `load_balance_seed`。
- 简单预选和分页游标分别在创建时生成一个 `request_distribution_seed()`；分页加载时用 `current_unix_secs()` 创建当前上下文，保留原 seed。
- Planner 的并发等待循环只替换上下文中的 `now_unix_secs`。
- 解析后最终排序在 port 创建时生成一次 `request_distribution_seed()`，不再以 `current_unix_ms()` 直接排序。

## 兼容与取舍

- 不为两个独立排序阶段扩展公共 DTO；每个排序批次保持一个稳定、请求唯一的种子即可消除同毫秒碰撞。
- `max_retries` 保留存储、导入导出和诊断兼容，不参与执行；唯一同 Key 重试预算仍为 `sticky_key_attempts`。
- 不增加执行前第二套熔断器或半开 lease；先修复已证实的时间混用。若部署后仍出现探测惊群，再以独立任务设计原子 half-open admission。

## 部署与回滚

- 推送本地已验证提交并锁定 release SHA；先等待该 SHA 的 `Rust CI` 成功，再创建并推送下一个正式补丁 tag。
- 等待 tag 对应的 `Release Aether` 成功并核验 GHCR 镜像后，服务器只执行镜像拉取，不运行 Docker build、npm 或 Cargo。
- 在 `/home/zipper/Aether` 仅将现有 `APP_IMAGE` 更新到正式 tag，保留原值备份；Compose 使用 `--no-deps --force-recreate app`，不重建数据库和 Redis。
- 健康、镜像 revision 或 active-open 验收失败时恢复原 `APP_IMAGE` 并重建 app。
