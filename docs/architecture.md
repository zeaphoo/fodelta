# 架构

## 总览

```text
工作区 ──扫描/忽略──▶ 内存 path→(mode, oid) ──build_trees──▶ tree/commit 对象
   │                                                        │
   └── stat-cache（仅缓存）                                  └── refs/heads/main
```

## 对象模型

- **blob**：文件字节；符号链接以目标文本作为内容。
- **tree**：单层目录，条目按名称字节序排序；条目含 mode、kind、oid、name。
- **commit**：tree + 唯一 parent + author + 版本头 + UTF-8 消息。

对象先编码为 `<type> <len>\n<payload>`，再用 BLAKE3 计算 oid，最后 zstd 压缩存储：
`.fodelta/objects/<oid[0..2]>/<oid[2..]>`。

## 磁盘布局

```text
.fodelta/
├── HEAD                 # ref: refs/heads/main
├── config.toml          # format_version = 1
├── objects/             # loose objects
├── refs/heads/main      # 提交 oid
├── stat-cache           # path → (oid, stat)，纯缓存
├── logs/safety          # 安全快照记录
└── lock                 # 进程锁
```

## 提交

没有持久化 index。`commit` 以工作区为输入：

- 不带路径：扫描全部非忽略文件（含未跟踪新文件）。
- 带路径：以父树为基底，只替换选中路径的工作区状态；未选中路径保持父提交内容。
- 忽略规则只来自各级 `.gitignore`；已提交文件优先于后续忽略规则。
- 结果树与父树相同则拒绝，`--allow-empty` 除外。

## 状态与 diff

`status` 比较 HEAD 树与工作区：新增 / 修改 / 删除。stat-cache 仅用于跳过未变化文件的
哈希；`status` 本身不写缓存。

`diff` 有三种视角：

- `diff`：HEAD ↔ 工作区
- `diff <rev>`：版本 ↔ 工作区
- `diff <rev1> <rev2>`：版本 ↔ 版本

树级比较按路径归并；文本使用 `similar`（Myers/Patience）生成 unified diff；二进制按
NUL/UTF-8 判定，只输出提示。

## 回退与安全

- `restore`：把版本写回工作区，不改历史。
- `rollback`：追加一个树等于目标版本的新提交，再同步工作区；历史只增不减。
- 没有 `reset`、detached HEAD、分支或合并提交。
- 覆盖未提交内容前生成 safety commit，记录在 `logs/safety`；GC 在保留期内将其视为根。

## 不变量

- 提交不可变；`refs/heads/main` 只移动到以当前 HEAD 为父的新提交。
- 每个非根提交恰好一个 parent。
- tree 条目严格按名称字节序排列，名称不能为空、`.`、`..`、含 `/` 或 NUL。
- 路径为相对仓库根的 `/` 分隔字节串；恢复拒绝绝对路径、`..` 与符号链接穿透。

## 可靠性

- 写命令持有 `.fodelta/lock` 排他锁；进程崩溃自动释放。
- 对象先写、引用后更新；引用更新使用临时文件 + fsync + rename。
- `core.fsync = refs|all|none` 控制对象持久化强度。
- `fsck` 校验引用、父链、树、blob 哈希；`gc` 按 HEAD 祖先链与保留期内 safety 做可达性回收。
