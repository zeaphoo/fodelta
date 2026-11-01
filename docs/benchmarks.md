# Benchmarks

环境：Linux x86_64，release profile（`lto = "thin"`），`cargo bench -- --quick`。
数值是 criterion 的快速模式结果，用于回归对比，不作为最终性能承诺。

| 基准 | 规模 | 结果 |
|------|------|------|
| `render_unified_5k_lines` | 5,000 行单文件 diff | 约 140 µs |
| `status_1k_clean` | 1,000 文件、无变更、热 stat-cache | 约 4.1 ms |

按线性外推，2 万文件的干净 `status` 约 80 ms，低于设计目标 150 ms；正式验收见
`openspec/changes/add-fodelta-core/tasks.md` 的 9.3。

运行：

```bash
cargo bench --bench diff -- --quick
cargo bench --bench status -- --quick
```
