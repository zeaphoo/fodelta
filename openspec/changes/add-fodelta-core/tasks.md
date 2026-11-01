# Tasks

## 1. 脚手架与基础

- [ ] 1.1 创建 `fodelta` crate（lib + bin，edition 2024，MSRV 1.89），配置 `[profile.release]`；验证：`cargo build` 成功且 `target/debug/fodelta --version` 输出版本号
- [ ] 1.2 加入运行期与开发依赖（clap、blake3、zstd、ignore、similar、toml、serde、chrono、rayon、thiserror、anyhow、anstream、anstyle、indicatif；dev：assert_cmd、predicates、tempfile、proptest、criterion；不引入 diffy）；验证：`cargo build --all-targets` 通过且 `Cargo.lock` 被提交
- [ ] 1.3 搭建 CLI 骨架：13 个子命令、全局 `-C` / `--color` / `--jobs` / `--quiet`、帮助文本；验证：`cargo run -- --help` 列出 init/status/commit/log/show/diff/restore/rollback/safety/ignore/gc/fsck/config，`cargo test cli_help` 通过
- [ ] 1.4 定义错误模型（`FodeltaError` 枚举 + 退出码映射），统一 stderr 呈现；验证：单元测试覆盖 `DiffFound` → 1、`NotFound`/`Corrupt`/`Usage` → 2
- [ ] 1.5 建立测试基建：临时仓库 fixture、`fodelta_cmd()` 辅助、`NO_COLOR`/`TERM=dumb` 固定；验证：`cargo test --test smoke` 中一个空测试通过
- [ ] 1.6 配置 CI：fmt、clippy `-D warnings`、Linux/macOS/Windows 测试矩阵；验证：本地 `cargo fmt --check && cargo clippy --all-targets -- -D warnings` 通过

## 2. 对象存储与仓库

- [ ] 2.1 实现 blob 写入/读取：BLAKE3、zstd（含 checksum）、大文件流式路径、读取长度上限；验证：单元测试覆盖空文件、1 字节、1 MiB、10 MiB 的哈希稳定与内容往返
- [ ] 2.2 实现对象头编解码（`<type> <len>\n`）与类型/长度校验；验证：单元测试拒绝超长、错误类型、哈希不匹配的对象并返回 `Corrupt`
- [ ] 2.3 实现 tree 编解码：版本字节、条目排序、非法名（`..`、`/`、NUL、空名）拒绝；验证：`cargo test tree_` 全绿，proptest 往返 1000 组随机树
- [ ] 2.4 实现 commit 编解码：tree/parent/author/`fodelta 1`/UTF-8 信息，根提交无 parent；验证：单元测试覆盖根提交、带 body、非法 UTF-8、未知格式版本
- [ ] 2.5 实现 ObjectStore：临时文件 + rename、已存在跳过、按 2 位扇出、`fsync` 策略（refs/all/none）；验证：同一内容写两次只产生一个文件，`cargo test odb_` 通过
- [ ] 2.6 实现 `init`、仓库向上发现、`HEAD`/refs 读写、配置加载与 `config` 子命令；验证：集成测试覆盖空目录初始化、重复初始化失败、子目录发现、嵌套仓库、未知配置键、版本过高报错
- [ ] 2.7 实现进程锁与引用 CAS 更新（旧值校验 + 原子 rename + 目录 fsync）；验证：并发两个提交只有一个成功，另一进程报告锁占用
- [ ] 2.8 实现用户身份解析（配置 → 环境变量覆盖 → `init` 默认值）与缺失时拒绝提交；验证：集成测试覆盖默认值、环境变量覆盖、邮箱为空时报错

## 3. 工作区与忽略

