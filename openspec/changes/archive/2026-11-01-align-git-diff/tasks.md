# Tasks

## 1. 补丁格式

- [x] 1.1 输出 `diff --git`、`index`、`/dev/null` 文件头，并让单行 hunk 省略 `,1`；验证：`cargo test --test diff` 中的新增/删除与单行 hunk 用例通过
- [x] 1.2 在缺少结尾换行时输出 `\ No newline at end of file`；验证：`no_newline_at_end_of_file_marker` 通过
- [x] 1.3 把文件↔符号链接类型变化拆成 deleted + new 两块，`--name-status` 保持 `T`；验证：类型变化用例通过

## 2. 重命名

- [x] 2.1 在树级 diff 中配对 blob oid 相同的删除+新增，输出 `R100` 与 `similarity index 100%`；验证：`exact_renames_are_detected` 通过
- [x] 2.2 实现行级相似度与阈值，支持 `-M`、`--rename-threshold`、`--no-renames`；验证：`similarity_renames_and_threshold_flags` 通过
- [x] 2.3 带内容修改的重命名输出 `index` 与 hunk；验证：`show_similarity_rename_with_edits` 通过
- [x] 2.4 路径过滤同时匹配重命名旧路径与新路径；验证：`rename_name_only_and_path_filter_use_new_path` 通过

## 3. 空白与算法

- [x] 3.1 实现 `-w`、`-b`、`--ignore-space-at-eol`，仅空白差异时 patch/stat 为空且 `--exit-code` 为 0；验证：`whitespace_options_ignore_only_whitespace_changes` 与 `space_change_and_eol_options` 通过
- [x] 3.2 接入 `similar::Algorithm::Histogram`，支持 `--diff-algorithm` 与 `diff.algorithm` 配置；验证：`histogram_algorithm_is_selectable` 与 `histogram_can_be_selected_through_config` 通过

## 4. 过滤与集成

- [x] 4.1 实现 `--diff-filter=ADMRT` 并在重命名检测之后过滤；验证：`similarity_renames_and_threshold_flags` 与 `diff_filter_rejects_unknown_letters` 通过
- [x] 4.2 让 `show` 与 `log --stat` 复用重命名检测与补丁渲染；验证：`show_and_log_stat_report_renames` 与 `show_similarity_rename_with_edits` 通过
- [x] 4.3 更新 `docs/cli.md`、`docs/architecture.md`、`docs/spec-coverage.md` 与 README；验证：文档中的命令在 e2e 仓库可执行

## 5. 验证

- [x] 5.1 运行 `cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --all-targets`；验证：全部通过
- [x] 5.2 运行 `./scripts/e2e.sh`；验证：输出 `e2e: ok`

## Workflow follow-up

- 运行 `openspec validate align-git-diff --strict` 并修复告警。
- 归档变更并确认 `openspec/specs/diff/spec.md` 已更新。
