# lo-md-lint

Markdown linter，从中文技术写作的排版规则起步。

## 定位与愿景

- **规则先于实现**：每条规则是一段语言无关的规范 (specification)，写在 `spec/rules.md`，有一个稳定的 id、可以逐条关掉；规则 id 按家族分前缀 —— `R` 中文排版、`A` AI 腔 (中英文同一家族)、`T` 术语选词 (ADR-0007)。中文排版规则 (中英文之间空格、数字与中文之间空格、半角括号外空格、全角标点……) 是默认规则集，AI 腔与术语选词是默认关闭的实验规则。
- **多实现、一套黄金 fixture (golden fixtures)**：黄金集在 `spec/fixtures/`，Python 版是参考实现 (reference implementation)，任何后续实现都对着同一套「输入 / 期望输出」跑，通过即合规。
- **对标 ruff 之于 Python**：长期大概率以 Rust 为主实现 —— 一个 Rust 写的 Markdown lint，可被 Python / Node 生态经 pre-commit、包管理器等集成，也能直接当命令行工具用。
- **配置走 toml**：独立配置文件 `lo-md-lint.toml` 或 `pyproject.toml` 的 `[tool.lo-md-lint]` 表，两者同构，用 `disable` / `enable` 两个键逐条开关规则；绝大多数规则默认启用，个别默认关闭的规则在规范条目里标明。每条规则另有可修复性 / 严重度 / 成熟度三个正交属性 (ADR-0006)，`severity` 覆盖单条规则的严重度、`enable_experimental` 一次纳入全部 experimental 规则。开关之外还有调整单条规则判定的键，当前是 R5 的 `skip_zh_units` (中文计量单位豁免，默认不豁免)。

决策记录在 `docs/adr/`；agent 守则在 `AGENTS.md`。

## 现状

Python 参考实现已就位。默认规则集是中文排版一套：宽度转换 (R1 CJK 旁的半角标点含句号、R2 全角括号、R10 全角数字)、空格 (R3 半角括号外侧、R4 CJK–拉丁字母、R5 CJK–数字、R6 数字–单位、R7 行内代码定界符、R8 破折号两侧、R11 全角标点旁去空格、R9 链接前，默认关)，全是 fixable · error · stable。另有一批默认关闭的实验规则：中文 AI 腔 A1 套话、A2 否定平行、A3 互联网黑话、A4 聊天残留，英文 tell A5 AI 词汇、A6 否定平行、A7 Claudish 专用词，与术语选词 T1 (ADR-0007)，全是 warning · experimental，判定用的词表在 `spec/wordlists/`。规则不分语言 (ADR-0006)：英文 tell 出现在中文文档里同样报。每条规则都可单独开关 (ADR-0003 / ADR-0004)，并按可修复性 / 严重度 / 成熟度三轴标注 (ADR-0006)。逃生口两个：行内指令按行 × 规则就地关掉，`.lo-md-lint-ignore` 把整份文件排除在输入之外。`spec/` 已建起来：规则规范在 `spec/rules.md`，黄金 fixture 在 `spec/fixtures/`，格式与 runner 的判定见 `spec/README.md`；Python 的薄 runner 是 `tests/test_fixtures.py`。

## 使用

