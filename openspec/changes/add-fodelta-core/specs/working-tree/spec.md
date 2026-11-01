# Spec Delta

## Purpose

定义工作区如何被扫描、哪些文件属于新增/修改/删除或被忽略，以及 `status` 如何展示相对 `HEAD` 的变更集合。

## ADDED Requirements

### Requirement: 状态分类

`fodelta status` SHALL 相对 `HEAD` 将工作区文件归类为“新增”“修改”“删除”，并输出可提交变更的总数；工作区与 `HEAD` 一致时输出“working tree clean”。路径 MUST 相对仓库根展示。

#### Scenario: 展示三类变更

- **WHEN** 用户新增 `a.txt`、修改 `b.txt`、删除 `c.txt` 后执行 `fodelta status`
- **THEN** 输出分别列出 `a.txt`（新增）、`b.txt`（修改）、`c.txt`（删除），并给出总数 3

#### Scenario: 干净工作区

- **WHEN** 工作区与 `HEAD` 完全一致
- **THEN** 输出包含“working tree clean”，退出码为 0

#### Scenario: 未出生仓库

- **WHEN** 仓库尚无提交且工作区有 2 个未被忽略的文件
- **THEN** 两个文件都显示为“新增”，并提示“no commits yet”

### Requirement: 变更检测与 stat-cache

系统 SHALL 维护 `.fodelta/stat-cache` 记录路径、blob 哈希与文件元数据（大小、mtime、inode、模式），以便在未变化时跳过内容哈希。元数据变化时 MUST 重新计算内容哈希；缓存损坏或缺失时 MUST 丢弃并以工作区为准，重建可发生在后续写操作中（可输出警告），MUST NOT 影响状态分类的正确性。

#### Scenario: 仅修改时间不产生变更

- **WHEN** 用户对未修改内容的文件执行 `touch` 后运行 `fodelta status`
- **THEN** 该文件不显示为修改，状态仍为 clean

#### Scenario: 缓存损坏可恢复

- **WHEN** `.fodelta/stat-cache` 被截断为随机字节后运行 `fodelta status`
- **THEN** 命令正常完成，变更分类与无缓存时一致；损坏的缓存被丢弃，下一次写操作会重建它

#### Scenario: 状态命令只读

- **WHEN** 用户连续运行两次 `fodelta status`
- **THEN** `.fodelta/stat-cache` 的 mtime 与内容均不改变

### Requirement: 忽略文件的默认隐藏

未被跟踪且被忽略的文件 MUST NOT 出现在默认 `status` 输出中，也 MUST NOT 进入提交。`status --ignored` MUST 额外列出这些文件并标明为忽略。

#### Scenario: 忽略文件默认不显示

- **WHEN** `build.log` 被 `.gitignore` 忽略且未跟踪
- **THEN** 默认 `fodelta status` 不列出该文件；`fodelta status --ignored` 列出它

#### Scenario: 被忽略文件的修改不影响状态

- **WHEN** 用户修改一个被忽略且未跟踪的文件
- **THEN** `fodelta status` 仍显示 clean

### Requirement: 已提交路径优先于忽略

已存在于 `HEAD` 的文件 MUST 继续参与状态、diff 与提交，即使其后被新的忽略规则匹配；其从磁盘删除时 MUST 被记录为删除，而不是被忽略规则隐藏。

#### Scenario: 已提交文件后来被忽略

- **WHEN** `build.log` 已提交，随后 `.gitignore` 新增 `*.log`，文件仍在磁盘上
- **THEN** `fodelta status` 不将其列为忽略，仍按 `HEAD` 比较内容

#### Scenario: 删除已提交的忽略文件

- **WHEN** 已提交的 `build.log` 被删除，且忽略规则匹配 `*.log`
- **THEN** `fodelta status` 显示 `build.log` 为删除

### Requirement: 文件类型与模式

系统 SHALL 支持普通文件、符号链接与 Unix 可执行位。符号链接 MUST 以其链接目标文本作为 blob 内容存储，恢复时重建链接；权限变化（`100644` ↔ `100755`）MUST 出现在状态与差异中。FIFO、socket、设备文件 MUST 被跳过并输出警告。

#### Scenario: 可执行位变化

- **WHEN** 用户对已提交脚本执行 `chmod +x`
- **THEN** `fodelta status` 将其显示为模式变化，`fodelta diff` 输出 `old mode 100644 / new mode 100755`

#### Scenario: 符号链接

- **WHEN** 工作区存在指向 `../target` 的符号链接 `link`
- **THEN** 提交后 `HEAD` 中 `link` 的模式为 `120000`，内容为字符串 `../target`

### Requirement: 大文件保护

提交时单个文件超过 `core.max_file_size_mb`（默认 512 MiB）MUST 失败并列出超限文件，除非传入 `--allow-large-files` 或调高配置；MUST NOT 静默跳过。

#### Scenario: 默认拒绝超大文件

- **WHEN** 工作区存在 600 MiB 文件且默认上限为 512 MiB，用户执行 `fodelta commit -m "x"`
- **THEN** 命令以非 0 退出，输出超限文件路径与当前上限，仓库不产生新提交

#### Scenario: 显式允许

- **WHEN** 用户执行 `fodelta commit -m "x" --allow-large-files`
- **THEN** 文件被分块读取、压缩并写入对象库，提交成功

### Requirement: 空目录与目录级忽略

空目录 MUST NOT 被跟踪。被忽略的目录 MUST 在扫描时整体跳过；若其中存在已提交文件，则仅针对这些已知路径做 stat/hash。

#### Scenario: 空目录不入库

- **WHEN** 工作区存在空目录 `empty/`，用户执行提交
- **THEN** 新提交的树中不存在 `empty/`，且 `fodelta status` 不将其列为新增
