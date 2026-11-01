# Proposal

## Why

个人本地目录（笔记、脚本、配置、素材、原型代码）需要“随时存档、随时回退、随时比较”，但 `git` 的分支、远端与合并提交对这类场景过重；`rustory` 证明了“本地快照 + 内容寻址”的方向可行，但它只能整目录快照，无法“只提交这几个文件”，且没有树对象、伪 diff、无历史比较能力。

`fodelta` 用一套类 Git 但显著更小的模型补齐这块空白：**单分支、无远端、无暂存区、无 reset、无 merge**；`commit` 默认提交全部变更，也可以用 `-- <path>` 只提交指定文件/目录，其余修改原样留在工作区，且不会被记成任何持久化的排除状态；下次不带路径提交时仍会按 `.gitignore` 规则参与。用内容寻址对象库保证去重与完整性，直接复用项目里已有的 `.gitignore`。

不做完整 index 暂存区：用户要的是“提交时挑选文件”，而不是“维护一份独立的暂存树”。把选择范围作为 `commit` 的一次性参数，既满足部分提交，又不引入三态状态、`add`/`rm`/`restore --staged` 等额外概念。

## What Changes

- 新建 Rust 项目 `fodelta`（lib + bin），零外部命令依赖，支持 Linux / macOS / Windows。
- 新增 `.fodelta/` 仓库布局：内容寻址对象库（blob / tree / commit）、单分支 `refs/heads/main`、`HEAD`、安全快照日志、忽略规则。
- 新增命令集：`init`、`status`、`commit`、`log`、`show`、`diff`、`restore`、`rollback`、`safety`、`ignore`、`gc`、`fsck`、`config`。
- **选择性提交**：`fodelta commit -m <msg> [-- <path>...]`；无路径时提交全部非忽略变更（含新增与删除），有路径时只把指定文件/目录的工作区状态写入新提交，未指定路径保持原样。
- 历史严格线性且**只前进**：没有 `reset`、分支、远端、detached HEAD 与合并提交；回退通过 `rollback <rev>` 追加一个新提交实现。
- **diff 支持三种比较**：工作区 ↔ `HEAD`（默认，整仓）、工作区 ↔ 指定版本、版本 ↔ 版本。
- 破坏性操作（`restore` / `rollback`）前自动生成安全快照，`fodelta safety` 可列出并恢复。
- 忽略规则只认项目内的 `.gitignore`（含嵌套），不维护 `.fodelta/info/exclude` 等第二份忽略来源；已提交文件不受忽略规则影响。
- 可靠性：原子引用更新、进程锁、内容校验、`fsck` 与 `gc`。

## Capabilities

### New Capabilities

- `repository`: 仓库初始化、向上发现、单分支与 `HEAD` 约束、用户身份与配置、磁盘格式版本。
- `working-tree`: 工作区扫描、新增/修改/删除/未跟踪/忽略状态、变更检测、文件类型与大小保护。
- `history`: 提交语义（全量与选择性提交）、提交元数据、`log` / `show`、版本引用语法与线性历史不变量。
- `diff`: 工作区 ↔ HEAD、工作区 ↔ 版本、版本 ↔ 版本三种比较，文本/二进制渲染与退出码。
- `rollback`: 文件级 `restore`、历史级 `rollback`（追加新提交）、自动安全快照与 `safety` 命令。
- `ignore`: 与 git 兼容的忽略规则来源、优先级、语法、内置排除，以及 `ignore` 子命令。
- `maintenance`: 可达性 GC、完整性校验、互斥锁、原子性与崩溃一致性、格式版本处理。

### Modified Capabilities

无（全新项目，无既有规格）。

## Impact

- 代码：新增 crate `fodelta`，`src/lib.rs` 承载核心库，`src/main.rs` 为薄 CLI；不修改 `rustory/`、`jj/` 等参考目录。
- 依赖：`clap`、`blake3`、`zstd`、`ignore`、`similar`、`toml` + `serde`、`chrono`、`rayon`、`thiserror` / `anyhow`、`anstream` / `anstyle`、`indicatif`；开发依赖 `assert_cmd`、`predicates`、`tempfile`、`proptest`、`criterion`。**不引入 `diffy`**（无三方合并），**不引入持久化 index**。
- 磁盘：在项目根目录下创建 `.fodelta/`；格式带版本号，升级时需保持向后兼容或提供迁移命令。
- 平台：路径、符号链接、可执行位在 Unix 与 Windows 上行为不同，需要在规格与文档中显式约定。
- 安全：恢复操作必须拒绝绝对路径与 `..` 逃逸；默认不跟随符号链接。
