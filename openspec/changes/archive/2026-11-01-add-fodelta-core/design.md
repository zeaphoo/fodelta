# Design

## Context

`fodelta` 是一个从零开始的本地目录版本管理工具，仓库当前为空。它服务的场景是“个人目录的存档、选择性提交、回退与比较”，因此显式放弃远端、分支、暂存区、`reset` 与合并（见 `proposal.md` 与 `specs/repository/spec.md`）。

设计参考了两类现有实现：

- `rustory`：本地快照 + 内容寻址 + `.gitignore` 的方向正确，但只能整目录快照，不能只提交指定文件，且没有树对象、伪 diff、无历史比较能力。
- `git` / `jj`：对象模型（blob/tree/commit）、内容寻址、原子引用更新是直接借用的成熟经验；fodelta 去掉分支、远端、暂存区、`reset` 与合并提交，只保留单条前进的主线。

硬约束：单分支线性历史、无远端、无暂存区、无 `reset`、无合并、支持 `commit -- <path>` 选择性提交、纯 Rust 无外部命令依赖、兼容 `.gitignore`、跨 Linux/macOS/Windows、破坏性操作可恢复。

## Goals / Non-Goals

**Goals**

- 默认整目录快照，同时允许 `commit -- <path>` 只提交指定文件/目录，未选中的修改原样留在工作区。
- 历史只前进：提交与回退都产生新提交，永不改写或丢弃已有提交。
- 三种明确的 diff 视角，行为可测试、可脚本化。
- 破坏性操作前自动留底，可通过 `fodelta safety` 找回。
- 在 2 万文件级别的目录上，`status` / `commit` 达到交互级响应。
- 磁盘格式显式版本化，为后续演进（如 packfile、cherry-pick）留出空间。

**Non-Goals**

- 持久化 index / 暂存区 / `add` / `rm` / `restore --staged`。
- 合并 / cherry-pick / 三方合并（本期不做；后续作为独立能力）。
- 分支、远端、clone/fetch/push、多人协作。
- `reset`、历史改写、detached HEAD。
- 多父合并提交。
- 行级部分提交（`commit -p`）；v1 只支持文件/目录粒度。
- 重命名检测（v1 显示为删除 + 新增）。
- packfile / delta 压缩（v1 使用 loose object；作为后续独立变更）。
- 子模块、稀疏检出、加密、文件监听。
- 对仓库外两个任意目录做独立 diff/merge。
- GUI、语言服务器、JSON 输出。

## Decisions

| # | 决策 | 主要备选 | 结论 |
|---|------|----------|------|
| D1 | 自研 blob/tree/commit 对象模型 | 直接内嵌 git 库（gix） | 自研 |
| D2 | 单分支、历史只前进、无 reset/merge | 允许 reset + 双亲合并 | 只前进 |
| D3 | 无暂存区；`commit -- <path>` 选择性提交 | 持久化 index | 路径过滤 |
| D4 | BLAKE3 + zstd loose object | SHA-1/SHA-256 + gzip | BLAKE3 + zstd |
| D5 | 自定义紧凑二进制 + 文本格式 | SQLite / JSON 元数据 | 文件格式 |
| D6 | `ignore` crate 复用 gitignore 语义 | 自写匹配器 | 复用 |
| D7 | `similar` 做文本 diff | 自写 Myers | 复用 |
| D8 | 安全快照写入独立 log，不用引用 | 隐藏分支 / stash | 独立 log |
| D9 | 先写对象、后原子更新引用；默认只 fsync 引用 | 全量 fsync | 可配置 |
| D10 | 单 crate（lib + bin），模块化 | workspace 多 crate | 单 crate |

### D1. 对象模型：blob / tree / commit

采用与 git 同构的最小对象集合：

```
commit ──parent──▶ commit ──parent──▶ ...
   │
   └──tree──▶ tree ──▶ tree
                └────▶ blob
```

- **blob**：文件内容（含符号链接目标文本），按内容寻址。
- **tree**：一层目录，条目为 `(mode, kind, oid, name)`，按名称字节序排序；目录本身也是 tree。
- **commit**：`tree` + 唯一 `parent` + `author` + `fodelta 1` + 提交信息。

选择理由：

