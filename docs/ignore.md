# 忽略规则

fodelta 只读取项目内的 `.gitignore`，不维护 `.fodelta/info/exclude`，也不读取
`.ignore`、`.git/info/exclude` 或用户全局 git 忽略文件。

## 规则来源与优先级

- 仓库内各级 `.gitignore`，规则相对该文件所在目录。
- 距离文件更近的 `.gitignore` 优先；同一文件中后出现的规则覆盖先出现的规则。
- `.gitignore` 本身是普通文件，默认会被提交；如果不想提交它，需要另行忽略。
- fodelta 不会自动修改 `.gitignore`。

## 基本语法

```gitignore
# 注释
*.log
!important.log
build/
**/tmp/
\#literal
```

- `!` 取反，可用于重新纳入被忽略的文件。
- 结尾 `/` 只匹配目录。
- `**` 跨目录匹配。
- `\` 转义特殊字符。

## 已提交文件优先

已存在于 `HEAD` 的文件不受后续忽略规则影响：

```bash
fodelta commit -m "track build.log"
echo '*.log' >> .gitignore
fodelta status          # build.log 仍然被跟踪
```

只有从未被提交过的忽略文件才会被隐藏和跳过。

## 提交被忽略的文件

默认提交和 `commit -- <path>` 都不会提交未跟踪的忽略文件。显式指定时会报错：

```text
fodelta: path is ignored: secret.env (add '!secret.env' to .gitignore to track it)
```

需要跟踪时，在 `.gitignore` 中加一条取反规则：

```gitignore
!secret.env
```

## 诊断

```bash
fodelta ignore list                 # 列出生效的 .gitignore 与规则
fodelta ignore check target/app     # 解释某个路径是否被忽略
fodelta ignore check src/main.rs    # 输出 not ignored
```

`ignore check` 会输出命中的模式与来源文件。`ignore add` 不存在：忽略规则只能由用户编辑 `.gitignore`。

## 与 git 的差异

- 不支持 `.git/info/exclude`、全局 gitignore、`.ignore`。
- 不支持 `git add -f`；用 `.gitignore` 中的 `!path` 表达显式纳入。
- 内置排除 `.fodelta/`、`.git/` 与嵌套 fodelta 仓库，不能被取反。
