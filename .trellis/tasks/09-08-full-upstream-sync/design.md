# 完整上游同步设计

## Git 与所有权

- 主工作区：`D:/Project/GitHub/Aether`，起点 `b0ad8ff7f7bdb1888abac8c06f47f61e76761431`。
- 隔离工作区：`C:/Users/Zipper/AppData/Local/Temp/aether-full-upstream-20260908`，分支 `codex/sync-upstream-20260908`。
- 在隔离分支执行 `git merge --no-commit --no-ff c7e403b410139d12a6189dda9c2bdf0c7c80782e`，逐文件解决冲突并形成双亲合并提交。
- 唯一产品写入所有者为 Trellis implement integrator；Root 负责工件、派发、验证收据、最终提交及本地合回。实现结束后再由独占 Trellis check 检查与自修复，禁止并发改同一写集。

## 合并规则

1. 完整采用上游明确新增的业务能力和已获用户确认的行为变化，不维持被淘汰数据库后端。
2. 对双方修改的文件先比对共同祖先及两侧改动，再将本地独有能力适配到上游结构；禁止批量 ours/theirs 代替语义解决。
3. 重点保护 auth-maintenance-memory、model-association-endpoint、balance-scheduling、runtime-quota-block、Codex HTTP/逻辑身份与能力测试相关契约和调用链。
4. 已获批准的新语义优先于旧规范中相冲突的陈述；更新受影响规范时说明替代原因，例如 PostgreSQL-only、原始 Header 保存和候选独立首字节预算。
5. fork 发布命名、既有环境配置和部署对象不因合并而变为上游实例；不执行任何部署动作。

## 验证边界

- 先检查冲突残留、Git 差异、Cargo metadata 与受影响配置解析，再检查类型及受冲突影响的 crate。
- 当前是跨核心执行、数据、协议和公共契约的整分支合并，允许相关 Rust workspace 编译/检查与现有目标回归；不盲目全量反复测试。
- 不运行 UI 浏览器/视觉或全量前端测试。类型检查和提取出的逻辑回归按项目命令执行，lint 不使用带自动修改的 `npm run lint` 作为只读命令。
- 数据库测试只能使用任务自有临时资源或 memory；不读取 .env 的现有数据库凭据进行测试，不启动已有 Compose stack。
- 如果宿主 Windows 不支持依赖，先记录具体失败，再使用已有受支持的隔离工具链；不修改全局环境或现有服务来绕开失败。

## 收尾

检查通过后 Root 提交合并结果，确认主工作区仍在原始干净起点后以 fast-forward 接入。若主工作区被用户推进或出现重叠改动，停止合回并报告，不覆盖。归档当前任务并记录 journal，最终清理已合并分支/worktree；全程不推送。