1. 相同内容只存一份，移动/复制目录几乎零成本。
2. 树级复用让“只改一个文件”的提交只需新建一条根路径上的 tree 链。
3. 两个版本的比较天然是树的有序归并，不需要先展开成完整文件清单。
4. 未来加 packfile、重命名检测、cherry-pick 时不需要推翻模型。

备选 `gix`：能直接获得成熟实现，但会把 fodelta 的语义绑到 git 仓库与 git 配置上，且引入大量依赖；本项目的价值恰在于“小而可控的本地模型”。

**不变量**

- 提交一旦写入不可变；`refs/heads/main` 只会移动到“以当前 HEAD 为父”的新提交。
- 每个非根提交恰好一个 parent；parent 链无环（fsck 校验）。
- tree 中不出现 `.`、`..`、空名、含 `/` 或 NUL 的名称；条目按名称字节序严格递增。
- 工作区路径一律为相对仓库根的 `/` 分隔路径。

### D2. 单分支、历史只前进、不做合并

- `HEAD` 固定为 `ref: refs/heads/main`；不提供 `branch` / `checkout` / `remote` / `reset` 命令，也不存在 detached HEAD。
- 能改变 `HEAD` 的只有两种操作：`commit`（父 = 当前 HEAD）与 `rollback <rev>`（父 = 当前 HEAD，树 = `<rev>` 的树）。两者都只追加，不删除、不移动已有提交。
- 没有合并：单分支下历史不会分叉，`merge` 退化为 cherry-pick；本期用 `restore <rev>` 取回旧文件、手工组合，三方合并留到后续独立的 `cherry-pick` 能力。
- 由此获得的性质：`log` 永远是单链、`HEAD~N` 永远有定义、GC 只需沿一条父链加安全快照做可达性分析、任何“回退”都不会丢失可追溯的历史。

备选“允许 `reset --hard`”被否决：它会移动引用、产生不可达提交，需要 reflog 与额外保留策略才能保证可恢复，与“历史只前进”的目标冲突。

### D3. 无暂存区，选择性提交

**核心模型**：提交的输入永远是工作区，但可以被 `<path>` 参数裁剪。没有持久化的待提交清单，因此没有 index、`add`、`rm`、`restore --staged`。

```
fodelta commit -m <msg>                    # 提交全部非忽略变更
fodelta commit -m <msg> -- src/ README.md  # 只提交这些路径
```

**算法**（`parent` = HEAD 的树，展平为 `path → (mode, oid)`）：

```
new = parent.clone()
if paths.is_empty():
    new = scan_worktree()                  # 全量快照
else:
    for p in normalized(paths):
        scope = resolve(p)                 # file | dir | tracked-deleted
        remove_all(new, under(scope))
        insert_all(new, scan_worktree(under(scope)))
tree = build_tree(new)
if tree == parent.tree && !allow_empty: error("nothing to commit")
```

- 未指定的路径保持父提交中的内容：即使工作区里改了、删了，也不会进入本次提交，并继续显示为未提交变更。
- 指定目录前缀时，该目录下所有非忽略文件以及已提交文件按工作区状态处理（新增、修改、删除）。
- 指定文件被删除且已跟踪时，记录删除。
- 未跟踪且被忽略的文件被**显式**指定时直接报错，并提示在 `.gitignore` 中加入 `!path`；目录前缀扫描仍遵守忽略规则。
- 路径参数按路径组件前缀匹配（`src` 匹配 `src/...`，不匹配 `src2`），不支持 glob；拒绝绝对路径与 `..`。
- 指定路径既不在 `HEAD` 也不在工作区时，报“pathspec did not match any files”。
- 选择性提交完成后输出“本次提交涉及 N 个路径、剩余 M 个未提交变更”，避免用户误以为其余修改也被带上。

未选中的路径不会被写入任何 exclude 或忽略记录；下一次不带路径的提交只要这些路径未被 `.gitignore` 匹配，就会按普通变更一起提交。

**为什么不用 index**：用户要的是“这次只提交这几个文件”，而不是“维护一份独立的暂存树”。路径参数是一次性输入，不需要跨命令持久化，也就没有三态状态、取消暂存、index 损坏恢复等一整套概念。代价是不支持行级挑选、也无法先构建好再分多次提交；这些留作后续能力。

**stat-cache 不是暂存区**：`.fodelta/stat-cache` 只记录 `path → (oid, size, mtime, inode, mode)`，用于跳过未变化文件的内容哈希。它不决定提交内容，提交永远以工作区扫描结果为准。