- [ ] 3.1 实现忽略遍历器：嵌套 `.gitignore`（不读 `.ignore` / `info/exclude` / 全局忽略）；验证：测试覆盖嵌套覆盖、取反、目录限定、`**` 与转义，且 `.ignore` 中的规则不生效
- [ ] 3.2 实现“已提交路径优先”与内置排除（`.fodelta/`、`.git/`、嵌套仓库不可取反）；验证：已提交文件在被忽略后仍参与状态，嵌套仓库整体跳过
- [ ] 3.3 实现 stat-cache 编解码、racy 时间保护、损坏重建；验证：`touch` 不产生修改，同尺寸快速改写能被发现，截断缓存后 `status` 正常
- [ ] 3.4 实现 `status` 分类（新增/修改/删除/忽略）与输出（clean、no commits yet、`--ignored`、总数）；验证：三态与忽略场景的输出快照测试通过
- [ ] 3.5 实现文件类型与模式：符号链接、可执行位、FIFO/socket 跳过；验证：Unix 测试覆盖 `chmod +x` 与 symlink，Windows 用例显式跳过
- [ ] 3.6 实现大文件保护（`commit` 超限拒绝、`--allow-large-files` 放行）；验证：600 MiB 稀疏文件默认被拒绝，加标志后成功
- [ ] 3.7 实现 `ignore list/check` 并编写 `docs/ignore.md`；验证：`ignore check target/debug/app` 输出命中规则来源，`ignore add` 为未知命令

## 4. 提交与历史

- [ ] 4.1 实现全量提交：扫描工作区（忽略感知 + 已提交例外）→ blob 并行哈希 → tree 自底向上构建/未变化子树复用 → commit → 引用 CAS；验证：提交 3 次后 `fsck` 通过，未变化子树 oid 与父提交一致
- [ ] 4.2 实现选择性提交：`commit -- <path>` 以父树为基底，只覆盖选中文件/目录前缀的工作区状态（含删除）；验证：提交 `a.txt` 后 `b.txt` 修改仍在 `status`，提交树等于“父树 + 选中覆盖”
- [ ] 4.3 实现选择性提交的边界：目录前缀、已提交文件删除、路径不匹配报错、显式选中被忽略文件报错；验证：对应集成测试逐条通过
- [ ] 4.4 实现空提交规则（默认拒绝、`--allow-empty` 允许）与提交信息 UTF-8 校验；验证：无变化返回非 0 且提示 nothing to commit，`--allow-empty` 成功
- [ ] 4.5 实现 `log`（`--oneline`、`-n`、`--stat`、`--reverse`、路径过滤、空仓库提示）；验证：构造 5 个提交，断言顺序、条数与路径过滤结果
- [ ] 4.6 实现 `show`（提交元数据 + 相对父提交差异）与 `show <rev>:<path>` 原字节输出；验证：集成测试逐字节比对二进制文件内容
- [ ] 4.7 实现 revparse：`HEAD`/`@`/`main`/`HEAD~N`/≥6 位短哈希，歧义与未知版本报错；验证：单元测试覆盖歧义前缀、越界 `~N`、未出生 HEAD
- [ ] 4.8 编写 `docs/cli.md` 的 commit/log/show 章节，说明“全量 vs 选择性提交”的语义；验证：文档中的两条提交工作流可执行，且 `fodelta reset` 返回未知命令

## 5. 差分

- [ ] 5.1 实现树级 diff：排序归并、递归目录、`Added/Deleted/Modified/TypeChanged`；验证：单元测试覆盖新增/删除/重命名式删除+新增/目录移动
- [ ] 5.2 实现文本 diff 与 unified 渲染：Myers/Patience、`-U N`、二进制判定、模式与类型变化行；验证：固定样例输出快照测试（含 CRLF 与无结尾换行）
- [ ] 5.3 实现三种比较视角：工作区↔`HEAD`、工作区↔版本、版本↔版本；验证：每种视角各一个集成测试，`diff HEAD HEAD` 无输出
- [ ] 5.4 实现 `--stat`、`--name-status`、`--name-only`、`--exit-code` 与互斥校验；验证：汇总行文本与退出码 0/1 的快照测试通过
- [ ] 5.5 编写 `docs/cli.md` 的 diff 章节与退出码说明；验证：文档示例命令输出与文本一致

## 6. 回退与安全

