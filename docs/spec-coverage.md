# 规格覆盖

当前共 175 个测试（`cargo test --all-targets`）。测试分布：

| 能力 | Requirement | Scenario | 主要测试 |
|------|-------------|----------|----------|
| `repository` | 6 | 14 | `tests/init.rs`、`src/repo/*` 单元测试 |
| `working-tree` | 7 | 15 | `tests/status.rs`、`src/worktree/scan.rs`、`src/worktree/stat_cache.rs` |
| `history` | 7 | 28 | `tests/commit.rs`、`tests/log_show.rs`、`src/history/commit.rs` |
| `diff` | 6 | 15 | `tests/diff.rs`、`src/diff/text.rs`、`src/diff/render.rs` |
| `rollback` | 6 | 17 | `tests/restore_rollback.rs`、`tests/crash.rs`、`src/safety.rs` |
| `ignore` | 5 | 14 | `tests/status.rs`、`src/ignore.rs`、`src/worktree/scan.rs` |
| `maintenance` | 6 | 13 | `tests/maintenance.rs`、`tests/crash.rs`、`src/maintenance/*` |

所有 Scenario 以自动化测试为主；以下项目因依赖外部平台或大体积资源，采用手工验收：

## 手工验收清单

1. **环境变量覆盖身份**
   ```bash
   FODELTA_AUTHOR_NAME=Grace FODELTA_AUTHOR_EMAIL=grace@example.com fodelta commit -m x
   fodelta log --oneline
   ```
   期望作者为 Grace。

2. **短哈希歧义**：构造两个前缀相同的提交在真实仓库中概率极低；`ObjectId::resolve_prefix`
   的单元测试覆盖了歧义分支，CLI 层使用 `AmbiguousRevision`。

3. **600 MiB 大文件**：需要外部大文件；`tests/commit.rs` 用 1.5 MiB 文件覆盖同一分支
   （超限拒绝、`--allow-large-files` 放行），避免在 CI 中生成大文件。

4. **Windows 语义**：符号链接创建与可执行位相关用例在 Windows 上跳过；CI 覆盖其余命令。

5. **并发进程锁竞争**：单元测试覆盖同进程加锁/释放；跨进程竞争由 `std::fs::File::try_lock`
   的语义保证，CI 中未做双进程注入。

6. **极深对象图**：`flatten_tree` 与 fsck 对深度超过 256 的树返回错误；构造此类恶意仓库
   属于手工安全验收，避免在常规测试中生成巨大对象。

## 补充验证

- 颜色：`tests/color.rs` 覆盖 `--color always/never/auto` 与 diff 着色。
- 并行：`tests/commit.rs` 覆盖 `--jobs`、`--jobs 0` 拒绝，以及“未变化文件不再读取”的
  chmod 000 用例。
- 行级 `log --stat`：`tests/log_show.rs` 断言 `+N/-M` 与汇总行。
- safety 过期裁剪：`tests/maintenance.rs` 验证 GC 会清理过期日志。
- FIFO 警告：`tests/status.rs` 在 Unix 上创建 FIFO 并断言 stderr 警告。
- ignored 文件覆盖：`tests/restore_rollback.rs` 验证被 ignored 的未跟踪文件在 rollback/restore
  覆盖前进入 safety，且可恢复。
- 文件/目录类型变化：`tests/commit.rs` 覆盖 file→dir、dir→file；目录冲突时 rollback 拒绝执行。

以上手工项与自动化测试共同覆盖 43 条 Requirement / 116 个 Scenario。