作为 pre-commit 远端 hook (推荐)，`rev` 固定到一个 tag ([tag 列表](https://github.com/Luolc/lo-md-lint/tags))：

```yaml
repos:
  - repo: https://github.com/Luolc/lo-md-lint
    rev: <tag>
    hooks:
      - id: lo-md-lint
```

默认只检查、不修复；要自动修复就自己加 `args: ["--fix"]`。

不接 pre-commit、手动或在 CI 里一次性跑：

```sh
uvx --from git+https://github.com/Luolc/lo-md-lint@<tag> lo-md-lint --all
uvx --from git+https://github.com/Luolc/lo-md-lint@<tag> lo-md-lint <file>...
```

### 开关某条规则

启用集 = ((默认集 ∪ experimental 集) ∪ `enable`) − `disable`，experimental 集只在 `enable_experimental = true` 时并入；不写配置就是默认行为。配置模型的正本是 `spec/rules.md`「配置」，这里只举例。

`pyproject.toml` 里 (Python 项目)：

```toml
[tool.lo-md-lint]
disable = ["R3"]
enable = ["R9"]
```

或者仓库根放一个 `lo-md-lint.toml` (键结构相同，只是不带表头)：

```toml
disable = ["R3"]
enable = ["R9"]
```

`skip_zh_units` 列出的中文计量单位字，紧跟其前的那段数字不加空格 (`2011年5月15日` 保持原样)，默认 `""` 即不豁免：

```toml
skip_zh_units = "年月日天号时分秒"
```

`severity` 把单条规则降成 `warning`：照常进报告、`--fix` 照常修，只是不再让退出码变成非零 (R1–R11 默认 `error`，实验规则默认 `warning`)。

```toml
severity = { R8 = "warning" }
```

`enable_experimental = true` 一次纳入全部 experimental 规则 —— 误报率还没验够、默认不进启用集的那些；纳入之后可以用 `disable` 逐条关、用 `severity` 逐条覆盖。

```toml
enable_experimental = true
```

### 实验规则 (默认关)

当前的 experimental 规则是中文 AI 腔的 A1–A4、英文 tell 的 A5–A7 与术语选词 T1，全部默认严重度 `warning`：照常进报告，但**不让退出码变成非零**，所以打开它们不会把 CI 变红。要它们参与退出码就用 `severity` 逐条改成 `error`；要单独关掉某一条就写进 `disable`。

| id | 管什么 | 可修复 |
| --- | --- | --- |
| A1 | 套话：综上所述、值得注意的是、众所周知…… | 否 |
| A2 | 否定平行：「不是 … 而是 …」 | 否 |
| A3 | 互联网黑话：赋能、抓手、赛道…… | 否 |
| A4 | 聊天残留：希望这对你有帮助…… | 否 |
| A5 | 英文 AI 词汇：`tapestry`、`testament`、`pivotal`…… | 否 |
| A6 | 英文否定平行：`not just X, but Y`、`it's not X, it's Y` | 否 |
| A7 | Claudish 专用词：`load-bearing` | 否 |
| T1 | 术语选词：`token` 语境里的「代币」→「令牌」 | 是 |

判定用的词表在 `spec/wordlists/`，是规范的一部分：加一条词只改那里，不动任何实现。中文词表按字面子串匹配，英文词表 (A5 / A7) 按整词、大小写不敏感 —— 落在行内代码、链接与 URL 里的词照全局豁免不报 —— 英文词在标识符与路径里太常见。英文词表短得刻意：收词要过一道自检 —— 给候选词造一句无修辞意图的常规技术行文，造得出就不收，所以 `realm`、`robust`、`gated on` 这类有日常用法的词一个都不在表里。T1 只在同一行出现语境锚点 (`token` / `OAuth` / `cache` 这类) 时才报、才改 —— 讲钱的语境里的「代币」不动。

```toml
enable_experimental = true
disable = ["A3"]
severity = { T1 = "error" }
```

临时在命令行上开关，整体覆盖配置文件：

```sh
lo-md-lint --disable R3 <file>...
lo-md-lint --disable R1,R3 --all
lo-md-lint --enable R9 --all
```

### 单点豁免与整份跳过

单点误报不必动配置：行内指令是独占一行的 HTML 注释，就地关掉规则，语义正本是 `spec/rules.md`「行内指令」。

```markdown
<!-- lo-md-lint-disable-next-line R4 -->
サンプルIT推進部の例

<!-- lo-md-lint-disable R4 R5 -->
成段的原样文本，R4、R5 关到下面这行为止。
<!-- lo-md-lint-enable -->
```

整份文件不该被检查就放一个 `.lo-md-lint-ignore`，语法与 `.gitignore` 相同，向上查找的顺序与配置文件一样；对 `--all` 与命令行显式传入的文件都生效 (正本是 `spec/rules.md`「忽略文件」)。

```gitignore
vendor/
docs/generated/*.md
!docs/generated/index.md
```

## 本地开发

```sh
uv sync                          # 建 .venv、装 dev 依赖
uv run pre-commit install        # 装本地钩子 (只需一次)
uv run lo-md-lint --all          # 检查全部 tracked Markdown
uv run lo-md-lint --all --fix    # 自动修复大部分违规后复查
uv run lo-md-lint <file>...      # 检查指定文件
uv run pytest -q                 # 测试
```
