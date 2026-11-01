# history Specification

## Purpose
定义提交如何把工作区固化为不可变历史、如何只提交指定路径，以及历史为何只能前进、用户如何浏览与追溯这些版本。

## Requirements

### Requirement: 全量提交

无路径参数时，`fodelta commit -m <message>` SHALL 提交工作区中全部非忽略变更：新增文件、修改、删除、模式变化，以及已提交但后来被忽略的文件。新提交的父提交 MUST 为当前 `HEAD`。

#### Scenario: 提交全部变更

- **WHEN** 工作区新增 `a.txt`、修改 `b.txt`、删除 `c.txt` 后执行 `fodelta commit -m "checkpoint"`
- **THEN** 新提交同时包含新增、修改与删除，`fodelta status` 随后输出 clean

#### Scenario: 默认提交包含未跟踪新文件

- **WHEN** 工作区存在未跟踪且未被忽略的 `new.txt`，用户不带路径执行 `fodelta commit -m "all"`
- **THEN** `new.txt` 进入新提交，不要求预先 `add`

#### Scenario: 已提交的忽略文件仍被提交

- **WHEN** `build.log` 已被提交，之后 `.gitignore` 新增 `*.log`，文件仍在磁盘上
- **THEN** 全量提交仍记录该文件当前内容，不会因忽略规则把它当成删除

#### Scenario: 空工作区

- **WHEN** 工作区与 `HEAD` 完全一致且未使用 `--allow-empty`
- **THEN** 命令以非 0 退出，输出“nothing to commit”，`refs/heads/main` 保持不变

### Requirement: 选择性提交

`fodelta commit -m <message> -- <path>...` SHALL 只把指定路径的工作区状态写入新提交：指定文件被新增、修改或删除；指定目录前缀下所有非忽略文件以及该目录下已提交文件按工作区状态处理。未指定的路径 MUST 保持父提交中的内容，工作区 MUST NOT 被修改，未提交修改 MUST 保持未提交状态；命令 MUST NOT 为未选中路径记录任何排除状态，下一次不带路径的提交只要该路径未被 `.gitignore` 匹配就会包含它。

#### Scenario: 只提交一个文件

- **WHEN** 用户修改 `a.txt` 与 `b.txt` 后执行 `fodelta commit -m "a only" -- a.txt`
- **THEN** 新提交只包含 `a.txt` 的修改，`fodelta status` 仍显示 `b.txt` 为修改

#### Scenario: 提交目录前缀

- **WHEN** 用户执行 `fodelta commit -m "docs" -- docs/`
- **THEN** `docs/` 下的新增、修改与删除进入提交，`src/` 下的未提交修改保持原样

#### Scenario: 提交指定删除

- **WHEN** 已提交文件 `old.txt` 被删除，用户执行 `fodelta commit -m "drop" -- old.txt`
- **THEN** 新提交中不再包含 `old.txt`，其余未指定路径不变

#### Scenario: 选择性提交后剩余变更

- **WHEN** 用户只提交了部分路径
- **THEN** `fodelta status` 只列出未提交的变更，已提交路径不再出现

#### Scenario: 选择性提交不修改工作区

- **WHEN** 用户执行 `fodelta commit -m "a only" -- a.txt`
- **THEN** `a.txt` 与 `b.txt` 的磁盘内容均保持提交前的状态，只有 `HEAD` 发生变化

#### Scenario: 未选中的新文件下次仍会被提交

- **WHEN** 用户只提交了 `a.txt`，工作区还有未跟踪且未被忽略的 `new.txt`，随后执行不带路径的 `fodelta commit -m "all"`
- **THEN** `new.txt` 进入第二次提交，不会因为上次选择性提交而被排除

#### Scenario: 拒绝逃逸路径

- **WHEN** 用户执行 `fodelta commit -m "x" -- ../outside.txt`
- **THEN** 命令以非 0 退出，输出“path escapes repository”，且不产生提交

#### Scenario: 路径不匹配

- **WHEN** 用户指定的路径既不存在于 `HEAD` 也不存在于工作区
- **THEN** 命令以非 0 退出，输出该路径未匹配任何文件，且不产生提交

#### Scenario: 显式选择被忽略文件

- **WHEN** 用户对未跟踪且被忽略的文件执行 `fodelta commit -m "x" -- secret.env`
- **THEN** 命令以非 0 退出，提示该路径被忽略，并建议在 `.gitignore` 中加入 `!secret.env` 或改用其他文件名

### Requirement: 提交元数据与空提交

每个提交 SHALL 记录树对象、唯一父提交（根提交除外）、作者姓名与邮箱、带时区的 Unix 时间戳、以及 UTF-8 提交信息。选择性提交的结果树与父提交相同时 MUST 视为空提交并拒绝，除非传入 `--allow-empty`。

