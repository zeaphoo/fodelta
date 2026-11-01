# repository Specification

## Purpose
定义 fodelta 仓库如何创建、被发现与配置，并锁定“单分支、无远端、无 reset、无合并”这一核心约束，为提交、历史与回退提供统一前提。

## Requirements

### Requirement: 仓库初始化

`fodelta init [path]` SHALL 在目标目录创建 `.fodelta/` 仓库结构（`HEAD`、`refs/heads/`、`objects/`、`config.toml`），且 SHALL NOT 自动创建首个提交。目标目录已存在 `.fodelta/` 时命令 MUST 失败并提示仓库已存在。

#### Scenario: 在空目录初始化

- **WHEN** 用户在空目录执行 `fodelta init`
- **THEN** 命令退出码为 0，目录下出现 `.fodelta/HEAD` 与 `.fodelta/config.toml`，`refs/heads/` 目录存在但 `main` 引用在首次提交前不存在，并提示后续使用 `fodelta commit`

#### Scenario: 重复初始化

- **WHEN** 用户在已有 `.fodelta/` 的目录再次执行 `fodelta init`
- **THEN** 命令以非 0 退出，输出包含“already a fodelta repository”，且不修改任何现有文件

#### Scenario: 在子目录初始化

- **WHEN** 用户在父仓库的子目录执行 `fodelta init`
- **THEN** 命令失败，提示该目录已位于仓库 `<root>` 内，避免创建嵌套仓库

### Requirement: 仓库发现

所有仓库内命令 SHALL 从当前目录向上查找最近的 `.fodelta/` 作为仓库根，并支持全局 `-C <dir>` 指定起始目录。查找 MUST 在文件系统根或用户主目录边界停止；未找到时 MUST 以非 0 退出并提示运行 `fodelta init`。

#### Scenario: 在子目录执行命令

- **WHEN** 用户在 `<root>/notes/2026` 执行 `fodelta status`，且 `<root>/.fodelta` 存在
- **THEN** 命令以 `<root>` 为仓库根运行，输出中的路径相对 `<root>` 展示

#### Scenario: 仓库外执行

- **WHEN** 用户在没有任何上级 `.fodelta/` 的目录执行 `fodelta log`
- **THEN** 命令以非 0 退出，输出包含“not a fodelta repository”，不创建任何文件

#### Scenario: 嵌套仓库不被穿透

- **WHEN** 仓库根下存在 `vendor/other/.fodelta/`
- **THEN** 父仓库的扫描与提交 MUST 跳过 `vendor/other/` 整个子树，不读取也不修改其内容

### Requirement: 单分支与线性历史

仓库 SHALL 只有一个用户可见分支：`HEAD` 指向 `refs/heads/main`。系统 SHALL NOT 提供创建、切换、删除分支、配置远端、`reset`、合并或任何改写历史的命令；每个非根提交 MUST 恰好有一个父提交，且父提交 MUST 是创建时的 `HEAD`。

#### Scenario: 初始化的引用布局

- **WHEN** 仓库初始化完成
- **THEN** `.fodelta/HEAD` 内容为 `ref: refs/heads/main`，且 `.fodelta/refs/` 下不存在除 `heads/main` 外的用户引用

#### Scenario: 提交链保持线性

- **WHEN** 用户连续创建三个提交
- **THEN** 每个提交对象的 `parent` 字段仅有一个值，且第三个提交的祖先链长度恰好为 3

### Requirement: 用户身份

提交作者信息 SHALL 由仓库配置 `[user] name/email` 提供，环境变量 `FODELTA_AUTHOR_NAME` / `FODELTA_AUTHOR_EMAIL` MUST 覆盖配置。`init` SHALL 使用当前系统用户名与主机名生成可编辑的默认值；提交时若姓名或邮箱为空，命令 MUST 失败并提示 `fodelta config user.name "..."`。

#### Scenario: 使用配置身份提交

- **WHEN** 用户设置 `user.name = "Ada"`、`user.email = "ada@example.com"` 后执行 `fodelta commit -m "init"`
- **THEN** `fodelta log` 显示作者 `Ada <ada@example.com>`

#### Scenario: 环境变量覆盖

- **WHEN** 仓库配置姓名为 `Ada`，用户以 `FODELTA_AUTHOR_NAME=Grace` 执行提交
- **THEN** 新提交的作者名为 `Grace`

#### Scenario: 身份缺失时拒绝提交

- **WHEN** 配置与环境中均没有有效邮箱
- **THEN** `fodelta commit` 以非 0 退出，输出包含设置 `user.email` 的指引，且仓库不产生新提交

### Requirement: 配置管理

`fodelta config` SHALL 支持读取、设置与列出仓库配置，键名使用 `section.key` 形式（如 `core.compression`、`diff.context`）。未知键 MUST 被拒绝；未设置的键返回文档中声明的默认值。配置文件 SHALL 为可读的 TOML。

#### Scenario: 设置并读取配置

- **WHEN** 用户执行 `fodelta config core.compression 9`，随后执行 `fodelta config core.compression`
- **THEN** 第二条命令输出 `9`，且 `.fodelta/config.toml` 可被文本编辑器正常解析

#### Scenario: 拒绝未知键

- **WHEN** 用户执行 `fodelta config core.unknown_key 1`
- **THEN** 命令以非 0 退出，输出可用键列表或提示，且配置文件保持不变

### Requirement: 磁盘格式版本

`.fodelta/config.toml` SHALL 记录仓库格式版本。打开版本高于当前二进制的仓库时，命令 MUST 失败并提示升级 fodelta，且 MUST NOT 写入或迁移任何数据。

#### Scenario: 打开更高版本仓库

- **WHEN** 仓库格式版本为 2，而当前二进制仅支持 1
- **THEN** 任意仓库内命令以非 0 退出，输出包含“unsupported repository format version 2”，仓库内容保持不变