### D4. 哈希、压缩与对象存储

- 哈希：BLAKE3，32 字节（64 位十六进制）。显示与输入使用 10 位短哈希，解析时要求至少 6 位且唯一。
- 压缩：zstd，默认级别 3（`core.compression` 可调），启用 frame checksum。
- 布局：loose object，路径 `objects/<hex[0..2]>/<hex[2..]>`。
- 对象文件内容 = `zstd(header || payload)`，其中 `header = "<type> <len>\n"`。
- 读取时校验：解压长度、header 类型/长度、`blake3(header||payload)` 与文件名一致；任一不符即报 `corrupt object`。

选择 BLAKE3 而非 SHA-1：更快、无已知碰撞攻击面；64 位十六进制带来的显示长度问题用 10 位短哈希缓解。备选 SHA-256：兼容性更好，但本项目无互操作需求。

**格式**

blob payload 就是原始字节。tree payload：

```
u8   version = 1
u32  entry_count
repeat entry_count:
  u32   mode       # 0o040000 / 0o100644 / 0o100755 / 0o120000
  u8    kind       # 0 = tree, 1 = blob
  [32]  oid
  u16   name_len
  bytes name       # 不含 '/'
```

commit payload 为 UTF-8 文本：

```
tree <64-hex>
parent <64-hex>            # 根提交没有该行
author Ada <ada@x> 1780000000 +0800
fodelta 1

<subject>

<body>
```

`fodelta 1` 是提交格式版本；未知版本直接报错，不做猜测性解析。

### D5. 磁盘布局与文件格式

```
.fodelta/
├── HEAD                     # "ref: refs/heads/main\n"
├── config.toml              # 仓库配置（含 format_version）
├── objects/ab/cdef...       # loose objects
├── refs/heads/main          # 64-hex 提交 id；首次提交前不存在
├── stat-cache               # 工作区 stat 缓存（二进制，非暂存区）
├── logs/safety              # 安全快照记录
└── lock                     # 进程锁（常驻空文件，flock 语义）
```

引用文件内容为 `<oid>\n`；更新方式为“临时文件 + fsync + rename”，并在持有锁时校验旧值（CAS）。

**stat-cache** 二进制：

```
magic  "FDLTSTAT" (8)
u16    version = 1
u64    written_at_ns
u32    entry_count
repeat:
  u16 path_len; bytes path
  [32] oid
  u64 size
  i64 mtime_sec; u32 mtime_nsec
  u64 inode
  u32 mode
  u8  flags                 # 保留，当前为 0
[16] blake3_truncated(前面全部字节)
```

条目按路径字节序排序。它只用于加速，损坏时直接丢弃重建（输出警告），不影响提交正确性。

**safety log** `logs/safety` 每行：

```
<oid> <unix> <tz>\t<reason>
```

它不是引用，不会产生用户可见的分支；`fodelta safety` 读取它并倒序展示。GC 在保留期内把它当作根。

**config.toml** 关键项（`init` 会写入按系统用户名/主机名推导的 `[user]` 默认值，可随后用 `fodelta config` 修改）：

```toml
format_version = 1

[user]
name = "Ada"
email = "ada@example.com"

[core]
compression = 3
max_file_size_mb = 512
allow_large_files = false
prune_age_days = 14
safety_retention_days = 30
fsync = "refs"              # refs | all | none

[diff]
context = 3
algorithm = "myers"         # myers | patience
max_inline_bytes = 8388608
color = "auto"
```

### D6. 忽略规则

唯一忽略来源是仓库内各级 `.gitignore`。`WalkBuilder` 关闭 `.ignore`、git 的 `info/exclude` 与全局忽略，只保留 `git_ignore`；不提供 fodelta 自己的 exclude 文件。这样忽略规则始终随项目一起版本化，没有第二处隐藏状态。关键语义：

- 未被跟踪且被忽略 → 不出现在 `status`、不进入提交、不能被显式 `commit -- <path>` 选中（需在 `.gitignore` 中加 `!path`）。
- **已提交文件不受忽略影响**：以 `HEAD` 中的路径为准，即使其后被新规则匹配，仍然参与状态、diff 与提交。
- `.fodelta/`、`.git/`、嵌套 `.fodelta/` 永远排除，不可被 `!` 取反。
- 忽略目录整体跳过；若其中存在已跟踪文件，则只遍历这些已知路径。