#### Scenario: 元数据可读

- **WHEN** 用户执行 `fodelta show HEAD`
- **THEN** 输出包含提交短哈希、作者、提交时间、完整提交信息与相对父提交的变更文件列表

#### Scenario: 拒绝非法提交信息

- **WHEN** 用户通过参数传入非 UTF-8 的提交信息
- **THEN** 命令以非 0 退出，输出编码错误，且不产生提交

#### Scenario: 允许空提交

- **WHEN** 用户执行 `fodelta commit -m "checkpoint" --allow-empty`
- **THEN** 命令成功，新提交与父提交的树相同

### Requirement: 历史只前进

`HEAD` SHALL 只移动到“以当前 `HEAD` 为父”的新提交；系统 MUST NOT 提供 `reset`、改写提交、切换分支或 detached HEAD 的能力。`rollback <rev>` 通过追加新提交回到旧状态，已有提交 MUST 保持可达。

#### Scenario: 不存在重置命令

- **WHEN** 用户执行 `fodelta reset --hard HEAD~1`
- **THEN** 命令以非 0 退出并提示未知子命令，仓库与工作区不发生任何变化

#### Scenario: 提交链始终连续

- **WHEN** 用户连续执行 `commit` 与 `rollback` 若干次后查看 `log`
- **THEN** 每个提交都只有一个父提交，父链连续且长度等于操作次数

#### Scenario: 回退不丢失历史

- **WHEN** 历史为 `c3 <- c2 <- c1`，用户执行 `fodelta rollback HEAD~2`
- **THEN** 新提交 `c4` 的父为 `c3`，`c3`、`c2`、`c1` 仍可通过 `log` 与 `show` 访问

### Requirement: 历史浏览

`fodelta log` SHALL 从 `HEAD` 开始按“新到旧”的顺序遍历单条父链，退出码为 0。命令 MUST 支持 `--oneline`、`-n <N>`、`--stat`、`--reverse` 与 `-- <path>` 过滤；`-- <path>` 仅显示触及该路径的提交。

#### Scenario: 限制条数

- **WHEN** 历史中有 10 个提交，用户执行 `fodelta log -n 3 --oneline`
- **THEN** 输出恰好 3 行提交摘要，且第一行为最新提交

#### Scenario: 按路径过滤

- **WHEN** 提交 A 修改 `docs/x.md`，提交 B 只修改 `src/y.rs`，用户执行 `fodelta log -- docs/x.md`
- **THEN** 输出包含 A 且不包含 B

#### Scenario: 空仓库的日志

- **WHEN** 仓库尚无任何提交，用户执行 `fodelta log`
- **THEN** 命令以非 0 退出，输出提示“当前分支尚无提交”，不打印伪造记录

### Requirement: 提交详情查看

`fodelta show <rev>` SHALL 输出提交元数据以及与父提交的差异；根提交与空树比较。`fodelta show <rev>:<path>` SHALL 将对应 blob 的原始字节写入标准输出。

#### Scenario: 查看历史提交

- **WHEN** 用户执行 `fodelta show HEAD~1`
- **THEN** 输出该提交的作者、时间、信息，以及 `HEAD~1` 相对 `HEAD~2` 的差异

#### Scenario: 输出单个文件内容

- **WHEN** 用户执行 `fodelta show HEAD:src/main.rs`
- **THEN** 标准输出为 `src/main.rs` 在该提交中的完整内容，二进制内容按原字节输出

#### Scenario: 路径不存在

- **WHEN** 用户执行 `fodelta show HEAD:no/such.txt`
- **THEN** 命令以非 0 退出，输出“path 'no/such.txt' does not exist in <rev>”

### Requirement: 版本引用语法

命令参数 `<rev>` SHALL 支持：`HEAD` / `@`（当前提交）、`main`（分支引用）、`HEAD~<N>`（沿父链回溯 N 代）、以及至少 6 位的唯一十六进制短哈希。无法解析或前缀有歧义时 MUST 以非 0 退出并列出候选。

#### Scenario: 回溯 N 代

- **WHEN** 历史为 `c3 <- c2 <- c1`，用户执行 `fodelta show HEAD~2`
- **THEN** 展示 `c1`

#### Scenario: 短哈希歧义

- **WHEN** 用户给出的 6 位前缀同时匹配两个提交对象
- **THEN** 命令以非 0 退出，输出“ambiguous revision”，并列出两个候选短哈希

#### Scenario: 未出生分支

- **WHEN** 仓库尚无提交，用户执行 `fodelta show HEAD`
- **THEN** 命令以非 0 退出，输出“HEAD does not point to a commit yet”
