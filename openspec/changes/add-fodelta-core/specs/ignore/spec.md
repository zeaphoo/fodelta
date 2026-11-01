# Spec Delta

## Purpose

定义 fodelta 如何只依据项目内的 `.gitignore` 决定哪些文件不进入提交，并提供可解释的忽略诊断。

## ADDED Requirements

### Requirement: 唯一忽略来源

系统 SHALL 只读取仓库内各级 `.gitignore` 作为忽略规则来源，MUST NOT 读取 `.ignore`、`.git/info/exclude`、用户全局 git 忽略，也 MUST NOT 在 `.fodelta/` 下维护第二份 exclude 文件。系统 MUST NOT 自动修改 `.gitignore`，所有忽略规则只能由用户编辑。距离文件更近的 `.gitignore` MUST 优先于更远的规则；同一文件中后出现的规则 MUST 覆盖先出现的规则。

#### Scenario: 嵌套规则覆盖父级

- **WHEN** 根 `.gitignore` 包含 `*.log`，`src/.gitignore` 包含 `!keep.log`
- **THEN** `src/keep.log` 不被忽略，而根目录的 `other.log` 被忽略

#### Scenario: 不读取 `.ignore`

- **WHEN** 项目中存在 `.ignore` 文件并写入 `secret.txt`，但没有对应的 `.gitignore` 规则
- **THEN** `secret.txt` 不被忽略，仍出现在 `status` 中

#### Scenario: 不维护第二份忽略文件

- **WHEN** 用户执行 `fodelta ignore add "*.bak"`
- **THEN** 命令以非 0 退出并提示未知子命令，`.fodelta/` 下不产生任何 exclude 文件

### Requirement: gitignore 语法兼容

实现 MUST 兼容 gitignore 语义：`#` 注释、空行、`!` 取反、结尾 `/` 仅匹配目录、`**` 跨目录通配、`\` 转义、以及相对规则文件所在目录的匹配。行为 MUST 与 `git check-ignore` 对常见模式一致。

#### Scenario: 目录限定模式

- **WHEN** 规则为 `build/`，工作区同时存在目录 `build/` 与文件 `build`
- **THEN** 目录 `build/` 被忽略，文件 `build` 不被忽略

#### Scenario: 双星号匹配

- **WHEN** 规则为 `**/tmp/`
- **THEN** 任意深度的 `tmp/` 目录都被忽略

#### Scenario: 取反恢复

- **WHEN** 规则依次为 `*.tmp` 与 `!important.tmp`
- **THEN** `important.tmp` 不被忽略

### Requirement: 内置排除

`.fodelta/`、`.git/`、以及包含自身 `.fodelta/` 的嵌套仓库目录 MUST 始终被排除，且 MUST NOT 被 `.gitignore` 中的 `!` 规则取反。仓库根目录本身 MUST NOT 被忽略规则排除。

#### Scenario: 仓库元数据不入库

- **WHEN** 用户执行提交
- **THEN** `HEAD` 的树中不存在 `.fodelta/` 或 `.git/` 下的任何路径

#### Scenario: 嵌套仓库整体跳过

- **WHEN** 工作区存在 `vendor/other/.fodelta/`
- **THEN** `vendor/other/` 下的文件不会被父仓库提交

### Requirement: 忽略决定提交范围

未被跟踪且被忽略的文件 MUST NOT 进入任何提交，即使被 `fodelta commit -- <path>` 显式指定也 MUST 拒绝；错误信息 MUST 提示在 `.gitignore` 中加入 `!<path>` 取反。已提交文件不受忽略规则影响：即使其后被 `.gitignore` 匹配，仍参与状态、diff 与提交。

#### Scenario: 构建产物不进入提交

- **WHEN** `.gitignore` 包含 `target/`，工作区存在 `target/debug/app`
- **THEN** `fodelta commit -m "x"` 不包含该文件，仓库体积不因其增长

#### Scenario: 显式提交被忽略文件被拒绝

- **WHEN** 用户执行 `fodelta commit -m "x" -- target/app`
- **THEN** 命令以非 0 退出，提示该路径被忽略，并建议在 `.gitignore` 中加入 `!target/app`

#### Scenario: 已提交文件后来被忽略

- **WHEN** `build.log` 已被提交，随后 `.gitignore` 新增 `*.log`，文件仍在磁盘上
- **THEN** 后续提交仍包含该文件，`status` 仍按 `HEAD` 比较其内容

### Requirement: 忽略诊断命令

`fodelta ignore` SHALL 提供只读诊断：`list` 列出仓库内生效的 `.gitignore` 文件及其规则；`check <path>` 输出该路径是否被忽略、命中的规则与来源文件（可获取时包含行号）。`check` 对未忽略路径 MUST 明确输出“not ignored”。两个子命令 MUST NOT 修改任何忽略规则或仓库状态。

#### Scenario: 检查被忽略路径

- **WHEN** 用户执行 `fodelta ignore check target/debug/app`
- **THEN** 输出 `ignored`，并指明规则来自哪个 `.gitignore` 及对应模式

#### Scenario: 检查未忽略路径

- **WHEN** 用户执行 `fodelta ignore check src/main.rs`
- **THEN** 输出 `not ignored`，退出码为 0

#### Scenario: 列出规则文件

- **WHEN** 仓库根与 `src/` 各有一个 `.gitignore`，用户执行 `fodelta ignore list`
- **THEN** 输出列出这两个文件路径，不包含 `.fodelta` 下的任何文件
