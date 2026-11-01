# Spec Delta

## Purpose

定义如何把历史版本的文件恢复到工作区、如何整体回退到旧版本，以及在覆盖未提交内容前如何保证用户数据可找回。

## ADDED Requirements

### Requirement: 从版本恢复工作区

`fodelta restore [<rev>] [-- <path>...]` SHALL 用 `<rev>` 的内容覆盖工作区对应路径；`<rev>` 缺省为 `HEAD`。该命令 MUST NOT 修改 `HEAD` 或历史。传入 `--delete` 时 MUST 删除目标范围内“存在于工作区但不存在于 `<rev>`”的文件，并清理因此产生的空目录。

#### Scenario: 恢复被修改的文件

- **WHEN** 用户修改 `a.txt` 后执行 `fodelta restore a.txt`
- **THEN** `a.txt` 恢复为 `HEAD` 中的内容，`fodelta status` 不再显示该修改

#### Scenario: 恢复旧版本文件

- **WHEN** 用户执行 `fodelta restore HEAD~2 -- src/a.rs`
- **THEN** `src/a.rs` 的内容变为 `HEAD~2` 中的版本，其余文件不变，`HEAD` 不变

#### Scenario: 恢复整棵树并删除多余文件

- **WHEN** 用户执行 `fodelta restore HEAD~1 --delete`
- **THEN** 工作区中未被忽略的文件集合与 `HEAD~1` 的树完全一致（被忽略文件保留），`HEAD` 指向不变

#### Scenario: 恢复路径不存在

- **WHEN** 用户对 `<rev>` 中不存在的路径执行 `fodelta restore <rev> -- no.txt`
- **THEN** 命令以非 0 退出，输出该路径在版本中不存在，且不修改工作区

### Requirement: 历史级回退

`fodelta rollback <rev>` SHALL 创建一个以当前 `HEAD` 为父、树内容等于 `<rev>` 树的新提交，并把工作区同步为该树；`refs/heads/main` MUST 前移到新提交。提交信息缺省为 `rollback to <short-id>`，可用 `-m` 覆盖。目标树与当前 `HEAD` 树相同时 MUST NOT 创建提交；若工作区也已一致则输出“nothing to rollback”，否则仅同步工作区（必要时创建安全快照）。

#### Scenario: 回退后历史保留

- **WHEN** 历史为 `c3 <- c2 <- c1`，用户执行 `fodelta rollback HEAD~2`
- **THEN** 新提交 `c4` 的树等于 `c1`，`c4` 的父为 `c3`，`fodelta log` 仍能看到 `c3` 与 `c2`

#### Scenario: 回退后工作区一致

- **WHEN** `rollback` 成功完成
- **THEN** 工作区内容与 `c4` 的树一致，`fodelta status` 输出 clean

#### Scenario: 目标与当前一致且工作区干净

- **WHEN** 工作区干净，用户执行 `fodelta rollback HEAD`
- **THEN** 命令输出“nothing to rollback”，不创建提交，工作区不变

#### Scenario: 回退中断后重跑

- **WHEN** 一次 `rollback` 在恢复工作区途中被终止，`HEAD` 已指向目标树
- **THEN** 再次执行同一 `fodelta rollback <rev>` 会补齐工作区差异，不创建重复提交

### Requirement: 安全快照

`restore` 与 `rollback` 在会覆盖未提交的工作区内容前 SHALL 创建安全快照：以当前 `HEAD` 为父、当前工作区为树（忽略规则生效），并把记录写入 `logs/safety`。命令输出 MUST 给出可恢复的短哈希；`--no-safety` 可关闭该行为，但 MUST 输出明确警告。工作区干净时 MUST NOT 创建多余快照。

#### Scenario: 覆盖前自动快照

- **WHEN** 用户修改 `a.txt` 后执行 `fodelta restore a.txt`
- **THEN** 输出包含“safety snapshot <short-id>”，`fodelta show <short-id>` 可看到修改后的内容，随后 `a.txt` 被恢复

#### Scenario: 回退前保留工作区

- **WHEN** 工作区有未提交修改，用户执行 `fodelta rollback HEAD~1`
- **THEN** 修改先被安全快照保存，回退完成后可用 `fodelta restore <safety-id> --delete` 找回

#### Scenario: 干净工作区不生成快照

- **WHEN** 工作区与 `HEAD` 一致，用户执行 `fodelta restore HEAD`
- **THEN** 命令不创建安全快照，输出中不出现 safety 相关提示

#### Scenario: 关闭安全快照

- **WHEN** 用户在脏工作区执行 `fodelta rollback HEAD~1 --no-safety`
- **THEN** 命令执行成功，但输出包含“未创建安全快照，未提交修改可能无法恢复”的警告

### Requirement: 安全快照列表

`fodelta safety` SHALL 按时间倒序列出 `logs/safety` 中的安全快照，显示短哈希、时间与原因；无法解析的行 MUST 被跳过并输出警告。安全快照 MUST NOT 出现在 `refs/` 下，也不得被 `log` 当作主线历史。

#### Scenario: 列出安全快照

- **WHEN** 用户执行一次会覆盖未提交内容的 `restore` 后运行 `fodelta safety`
- **THEN** 输出首条为刚创建的快照，包含其短哈希与原因

#### Scenario: 安全快照不进入主线

- **WHEN** 用户执行 `fodelta log --oneline`
- **THEN** 输出不包含安全快照提交

### Requirement: 恢复路径安全

任何写入或删除 MUST 限定在仓库根内。路径为绝对路径、包含 `..` 组件、或解析后逃逸仓库根时，命令 MUST 失败且 MUST NOT 修改工作区。默认 MUST NOT 跟随符号链接写入其目标。

#### Scenario: 拒绝目录穿越

- **WHEN** 用户执行 `fodelta restore HEAD -- ../outside.txt`
- **THEN** 命令以非 0 退出，输出“path escapes repository”，仓库外文件保持不变

#### Scenario: 不透过符号链接写入

- **WHEN** 工作区 `link` 是指向仓库外目录的符号链接
- **THEN** 恢复操作不会通过该链接写入仓库外文件，而是替换 `link` 本身或拒绝该路径

### Requirement: 恢复后的状态一致性

`restore` 与 `rollback` 完成后，系统 SHALL 保证 stat-cache 与工作区实际文件一致；无法刷新时 MUST 丢弃受影响的缓存条目、降级为下次重新哈希，而不是让 `status` 产生虚假的修改条目。

#### Scenario: 恢复后状态准确

- **WHEN** 用户执行 `fodelta restore HEAD` 后立即执行 `fodelta status`
- **THEN** 输出 clean，不出现虚假的修改条目