`ignore check <path>` 使用 `Gitignore::matched_path_or_any_parents` 给出命中的规则来源；无法精确到行号时退化为“来源文件 + 模式”。

### D7. 差分

三种比较视角：

| 调用 | 左侧 | 右侧 | 用途 |
|------|------|------|------|
| `fodelta diff` | `HEAD` | 工作区 | 整个仓库与工作区的全部差异（默认） |
| `fodelta diff <rev>` | `<rev>` | 工作区 | 与某个历史版本比较 |
| `fodelta diff <rev1> <rev2>` | `<rev1>` | `<rev2>` | 历史比较 |

**树级 diff**：对两棵已排序的 tree 做归并扫描，按路径前缀递归，得到 `Added / Deleted / Modified / TypeChanged` 列表。工作区作为一侧时，构造内存中的 `path → (oid, mode)` 映射，复用 stat-cache 避免无关文件重算哈希，不需要写对象。

**文本 diff**：

- 二进制判定：前 8000 字节含 NUL，或不是合法 UTF-8 → 按二进制处理。
- 使用 `similar`：默认 Myers；`diff.algorithm = patience` 时切换。按 `\n` 切分但保留每行原始终止符（含 CRLF），不直接使用会丢弃 `\r` 的 `str::lines`。
- 输出 unified 格式，默认 3 行上下文；`--stat` 用同一份 hunk 统计增删行数。
- 大文件保护：超过 `diff.max_inline_bytes`（默认 8 MiB）时只输出二进制式提示。
- 路径参数按路径组件前缀匹配，不支持 glob。

**重命名**：v1 不做相似度检测，统一显示为删除 + 新增；`--stat` 仍按路径统计。

### D8. 回退、安全快照与 safety

没有 `reset`。两种操作覆盖“回到过去”：

| 命令 | 历史 | 工作区 | 用途 |
|------|------|--------|------|
| `restore [<rev>] [-- path]` | 不变 | 被覆盖 | 取回旧文件，之后可审查再提交 |
| `rollback <rev>` | 追加新提交 | 同步为 `<rev>` | 整体回退，历史保留 |

**restore 细则**

- 默认源为 `HEAD`；`restore <rev>` 指定历史版本。
- 只改工作区，不改历史。
- `--delete`：删除目标范围内“存在于工作区但不存在于源”的文件，并清理因此产生的空目录。
- 路径必须位于仓库根内，拒绝绝对路径、`..` 与符号链接穿透。
- 恢复后更新 stat-cache；无法写入时降级为下次重新哈希。

**rollback 细则**

- 工作区脏 → 先创建安全快照（`--no-safety` 可关闭并警告）。
- 新提交的树 = `<rev>` 的树、父 = 当前 `HEAD`；随后把工作区同步到该树（删除多余文件，保留未被跟踪的忽略文件）。
- 目标树与 `HEAD` 树相同：若工作区也已一致，输出 “nothing to rollback”；否则仅同步工作区（必要时先建安全快照），不产生新提交。这样回退中断后重跑可以收敛。

**安全快照**

- 仅在 `restore` 或 `rollback` 会覆盖未提交的工作区内容时创建。
- 安全快照遵循与提交相同的忽略规则与大文件上限；遇到超限文件时命令 MUST 拒绝执行并提示先忽略或调整上限，而不是静默丢弃。
- 快照提交的 parent = 当前 `HEAD`，tree = 当前工作区（忽略规则生效），记录写入 `logs/safety`。
- 快照不进入 `refs/`，因此不会产生用户可见的分支；`fodelta safety` 列出短哈希、时间与原因，`fodelta restore <safety-id> --delete` 可整体找回。

**GC 根集合**：`HEAD` 的完整祖先链、保留期内 `logs/safety` 的记录。没有 reflog、没有 merge state、没有 index，可达性分析只有这两类根。

### D9. 原子性、锁与持久化

