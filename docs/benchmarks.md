# Benchmarks

环境：Linux x86_64，release profile（`lto = "thin"`），`cargo bench -- --quick`。
数值是 criterion 的快速模式结果，用于回归对比，不作为最终性能承诺。

| 基准 | 规模 | 结果 |
|------|------|------|
| `render_unified_5k_lines` | 5,000 行单文件 diff | 约 140 µs |
| `status_1k_clean` | 1,000 文件、无变更、热 stat-cache | 约 4.1 ms |

按线性外推，2 万文件的干净 `status` 约 80 ms，低于设计目标 150 ms；正式验收见
`openspec/changes/archive/2026-11-01-add-fodelta-core/tasks.md` 的 9.3。

运行：

```bash
cargo bench --bench diff -- --quick
cargo bench --bench status -- --quick
```

## 20k 文件实测

`./scripts/perf-20k.sh`（release，20,000 个小文件，首次全量提交）：

```text
commit:   2.09 s     # 首次提交，需要哈希/压缩全部 20k 文件
status:   0.10 s     # 热 stat-cache，无变更
commit-x: 0.24 s     # 只提交 data/file-42.txt
log:      0.00 s     # log -n 50
```

`status` 低于 150 ms 目标；首次全量提交受文件数量与磁盘影响，明显高于“100 个变更文件
< 400 ms”的目标场景，但不属于同一基准。
