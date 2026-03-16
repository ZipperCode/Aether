# Tavily 账号池批量导入设计

**日期：** 2026-03-16  
**状态：** 已评审（用户确认）

## 1. 目标
在管理面板 Tavily 账号池页面新增“批量导入账号”能力，支持 `JSON` 与 `CSV` 两种文件格式，并在导入弹窗中提供示例与说明，降低格式错误率。

## 2. 范围
- 前端：`TavilyPoolList` 增加导入入口与导入弹窗。
- 后端：新增 Tavily 账号批量导入接口。
- 兼容格式：`JSON`、`CSV`。
- 冲突策略：`skip`、`overwrite`、`error`。

不在本次范围：
- Excel (`.xlsx`) 解析。
- 导入异步任务化。
- 跨模块通用导入框架抽象。

## 3. 交互设计
- 在 Tavily 账号池页面头部操作区新增按钮：`批量导入`。
- 点击后打开导入弹窗，包含：
  - 格式选择：`JSON` / `CSV`
  - 示例展示：对应格式最小可用示例（可复制）
  - 说明文案：必填/可选字段说明、冲突策略说明
  - 文件选择：仅允许 `.json,.csv`
  - 导入提交：调用后端导入接口
- 导入结束后展示结果汇总：`total/created/updated/skipped/failed/errors`。

## 4. 数据模型
### 4.1 文件内记录统一结构（逻辑层）
- `email: str`（必填，唯一键）
- `password: str | null`（可选）
- `tokens: list[str]`（可选，默认空数组）
- `notes: str | null`（可选）
- `source: str`（可选，默认 `import`）

### 4.2 JSON 示例
```json
[
  {
    "email": "user1@example.com",
    "password": "plain_password_or_empty",
    "tokens": ["tvly-xxx1", "tvly-xxx2"],
    "notes": "可选备注",
    "source": "import"
  }
]
```

### 4.3 CSV 示例
```csv
email,password,tokens,notes,source
user1@example.com,plain_password_or_empty,"tvly-xxx1|tvly-xxx2",可选备注,import
```

## 5. 后端处理策略
1. 根据 `file_type` 分支解析（`json/csv`）。
2. 解析后统一映射到记录结构并校验字段。
3. 同文件内同邮箱合并（tokens 去重），避免内部重复冲突。
4. 依 `merge_mode` 写入：
   - `skip`：已存在账号跳过更新；默认允许为已存在账号补充新 token。
   - `overwrite`：更新账号可变字段并补充 token。
   - `error`：遇冲突中止并回滚。
5. 返回统计与逐行错误。

## 6. 错误与提示
- 前端预检：文件类型、空文件、基本 JSON 结构检查。
- 后端校验：邮箱格式、字段类型、非法 token、非法 `merge_mode`。
- 错误结构：`[{ row, email, reason }]`，便于用户定位问题。

## 7. 测试策略
- 后端：
  - JSON/CSV 成功导入
  - 三种冲突模式
  - 字段缺失/格式错误/重复 token
- 前端：
  - 弹窗示例与说明渲染
  - 格式切换
  - 文件上传与提交流程
  - 结果展示

## 8. 风险与缓解
- 风险：CSV tokens 分隔符解析歧义。
- 缓解：明确采用 `|` 分隔，并在示例与说明中强调。

- 风险：`error` 模式事务一致性。
- 缓解：后端使用单事务，出现冲突即回滚。