- **锁**：所有写命令（`commit`、`restore`、`rollback`、`gc`）持有 `.fodelta/lock` 的排他锁（`std::fs::File::try_lock`，Rust 1.89+）。进程崩溃时内核自动释放。读命令（`status` / `log` / `show` / `diff` / `safety`）不加锁、不写 stat-cache。
- **对象写入**：先写同目录临时文件，再 rename；对象不可变，重复写入直接跳过。
- **stat-cache 写入**：锁内“读旧 → 合并 → 写临时文件 → fsync → rename”；checksum 覆盖全部内容。
- **引用更新**：锁内校验旧值 → 写临时文件 → fsync → rename → fsync 父目录。
- **进程崩溃与掉电要分开看**：对象先于引用写入，进程被 `kill -9` 时已写入的数据仍在内核页缓存中可读，引用不会指向半成品；掉电是否丢失对象取决于 fsync。
- **持久化策略**（`core.fsync`）：
  - `refs`（默认）：fsync 引用与目录，不逐个 fsync 对象。可抵御进程崩溃，不保证掉电后对象数据落盘；掉电后由 `fsck` 报告。
  - `all`：每个新对象都 fsync，可抵御掉电，但大提交明显变慢。
  - `none`：完全交给操作系统，适合临时仓库。
- `rollback` 不是跨文件事务：崩溃后可能停在“HEAD 已前移、工作区只恢复了一部分”的状态；由于新提交与安全快照都已落盘，重新执行 `rollback` 或从 safety 恢复即可收敛。

### D10. 性能策略

1. **stat-cache 快路径**：`status` / `diff` 对每个文件只做 `lstat`；元数据与缓存一致时直接信任缓存中的 oid，不读文件。
2. **racy 保护**：缓存记录 `written_at_ns`；若文件 mtime 不早于缓存写入时间，则强制重算哈希。
3. **选择性提交的局部扫描**：有 `<path>` 时只遍历选中范围，不扫描整个仓库。
4. **树级复用**：提交时只重建发生变化目录的 tree，未变化子树直接复用父提交的 oid。
5. **并行**：blob 的哈希/压缩、恢复时的文件写入使用 rayon；并发度默认等于 CPU 核数，`--jobs` 可调。
6. **流式大文件**：超过 8 MiB 的文件使用流式哈希 + 流式 zstd，避免整文件读入内存；超过 512 MiB 默认拒绝（可配置）。
7. **目标**（SSD，2 万文件）：

| 操作 | 目标 |
|------|------|
| `status`（无变化，热缓存） | < 150 ms |
| `commit`（100 个变更文件） | < 400 ms |
| `commit -- src/`（局部范围） | 与范围大小线性，不随整仓增长 |
| `log -n 50`（1 万提交） | < 50 ms |
| `diff`（单文件 5 千行） | < 50 ms |
| `restore`（1 千文件） | < 1 s |

### D11. CLI 与退出码

```
fodelta [-C <dir>] <command>

init [path]
status [--ignored]
commit -m <msg> [--allow-empty] [--allow-large-files] [-- <path>...]
log [--oneline] [-n N] [--stat] [--reverse] [-- <path>]
show <rev>[:<path>]
diff [<rev1> [<rev2>]] [--stat|--name-status|--name-only] [-U N] [--exit-code] [-- <path>]
restore [<rev>] [--delete] [--no-safety] [-- <path>]
rollback <rev> [-m <msg>] [--no-safety]
safety
ignore list | check <path>
gc [--dry-run]
fsck [--full]
config [<key> [<value>]]
```

退出码：

- `0`：成功；无差异。
- `1`：有差异（仅 `diff --exit-code`）。
- `2`：运行期错误（I/O、非仓库、对象损坏、锁占用、路径不匹配）与命令行用法错误。

输出默认面向人读，路径相对仓库根，颜色遵循 `--color` / `NO_COLOR` / TTY 检测；错误写 stderr，正常结果写 stdout。全局选项包括 `-C <dir>`、`--color <auto|always|never>`、`--jobs <N>` 与 `-q/--quiet`。

### D12. 代码结构与依赖

单 crate，核心逻辑在 lib，`main.rs` 只做参数解析与错误呈现：

```
src/
├── main.rs / lib.rs / cli.rs
├── repo/        # discover, init, config, refs, lock
├── odb/         # object, blob, tree, commit, store
├── worktree/    # walk, stat_cache, status, restore
├── history/     # commit（全量/选择性）, revparse, log, show
├── diff/        # tree, text, render
├── safety.rs    # 安全快照与 logs/safety
├── maintenance/ # gc, fsck
├── ignore.rs
└── util/        # atomic, path, time, hex
```

