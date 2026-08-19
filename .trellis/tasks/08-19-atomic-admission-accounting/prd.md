# 原子准入与高并发计数

## Goal

独立治理普通 Key 运行态只读取最近 128 条记录造成的计数截断，以及 eligibility 检查与请求 started 记录之间缺少原子 reservation 导致的并发超限。

## Confirmed Evidence

- 运行态候选计数只读取最近 128 条全局记录，而 Key RPM 和并发上限可高于该窗口。
- eligibility 快照发生在候选持久化/started 之前，并发请求可能同时通过同一上限。

## Requirements

- 设计 scoped、原子的 provider/Key RPM 与并发准入 reservation/release 合同。
- 覆盖成功、失败、取消、超时和进程异常下的释放/过期语义。
- 保持 PostgreSQL、MySQL、SQLite 和内存实现一致，不以扫描更大历史窗口作为最终方案。
- 先完成独立设计和兼容性评审，再实施迁移或数据合同变更。

## Acceptance Criteria

- [ ] 并发请求不能共同越过同一 provider/Key 上限。
- [ ] 计数正确性不依赖固定长度的最近记录窗口。
- [ ] reservation 在成功、失败、取消和超时路径都有确定释放或过期机制。
- [ ] 所有存储实现和核心并发场景有独立验证。

## Out of Scope for Parent Implementation

- 本子任务本轮保持 planning，不修改产品代码、schema 或运行配置。

