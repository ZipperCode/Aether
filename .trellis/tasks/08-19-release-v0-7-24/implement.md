# Aether v0.7.24 执行计划

## 前置关卡

- 用户批准更新后的最终规划摘要。
- 只有批准后才运行 `python ./.trellis/scripts/task.py start 08-19-release-v0-7-24`；本规划阶段不得运行。
- 执行代理先读取 `prd.md`、`design.md`、本文件和 `research/release-evidence.md`。

## 严格顺序

### 1. 锁定候选提交并复核现场

```powershell
git status --short --branch
git diff --quiet
git diff --cached --quiet
$releaseSha = (git rev-parse HEAD).Trim()
git log --oneline origin/master..$releaseSha
$remoteMasterBefore = ((git ls-remote origin refs/heads/master) -split '\s+')[0]
git merge-base --is-ancestor $remoteMasterBefore $releaseSha
git show-ref --verify --quiet refs/tags/v0.7.24
git ls-remote --tags origin refs/tags/v0.7.24 'refs/tags/v0.7.24^{}'
```

期望：分支是 `master`；tracked diff 和 staged diff 都为空；除本任务目录外没有未预期工作区改动；候选提交仍包含预期 4 个提交；`$remoteMasterBefore` 是候选提交祖先；本地和远端均没有 `v0.7.24`。任一不符即停止。

### 2. 普通推送并锁定远端结果

```powershell
git push origin master
$remoteMasterAfter = ((git ls-remote origin refs/heads/master) -split '\s+')[0]
$remoteMasterAfter
```

期望远端 SHA 精确等于 `$releaseSha`。禁止 `--force`、`--force-with-lease` 或任何历史改写。推送失败或远端结果不等时停止。

### 3. 等待精确 SHA 的 Rust CI

GitHub 创建 run 可能有短暂延迟；有界重查下面的查询，未出现时停止并报告，不用其他提交的 run 代替：

```powershell
gh run list --workflow rust-ci.yml --event push --branch master --commit $releaseSha --limit 1 --json databaseId,status,conclusion,headBranch,headSha,url
$ciRun = gh run list --workflow rust-ci.yml --event push --branch master --commit $releaseSha --limit 1 --json databaseId --jq '.[0].databaseId'
gh run watch $ciRun --exit-status
gh run view $ciRun --json status,conclusion,headBranch,headSha,url,jobs --jq '{status,conclusion,headBranch,headSha,url,jobs:[.jobs[]|{name,status,conclusion}]}'
```

只有 run 为 `completed/success`、`headBranch=master`、`headSha=$releaseSha`，且输出中的 `check` 与全部 jobs 均为 `success` 才继续。失败、取消、超时、空 run ID 或任何非 success 时停止，且不得创建 tag。

### 4. Tag 前二次防漂移检查

```powershell
git status --short --branch
git rev-parse HEAD
git ls-remote origin refs/heads/master
git show-ref --verify --quiet refs/tags/v0.7.24
git ls-remote --tags origin refs/tags/v0.7.24 'refs/tags/v0.7.24^{}'
```

要求本地 `HEAD` 和远端 `master` 仍等于 `$releaseSha`，tracked/staged diff 仍为空，且 tag 仍不存在。任何漂移都停止并重新规划。

### 5. 创建并推送唯一 tag

```powershell
git tag -a v0.7.24 $releaseSha -m "发布 v0.7.24"
git cat-file -t v0.7.24
git rev-parse 'v0.7.24^{}'
git push origin refs/tags/v0.7.24:refs/tags/v0.7.24
git ls-remote --tags origin refs/tags/v0.7.24 'refs/tags/v0.7.24^{}'
```

期望 tag 类型为 `tag`，本地和远端 peeled commit 都等于 `$releaseSha`。只推该 tag，不使用 `git push --tags`。push 失败后不移动或删除本地 tag。

### 6. 验证 Release Aether

有界重查直到找到 `headBranch=v0.7.24`、`headSha=$releaseSha` 的 push run：

```powershell
gh run list --workflow release.yml --event push --commit $releaseSha --limit 10 --json databaseId,status,conclusion,headBranch,headSha,url --jq '.[] | select(.headBranch == "v0.7.24")'
$releaseRun = gh run list --workflow release.yml --event push --commit $releaseSha --limit 10 --json databaseId,headBranch --jq '.[] | select(.headBranch == "v0.7.24") | .databaseId' | Select-Object -First 1
```

找到对应 run 后，必须继续等待完整发布：

```powershell
gh run watch $releaseRun --exit-status
gh run view $releaseRun --json status,conclusion,headBranch,headSha,url,jobs --jq '{status,conclusion,headBranch,headSha,url,jobs:[.jobs[]|{name,status,conclusion}]}'
gh release view v0.7.24 --json tagName,isDraft,isPrerelease,publishedAt,url,assets --jq '{tagName,isDraft,isPrerelease,publishedAt,url,assets:[.assets[]|{name,size,contentType,state}]}'
```

要求七个 release jobs 全部 success；release 非 draft、非 prerelease；四个必需 assets 均为 `uploaded`：

- `aether-v0.7.24-linux-amd64.tar.gz`
- `aether-v0.7.24-linux-arm64.tar.gz`
- `install.sh`
- `SHA256SUMS`

任一失败都停止并报告，不移动/删除 tag，不自动重跑 workflow。

### 7. 最终报告与 Trellis 收尾

报告锁定 SHA、`origin/master`、本地/远端 tag peeled SHA、Rust CI run URL/结论、Release Aether run URL/结论以及 assets 结果。不得包含 remote URL 或认证信息。之后按 Trellis Phase 2.2、3.3、3.4 顺序检查、判断是否需要 spec 更新并提交任务工件；未经另行授权不 push 新提交。

## 停止条件摘要

- 现场出现未预期改动或分支/SHA 漂移。
- 远端 master/tag 与预期不符。
- 找不到精确 SHA 的 CI/release run。
- CI 或 release run 失败、取消、超时或 jobs 非全 success。
- GitHub 权限、认证或网络错误导致结果无法实时验证。