依赖（版本以 Cargo.lock 为准）：

| crate | 用途 |
|-------|------|
| `clap` 4 | CLI |
| `blake3` | 内容哈希 |
| `zstd` | 对象压缩 |
| `ignore` | gitignore 遍历与匹配 |
| `similar` | 文本 diff（Myers / Patience） |
| `toml` + `serde` | 配置 |
| `chrono` | 时间格式化与时区 |
| `rayon` | 并行哈希/写入 |
| `thiserror` / `anyhow` | 错误类型（lib 用 thiserror，bin 用 anyhow） |
| `anstream` / `anstyle` | 颜色与 TTY 适配 |
| `indicatif` | 大操作进度条 |
| `tempfile`（dev） | 测试临时目录 |
| `assert_cmd` / `predicates`（dev） | CLI 集成测试 |
| `proptest`（dev） | 编解码性质测试 |
| `criterion`（dev） | 基准 |

MSRV 1.89（`std::fs::File::try_lock` 稳定），edition 2024。**不依赖 `diffy`**。

### D13. 测试策略

- **单元**：对象编解码往返、tree 排序与非法名拒绝、stat-cache 往返/截断重建、路径校验、diff hunk 行号。
- **集成**：用 `assert_cmd` + 临时目录跑完整工作流（init → status → commit（全量/选择性）→ log → diff 三模式 → restore → rollback → safety → gc），覆盖 `.gitignore` 嵌套、已提交例外、选择性提交后剩余变更。
- **性质**：任意路径集合 → tree 编码 → 解码 → 展平，结果一致；`blob` 哈希稳定；选择性提交的树等于“父树 + 选中路径覆盖”。
- **故障注入**：提交中途删除临时文件/截断对象/截断 stat-cache，验证 `fsck` 报告、缓存重建与仓库仍可打开。
- **平台**：Linux / macOS / Windows CI；Windows 上跳过符号链接与可执行位相关用例，改为显式文档说明。
- **基准**：2 万文件合成场景，纳入 CI 的 `--release` 手动任务。

## Risks / Trade-offs

- **选择性提交的语义容易误解**：用户可能以为“未指定路径的修改也会被带上”。文档与 `commit` 输出必须明确“本次提交包含 N 个路径、剩余 M 个未提交变更”，并在 `status` 中保持可见。
- **无暂存区** → 无法行级挑选，也无法先构建好再分多次提交；`commit -- <path>` 覆盖文件/目录粒度，后续可加 `-p`。
- **无合并** → 需要组合两边修改时只能手工编辑或 `restore` 后再改；作为后续独立能力，不影响磁盘格式。
- **历史只增不减** → 仓库会持续增长；通过 GC 与去重控制体积，未来 packfile 进一步优化。
- **stat-cache 失效或损坏** → 只影响性能，不影响正确性；重建即可。
- **自定义格式的长期兼容** → 所有格式带版本号；`fsck` 校验；编解码性质测试；拒绝未知版本而不是猜测。
- **默认不 fsync 对象** → 进程崩溃不会损坏引用，掉电可能丢失对象；`core.fsync = all` 提供掉电级强保证。
- **Windows 语义差异** → 大小写不敏感、无执行位、符号链接权限；在文档与测试中显式限定。
- **安全快照增长仓库** → 默认 30 天保留期 + GC；快照与正常提交共享 blob/tree，增量成本接近零。

## Migration Plan

全新项目，无历史数据需要迁移。格式演进规则：

1. `config.toml` 的 `format_version` 与各文件的 `version` 字段独立递增。
2. 低版本二进制遇到高版本仓库必须拒绝打开；高版本二进制需内置对低版本的只读或迁移路径。
3. 未来引入 packfile 时，以“新增对象后端”的方式实现，loose object 保持可读，迁移通过 `fodelta gc --repack` 完成（独立变更）。

## Open Questions

- 默认压缩级别 3、`prune_age_days = 14`、`safety_retention_days = 30` 需要用真实目录基准校准；这些只是默认值，不影响规格。
- `commit` 在未提供 `-m` 时是否打开 `$EDITOR`（或从 stdin 读取）属于交互层增强，可在不改变提交语义的前提下后续补充。
- `commit -p` 行级选择、`diff` 的 glob 路径匹配属于后续增强，不改变本期的对象与存储格式。