- [ ] 6.1 实现 `restore`（默认源 `HEAD`，覆盖工作区、不改历史）；验证：恢复被修改文件后 `status` 干净，`HEAD` 不变
- [ ] 6.2 实现 `restore <rev>`、路径范围与 `--delete` 清理空目录；验证：恢复旧版本后逐字节比对，`--delete` 后集合与目标树一致
- [ ] 6.3 实现恢复路径安全：拒绝绝对路径、`..`、逃逸根目录、透过符号链接写入；验证：安全测试不修改仓库外文件
- [ ] 6.4 实现 `rollback`：脏工作区安全快照、追加单亲提交、同步工作区（含删除多余文件）、目标等于 HEAD 时按需同步工作区且不产生重复提交；验证：历史长度 +1，内容一致，`rollback HEAD` 不产生提交，中断后重跑可收敛
- [ ] 6.5 实现安全快照：`logs/safety` 追加、快照哈希输出、`--no-safety` 警告、干净工作区不生成快照；验证：快照哈希可 `show`，`restore <safety-id> --delete` 完整找回
- [ ] 6.6 实现 `safety` 列表（倒序、短哈希、原因、坏行跳过）；验证：快照不出现在 `log`，`safety` 输出顺序正确
- [ ] 6.7 恢复/回退后刷新 stat-cache，保证 `status` 无虚假修改；验证：`restore HEAD` 与 `rollback` 后立即 `status` 均为 clean
- [ ] 6.8 编写 `docs/cli.md` 的 restore/rollback/safety 章节与“误操作恢复”指南；验证：按文档从 safety 恢复被覆盖的内容

## 7. 维护、发布与文档

- [ ] 7.1 实现 `gc`：可达性标记（HEAD 祖先链 + 保留期内 safety）、`prune_age_days`、`--dry-run` 统计；验证：无引用对象在保留期后删除，安全快照在期内保留，`fsck` 通过
- [ ] 7.2 实现 `fsck`：引用/父链/树结构/blob 哈希校验，`--full` 全量扫描，dangling 统计与损坏报告；验证：篡改 blob 后报告 `corrupt object` 并非 0 退出
- [ ] 7.3 添加崩溃注入测试（提交中断、临时文件残留、锁进程被杀、rollback 中断）；验证：中断后仓库可打开，`fsck` 通过，重跑 rollback 收敛
- [ ] 7.4 添加 criterion 基准（status/commit/commit 选择性/log/diff）并记录 2 万文件场景数值；验证：`cargo bench` 可运行，结果写入 `docs/benchmarks.md`
- [ ] 7.5 完善发布配置：release profile、shell completion、`cargo install` 说明、musl 构建脚本；验证：`cargo build --release` 产物可运行，completion 输出非空
- [ ] 7.6 完成 README（定位、安装、5 分钟上手、与 git/rustory 的差异、无 reset/merge 的设计取舍）与 `docs/architecture.md`；验证：新用户按 README 完成 init/commit（全量与选择性）/diff/rollback

## 8. 集成验收

- [ ] 8.1 编写端到端脚本覆盖完整生命周期（init → status → commit 全量/选择性 → log/show → diff 三模式 → restore → rollback → safety → gc → fsck）；验证：脚本在 Linux/macOS 通过，退出码为 0
- [ ] 8.2 验证规格覆盖：逐条对照 `specs/*/spec.md` 的 Scenario 建立测试映射表；验证：每个 Scenario 至少有一个自动化测试或明确的手工验收记录
- [ ] 8.3 性能验收：在 2 万文件合成目录上实测 `status`/`commit`/`log`/选择性提交，与设计目标对比；验证：数值记录在 `docs/benchmarks.md`，超标项有后续任务
- [ ] 8.4 安全验收：路径穿越、符号链接逃逸、恶意 tree/blob 输入；验证：`cargo test security` 全部通过

## Workflow follow-up

- 实现完成后运行 `openspec validate add-fodelta-core --strict` 并修复所有告警。
- 通过项目评审后归档变更：`openspec archive add-fodelta-core`。
- 归档后确认 `openspec/specs/` 下的主规格已生成且 Purpose 完整。
