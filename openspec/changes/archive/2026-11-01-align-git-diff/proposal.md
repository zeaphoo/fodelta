# Proposal

## Why

fodelta 的 diff 已经能比较工作区、历史版本与两个版本，但输出与行为仍和 git 有明显差异：没有 `diff --git` / `index` / `/dev/null`、不显示 no-newline 标记、只识别 100% 相同的重命名、缺少空白忽略、histogram 与 `--diff-filter`。这会让习惯 git 的用户难以直接复用补丁与脚本。

本次变更把 diff 的输出格式、重命名检测、空白忽略、算法选择和状态过滤对齐到 git 的常用行为，同时保留 fodelta 不依赖 git 的实现。

## What Changes

- 文本补丁改为 git 风格：`diff --git`、`index`、`/dev/null`、单行 hunk 省略 `,1`、`\ No newline at end of file`。
- 类型变化拆成 deleted + new 两块；`--name-status` 仍显示 `T`。
- 重命名检测默认开启：先精确匹配（R100），再按行相似度（默认阈值 50）检测带修改的重命名。
- 新增 `-M/--find-renames`、`--rename-threshold`、`--no-renames`。
- 新增 `-w/--ignore-all-space`、`-b/--ignore-space-change`、`--ignore-space-at-eol`。
- 新增 `--diff-algorithm myers|patience|histogram`，并允许配置 `diff.algorithm = "histogram"`。
- 新增 `--diff-filter=ADMRT`。
- `show`、`log --stat` 复用同一套重命名与补丁渲染。
- 补充对应自动化测试与文档。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `diff`: 更新文本渲染、二进制/类型变化、汇总输出与退出码要求，并新增重命名、空白忽略、算法选择与状态过滤要求。

## Impact

- 代码：`src/diff/text.rs`、`src/diff/mod.rs`、`src/diff/render.rs`、`src/cli.rs`、`src/repo/config.rs`。
- 输出：补丁头与 hunk 格式变化，可能影响依赖旧格式的脚本；`--stat` 的汇总措辞改为单复数正确形式。
- 兼容：不修改对象格式、提交格式或仓库布局；旧仓库可直接使用。
- 限制：相似度分数是行级近似值，不保证与 git 的 similarity index 完全一致；不包含函数上下文、word-diff、`.gitattributes` 与复制检测。
