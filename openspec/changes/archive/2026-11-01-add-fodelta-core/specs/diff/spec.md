# Spec Delta

## Purpose

定义 fodelta 如何比较工作区与当前仓库、工作区与历史版本、以及两个历史版本，产出适合人读与脚本消费的差异结果。

## ADDED Requirements

### Requirement: 三种比较视角

`fodelta diff` SHALL 支持以下比较，且默认（无参数）比较整个仓库的 `HEAD` 与工作区：

| 调用 | 左侧 | 右侧 |
|------|------|------|
| `fodelta diff` | `HEAD` | 工作区 |
| `fodelta diff <rev>` | `<rev>` | 工作区 |
| `fodelta diff <rev1> <rev2>` | `<rev1>` | `<rev2>` |

两侧完全相同时 MUST 不输出差异且退出码为 0。

#### Scenario: 默认比较 HEAD 与工作区

- **WHEN** 用户修改工作区中的 `a.txt` 后执行 `fodelta diff`
- **THEN** 输出仅包含 `a.txt` 相对 `HEAD` 的变化

#### Scenario: 与历史版本比较

- **WHEN** 用户执行 `fodelta diff HEAD~3 -- src/`
- **THEN** 输出工作区相对 `HEAD~3` 在 `src/` 目录下的全部差异

#### Scenario: 历史版本之间比较

- **WHEN** 用户执行 `fodelta diff HEAD~3 HEAD`
- **THEN** 输出两个版本之间全部差异，不读取工作区

#### Scenario: 相同版本

- **WHEN** 用户执行 `fodelta diff HEAD HEAD`
- **THEN** 标准输出为空，退出码为 0

### Requirement: 路径过滤

`diff` 的 `-- <path>` 参数 MUST 将比较限制在给定文件或目录前缀内，并对三种比较视角一致生效；不存在的路径只影响匹配结果，MUST NOT 导致命令失败。

#### Scenario: 限制到目录

- **WHEN** 用户执行 `fodelta diff -- src/lib.rs`
- **THEN** 输出只涉及 `src/lib.rs` 一个路径

#### Scenario: 不存在的路径

- **WHEN** 用户执行 `fodelta diff -- no/such/path`
- **THEN** 命令输出空差异并以 0 退出，不修改任何状态

### Requirement: 文本差异渲染

文本文件差异 SHALL 以 unified diff 形式输出，包含 `--- a/<path>` / `+++ b/<path>` 文件头、`@@ -l,c +l,c @@` 块头、默认 3 行上下文，并支持 `-U <n>` 调整上下文行数。新增行以 `+`、删除行以 `-` 标记；内容按字节比较，不静默转换换行符。

#### Scenario: 标准文本差异

- **WHEN** 用户把 `a.txt` 第 3 行由 `old` 改为 `new` 后执行 `fodelta diff`
- **THEN** 输出包含 `-old` 与 `+new`，且块头行号与实际行号一致

#### Scenario: 调整上下文

- **WHEN** 用户执行 `fodelta diff -U0`
- **THEN** 输出中不包含未变化的上下文行

#### Scenario: CRLF 内容不被改写

- **WHEN** 文件中包含 CRLF 行尾且仅有一行变化
- **THEN** 未变化行的 CRLF 原样保留在输出中，不出现整文件重写

### Requirement: 二进制与元数据变化

无法按 UTF-8 文本处理的文件 SHALL 以“Binary files a/<path> and b/<path> differ”形式提示，MUST NOT 输出乱码内容。模式变化 SHALL 输出 `old mode` / `new mode`；文件与符号链接之间的类型变化 MUST 被明确标记。

#### Scenario: 二进制文件差异

- **WHEN** 两个版本的 `logo.png` 内容不同
- **THEN** 输出 Binary files differ 提示，不打印文件字节

#### Scenario: 类型变化

- **WHEN** 路径 `cfg` 从普通文件变为符号链接
- **THEN** 输出同时包含 `deleted file mode 100644` 与 `new file mode 120000`

### Requirement: 汇总与机器可读输出

`fodelta diff` SHALL 支持 `--stat`（每文件增删行数与总计）、`--name-status`（`A`/`M`/`D`/`T` 前缀）与 `--name-only`，三者互斥且不与完整补丁同时输出。汇总 MUST 统计所有被比较路径，而不只是有文本差异的文件。

#### Scenario: 统计输出

- **WHEN** 用户执行 `fodelta diff --stat HEAD~1 HEAD`
- **THEN** 每个变更文件一行，末尾输出形如 `3 files changed, 10 insertions(+), 2 deletions(-)` 的汇总

#### Scenario: 名称与状态

- **WHEN** 用户执行 `fodelta diff --name-status HEAD~1 HEAD`
- **THEN** 每行形如 `M\tsrc/lib.rs`，新增文件前缀为 `A`，删除为 `D`

### Requirement: 差异退出码

`fodelta diff` 默认在存在差异时仍返回 0；传入 `--exit-code` 时 SHALL 在存在差异时返回 1、无差异返回 0。参数或仓库错误 MUST 以 2 返回，不得与“存在差异”的 1 混淆。

#### Scenario: 脚本检测差异

- **WHEN** 存在差异且用户执行 `fodelta diff --exit-code`
- **THEN** 退出码为 1

#### Scenario: 无差异时脚本通过

- **WHEN** 无差异且用户执行 `fodelta diff --exit-code`
- **THEN** 退出码为 0
