# Design

## Context

现有 diff 规格（见 `openspec/specs/diff/spec.md`）只约束 unified diff 的基本形状。实现已经具备树级比较、Myers/Patience 文本 diff、二进制提示与 `--stat`，但输出与 git 仍有差距。本变更只调整 diff 层，不触碰对象、提交与仓库布局。

## Goals / Non-Goals

**Goals**

- 生成可被 git 用户直接阅读的补丁头与 hunk。
- 覆盖精确与近似重命名，并允许调整/关闭。
- 空白忽略、算法选择、状态过滤与 git 常用参数一致。
- 保持 `show` / `log --stat` 与 `diff` 使用同一套渲染。

**Non-Goals**

- 不实现函数上下文、`--word-diff`、`--color-moved`、`--check`。
- 不实现 `.gitattributes`、textconv、外部 diff。
- 不实现复制检测 `-C` 与 `--diff-filter` 的 C/U/X/B。
- 不改变二进制判定（非 UTF-8 仍按二进制），也不实现 pathspec glob。

## Decisions

### 1. 补丁头与 hunk

- 所有文件变更先输出 `diff --git a/<old> b/<new>`。
- 非重命名输出 `index <old>..<new> <mode>`；新增/删除缺失的一侧用 64 个 0 的短哈希。
- 新增/删除的文本头使用 `/dev/null`。
- hunk 范围：计数为 1 时只写起始行，其余写 `start,count`。
- `push_line` 在行缺少终止换行时追加 `\ No newline at end of file`。
- 类型变化在渲染层拆成 Deleted + Added 两个虚拟变更，从而复用现有文件头与 hunk 逻辑。

### 2. 重命名

- `diff_trees` 默认先做精确重命名：删除与新增的 blob oid 相同即配对，`similarity = 100`。
- 剩余的删除/新增由 `detect_renames` 按行相似度配对：
  - `score = 2 * equal_lines / (old_lines + new_lines) * 100`；
  - 默认阈值 50；
  - 二进制不参与近似匹配；
  - 先用文件大小比过滤，删除×新增超过 100,000 组合时只保留精确重命名。
- 重命名变更携带 `old_path` 与 `similarity`；`render` 对内容相同者只输出 `similarity`/`rename` 行，对内容变化者继续输出 `index` 与补丁。
- `--no-renames` 关闭精确与近似；`--rename-threshold` 调整阈值。

### 3. 空白忽略

- 在 `similar::capture_diff_slices_by_key` 上按行生成比较键：
  - `-w` 去掉所有空白；
  - `-b` 折叠空白为单个空格；
  - `--ignore-space-at-eol` 去掉行尾空白。
- 输出仍使用原始行。
- 若 patch/stat 在忽略空白后没有变化，则不输出该文件；`--exit-code` 返回 0；`--name-status` 仍列出文件（与 git 一致）。

### 4. 算法与过滤

- `DiffAlgorithm` 增加 `Histogram`，映射到 `similar::Algorithm::Histogram`。
- `--diff-algorithm` 覆盖配置；配置校验允许 `myers`/`patience`/`histogram`。
- `--diff-filter` 在重命名检测之后过滤 `A/D/M/R/T`，不支持字母直接报错。

## Risks / Trade-offs

- **相似度分数与 git 不同** → 行级近似，阈值附近可能得到不同 R 分数；文档明确说明，测试只断言稳定场景。
- **补丁头变化影响旧脚本** → 这是对齐目标；旧脚本若只匹配 `--- a/` 仍可用，新增行不影响。
- **重命名检测成本** → 先精确、再大小过滤、最后限制组合数；大仓库退化到只做精确重命名。
- **空白忽略的 `-b` 语义** → 使用 `split_whitespace` 折叠，和 git 的逐字符空白处理在极端情况下可能不同。
- **类型变化拆成两块** → `--stat` 仍按一个文件统计，和 git 的呈现可能有差异。

## Migration Plan

无数据迁移。补丁输出格式变化属于行为更新；旧仓库与历史对象无需修改。

## Open Questions

- 是否需要继续实现 `-M90` 的短参数写法（当前为 `-M` + `--rename-threshold 90`）。
- 是否在后续实现复制检测、函数上下文与 pathspec glob。
