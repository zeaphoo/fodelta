# Spec Delta

## Purpose

定义仓库在长期使用、并发访问与异常中断下如何保持可维护、可校验与可恢复，覆盖垃圾回收、完整性检查、加锁与原子写入。

## ADDED Requirements

### Requirement: 可达性垃圾回收

`fodelta gc` SHALL 从 `refs/heads/main` 的完整祖先链、以及 `logs/safety` 中仍在保留期内的安全快照出发标记可达对象；仅当对象不可达且最后修改时间早于 `core.prune_age_days`（默认 14 天）时才允许删除。存在任何可达对象被误删的风险时 MUST 保留该对象。

#### Scenario: 清理不可达对象

- **WHEN** 一次中断的提交留下了无引用的临时对象，且其时间早于 prune 期限
- **THEN** `fodelta gc` 删除这些对象，`fodelta fsck` 仍通过

#### Scenario: 保留期内的安全快照不被清理

- **WHEN** 安全快照创建后不足保留期，用户执行 `fodelta gc`
- **THEN** 该快照及其树、blob 仍可被 `fodelta show` 读取

#### Scenario: 预演模式

- **WHEN** 用户执行 `fodelta gc --dry-run`
- **THEN** 输出将要删除的对象数量与可回收字节数，且不删除任何文件

### Requirement: 完整性校验

`fodelta fsck` SHALL 校验引用指向、提交父链、树对象结构，以及所有可达 blob 的哈希；`--full` 时 MUST 额外扫描并校验对象库中的全部对象。发现缺失或损坏时 MUST 输出涉及路径、报告问题总数并以非 0 退出，且 MUST NOT 修改仓库。

#### Scenario: 正常仓库

- **WHEN** 用户在健康仓库执行 `fodelta fsck`
- **THEN** 输出检查对象数量，退出码为 0

#### Scenario: 检测损坏对象

- **WHEN** 某个 blob 文件内容被篡改后执行 `fodelta fsck`
- **THEN** 输出 `corrupt object <short-id>`（含引用它的树或提交），退出码非 0，且对象未被自动删除

#### Scenario: 悬空对象

- **WHEN** 对象库中存在没有任何提交或安全快照引用的对象
- **THEN** `fodelta fsck` 将其列为 dangling 并给出数量，退出码保持 0

### Requirement: 并发写互斥

所有会修改仓库的命令（包括 `commit`、`restore`、`rollback`、`gc`）SHALL 在操作期间持有 `.fodelta/lock` 上的排他锁。第二个写命令 MUST 以非 0 退出并提示已有 fodelta 进程正在操作；进程退出（包括崩溃）后锁 MUST 可被后续进程获取，不依赖手工删锁。

#### Scenario: 并发提交

- **WHEN** 两个 `fodelta commit` 同时启动
- **THEN** 至多一个成功，另一个输出“another fodelta process is running”并非 0 退出，历史不会分叉或丢失

#### Scenario: 崩溃后锁释放

- **WHEN** 一个持有锁的进程被强制终止
- **THEN** 后续命令可正常获取锁并继续操作

### Requirement: 提交原子性

一次提交 SHALL 先完成所有对象写入，再以原子方式更新 `refs/heads/main`。进程在任意时刻崩溃后，引用 MUST 要么指向旧提交、要么指向完整可读的新提交；MUST NOT 出现指向缺失对象的引用。

#### Scenario: 中断不产生半成品

- **WHEN** 提交过程中进程被强制终止，随后重新运行 `fodelta status` 与 `fodelta fsck`
- **THEN** 仓库可正常打开，`HEAD` 要么是提交前版本要么是完整的新提交，`fsck` 通过

#### Scenario: 残留临时文件

- **WHEN** 崩溃后对象目录中残留临时文件
- **THEN** 后续命令忽略这些文件，`fodelta gc` 可清理它们，且不影响任何哈希校验

### Requirement: 回退的收敛性

`rollback` SHALL 按“写入安全快照 → 写入新提交 → 原子更新引用 → 恢复工作区 → 刷新 stat-cache”的顺序执行；中途崩溃后，再次执行 `rollback` 或从安全快照恢复 MUST 能得到一致状态。

#### Scenario: 回退中途中断

- **WHEN** `rollback` 在恢复工作区阶段被强制终止
- **THEN** `HEAD` 指向完整的新提交，`fodelta status` 能正常报告未完成的工作区差异，重新执行 `rollback` 后状态收敛

### Requirement: 缓存与状态文件容错

`logs/safety` 中不可解析的行 MUST 被跳过并输出警告，不得导致 `fodelta safety` 失败；stat-cache 损坏 MUST 被丢弃并在后续写操作中重建，MUST NOT 让仓库整体不可用或产生错误的状态分类。

#### Scenario: safety 日志损坏行被跳过

- **WHEN** `.fodelta/logs/safety` 中混入无法解析的行
- **THEN** `fodelta safety` 跳过该行并正常输出其余记录，退出码为 0

#### Scenario: stat-cache 损坏

- **WHEN** `.fodelta/stat-cache` 被截断
- **THEN** `fodelta status` 正常完成并按需重建缓存，不报告仓库损坏
