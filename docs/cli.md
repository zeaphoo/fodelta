# fodelta CLI

## 全局选项

```text
fodelta [-C <DIR>] [--color auto|always|never] [--jobs N] [-q] <command>
```

- `-C <DIR>`：以指定目录为起点查找仓库。
- `--color auto|always|never`：彩色输出策略；`auto` 仅在终端着色并尊重 `NO_COLOR`，显式 `always` 优先。
- `--jobs N`：提交时并行哈希/压缩的工作线程数，默认等于 CPU 数。
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
fodelta log -n 10 --stat      # 每文件 +N/-M 行级统计
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

Git 风格输出与选项：

- 文件头包含 `diff --git a/... b/...`、`index <old>..<new> <mode>`。
- 新增/删除文件使用 `--- /dev/null` / `+++ /dev/null`。
- 最后一行没有换行时输出 `\ No newline at end of file`。
- 单行 hunk 省略 `,1`，例如 `@@ -1 +1 @@`。
- 内容完全相同的删除+新增会识别为精确重命名：`R100`、`similarity index 100%`、
  `rename from` / `rename to`，`--stat` 显示 `old => new | 0`。
- 类型变化（文件 ↔ 符号链接）的补丁拆成 deleted + new 两块，`--name-status` 仍显示 `T`。
- 支持 `-w/--ignore-all-space`、`-b/--ignore-space-change`、
  `--ignore-space-at-eol`；仅空白差异时 patch/stat 为空、`--exit-code` 返回 0，
  但 `--name-status` 仍会列出该文件（与 git 一致）。
- `--diff-algorithm myers|patience|histogram`；也可配置 `diff.algorithm`。
- 重命名检测目前只覆盖内容完全相同的精确重命名；内容有修改的移动仍显示为 D + A。

## restore

```bash
fodelta restore -- a.txt              # 工作区从 HEAD 恢复 a.txt
fodelta restore HEAD~2 -- src/        # 从历史版本恢复目录
fodelta restore HEAD~1 --delete       # 整棵树恢复，并删除目标版本中不存在的文件
fodelta restore --no-safety -- a.txt
```

- `restore <rev>` 缺省使用 `HEAD`。
- 只改工作区，不改历史。
- 路径放在 `--` 之后，避免与 `<rev>` 位置参数混淆。
- 会覆盖未提交内容时自动创建安全快照；`--no-safety` 关闭并警告。
- 路径必须位于仓库内，拒绝 `..` 与绝对路径。

## rollback

```bash
fodelta rollback HEAD~2
fodelta rollback HEAD~2 -m "回到上周版本"
fodelta rollback HEAD                # 干净时 no-op；脏时仅同步工作区
```

追加一个新提交，其树等于目标版本，历史不丢失。工作区会被同步为目标树的精确状态
（保留未跟踪的忽略文件）；有未提交修改时先创建安全快照。

## 未提交修改与破坏性操作

`rollback` 与 `restore` 会覆盖工作区，规则如下：

- 工作区有未提交修改（含未跟踪的新文件）时，先创建 safety 快照，再执行操作。
- `rollback` 把工作区同步为目标树的精确状态：删除多余的非忽略文件，保留被忽略的未跟踪文件。
- 如果目标版本中的某个路径，在当前工作区是一个被忽略的未跟踪文件，fodelta 会把该文件也写入
  safety 快照，避免被覆盖后无法找回。
- 如果目标路径是文件、而当前工作区同名路径是目录，操作会被拒绝（`cannot replace directory with file`），
  不会删除目录内容。
- `--no-safety` 跳过快照并输出警告；此时被覆盖的修改可能无法恢复。
- 恢复方式：`fodelta safety` 找到短哈希，然后 `fodelta restore <safety-id> --delete`。
- safety 快照默认保留 `core.safety_retention_days`（30 天），`gc` 会裁剪过期记录。

### rollback 之后继续提交

回滚不是从旧提交拉分支，也不会删除旧历史，而是追加一个“树等于目标版本”的提交：

```text
c1 <- c2 <- c3 <- R(rollback c1) <- C(修改后提交)
```

- `R` 的父提交是 rollback 前的 tip（示例中是 `c3`），所以 `HEAD~1 = R`、`HEAD~2 = c3`、
  `HEAD~3 = c2`、`HEAD~4 = c1`。
- 之后提交的 `C` 的树 = `c1` 的树 + 新修改。
- 想比较“回到 c1 后改了什么”，用 `fodelta diff <c1> HEAD`，不要用 `HEAD~2`。
- `c2`、`c3` 仍然是 `R` 的祖先，`log` 能看到，`gc` 不会回收。

## safety

```bash
fodelta safety
```

按时间倒序列出安全快照：短哈希、时间、原因。安全快照不在 `refs/` 下，不会出现在
`log` 中，但可以用 `fodelta show <id>` 查看、用 `fodelta restore <id> --delete` 整体恢复。

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
