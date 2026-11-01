# fodelta CLI

## 全局选项

```text
fodelta [-C <DIR>] [--color auto|always|never] [--jobs N] [-q] <command>
```

- `-C <DIR>`：以指定目录为起点查找仓库。
- `--color`：彩色输出策略，遵循 `NO_COLOR`。
- `--jobs`：并行任务数，默认等于 CPU 数。
- `-q`：减少提示输出。

## init

```bash
fodelta init            # 在当前目录初始化
fodelta init notes/     # 在 notes/ 初始化
```

创建 `.fodelta/`（`HEAD`、`config.toml`、`objects/`、`refs/heads/`、`logs/`），不创建首个提交。
不允许在已有仓库内部初始化。

## status

```bash
fodelta status
fodelta status --ignored
```

输出相对 `HEAD` 的新增、修改、删除；`--ignored` 额外列出被忽略的未跟踪文件。
工作区与 `HEAD` 一致时输出 `working tree clean`。

## commit

```bash
fodelta commit -m "checkpoint"                  # 提交全部非忽略变更（含新文件）
fodelta commit -m "only docs" -- docs/ README.md
fodelta commit -m "empty" --allow-empty
```

- 不带路径：全量快照，所有未跟踪且未忽略的新文件都会进入提交。
- 带路径：只提交指定文件或目录前缀下的变更；其他修改保留在工作区，不会被记录成任何排除状态。
- 未跟踪且被忽略的文件即使显式指定也会被拒绝，需在 `.gitignore` 中加入 `!path`。
- 提交信息不能为空。
- 超过 `core.max_file_size_mb` 的文件需要 `--allow-large-files`。

选择性提交会输出本次涉及的文件数和剩余未提交变更数。

## log

```bash
fodelta log
fodelta log --oneline
fodelta log -n 10 --stat
fodelta log --reverse
fodelta log -- src/
```

单链历史、新到旧；`-- <path>` 仅显示触及该路径的提交。
空仓库执行 `log` 会提示当前分支尚无提交。

## show

```bash
fodelta show HEAD
fodelta show HEAD~2
fodelta show HEAD:src/main.rs
```

`show <rev>` 输出提交元数据与相对父提交的变更列表；`show <rev>:<path>` 把该 blob 原样写入标准输出。

## diff

```bash
fodelta diff                       # HEAD ↔ 工作区（整仓）
fodelta diff HEAD~2                # HEAD~2 ↔ 工作区
fodelta diff HEAD~3 HEAD           # 版本 ↔ 版本
fodelta diff --stat
fodelta diff --name-status
fodelta diff --name-only
fodelta diff -U0 -- src/
fodelta diff --exit-code
```

- 默认比较整个仓库的 `HEAD` 与工作区。
- 一个版本：该版本 ↔ 工作区；两个版本：版本 ↔ 版本。
- `--stat`、`--name-status`、`--name-only` 互斥。
- 文本输出 unified diff；二进制文件输出 `Binary files ... differ`。
- `--exit-code`：有差异返回 1，无差异返回 0；其他错误返回 2。
- 路径过滤使用 `-- <path>`，按路径组件前缀匹配，不支持 glob。

## ignore

```bash
fodelta ignore list
fodelta ignore check target/debug/app
```

只看项目内的 `.gitignore`。详见 [ignore.md](ignore.md)。

## config

```bash
fodelta config                     # 打印完整配置
fodelta config user.name           # 读取
fodelta config user.name "Ada"     # 修改并保存
```

常用键：`user.name`、`user.email`、`core.compression`、`core.max_file_size_mb`、
`core.fsync`、`diff.context`、`diff.algorithm`。

## 退出码

| 退出码 | 含义 |
|--------|------|
| 0 | 成功；无差异 |
| 1 | `diff --exit-code` 发现差异 |
| 2 | 运行期错误、路径错误、仓库损坏、用法错误 |

错误写 stderr，正常结果写 stdout。
