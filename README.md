# fodelta

本地优先、单分支、无远端的目录版本管理工具。用 Rust 写成，提供类 Git 的对象模型、
选择性提交、历史比较、可恢复回退与安全快照；不需要远端，也不引入暂存区、分支、
`reset` 或合并提交。

## 特性

- **选择性提交**：`fodelta commit -m "..." -- docs/ README.md` 只提交指定路径，其余修改留在工作区。
- **全量快照**：不带路径的 `commit` 记录全部非忽略变更，包括未跟踪的新文件。
- **只看 `.gitignore`**：不维护 `.fodelta/info/exclude`，不读取 `.ignore` 或全局 gitignore。
- **历史只前进**：没有 `reset`；回退用 `rollback` 追加新提交，旧历史始终可追溯。
- **可恢复**：覆盖未提交内容前自动创建 safety 快照，`fodelta safety` 可列出并恢复。
- **可靠存储**：BLAKE3 内容寻址、zstd 压缩、tree 去重、原子引用更新、进程锁、`fsck`/`gc`。
- **三种 diff**：工作区 ↔ HEAD、工作区 ↔ 版本、版本 ↔ 版本；Git 风格文件头、精确重命名、
  histogram、空白忽略与 no-newline 标记。

## 安装

```bash
cargo install --path .
fodelta --version
```

需要 Rust 1.89+。

## 快速开始

```bash
mkdir notes && cd notes
fodelta init

printf 'hello\n' > a.txt
fodelta status
fodelta commit -m "first"

printf 'hello world\n' > a.txt
printf 'b\n' > b.txt
fodelta status
fodelta commit -m "only a" -- a.txt      # 只提交 a.txt
fodelta status                            # b.txt 仍未提交

fodelta log --oneline
fodelta diff
fodelta diff HEAD~1 HEAD
fodelta rollback HEAD~1                   # 追加回退提交
fodelta safety
```

## 命令

| 命令 | 作用 |
|------|------|
| `init` | 初始化仓库 |
| `status` | 工作区相对 HEAD 的状态 |
| `commit` | 全量或选择性提交 |
| `log` | 单链历史 |
| `show` | 提交详情 / 文件内容 |
| `diff` | 工作区、版本之间的差异 |
| `restore` | 把版本写回工作区 |
| `rollback` | 追加回退提交 |
| `safety` | 列出安全快照 |
| `ignore` | `.gitignore` 诊断 |
| `gc` | 可达性垃圾回收 |
| `fsck` | 完整性校验 |
| `config` | 仓库配置 |

完整参数见 [docs/cli.md](docs/cli.md)。

## 设计取舍

- **不做 index**：用户要的是“这次只提交这几个文件”，而不是维护独立的暂存树；路径参数
  一次性生效，不产生持久化排除状态。
- **不做 reset**：历史只前进，回退通过追加提交实现，避免不可达提交与 reflog 复杂度。
- **不做 merge**：单分支下没有分叉拓扑；需要组合旧版本时用 `restore` 手工处理，三方合并
  留作后续独立能力。
- **不引入 git 依赖**：对象格式、索引与引用都由本项目实现，保持小而可控。

## 项目结构

```text
src/
├── cli.rs        # 参数解析与命令分发
├── repo/         # 发现、初始化、配置、引用、锁
├── odb/          # blob/tree/commit 编解码与对象库
├── worktree/     # 扫描、stat-cache、status、restore
├── history/      # commit、log、show、revparse、rollback、tree map
├── diff/         # 树 diff、文本 diff、渲染
├── safety.rs     # 安全快照
├── maintenance/  # gc、fsck、可达性
├── ignore.rs     # .gitignore 诊断
└── util/         # 原子写、路径、十六进制、时间
```

设计与规格在 `openspec/`：
[proposal](openspec/changes/archive/2026-11-01-add-fodelta-core/proposal.md)、
[design](openspec/changes/archive/2026-11-01-add-fodelta-core/design.md)、
[tasks](openspec/changes/archive/2026-11-01-add-fodelta-core/tasks.md)、
[specs](openspec/specs)。

## 开发

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo bench --bench diff -- --quick
```

## License

MIT OR Apache-2.0
