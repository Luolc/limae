# limae

Markdown linter，从中文技术写作的排版规则起步。名字取自贺拉斯 (Horace) 的 *limae labor* —— 「锉刀的功夫」，把写完的稿子一遍遍磨到干净。

## 定位与愿景

- **规则先于实现**：每条规则是一段语言无关的规范 (specification)，写在 `spec/rules.md`，有一个稳定的 id、可以逐条关掉；规则 id 按家族分前缀 —— `R` 中文排版、`A` AI 腔 (中英文同一家族)、`T` 术语选词 (ADR-0007)。中文排版规则 (中英文之间空格、数字与中文之间空格、半角括号外空格、全角标点……) 是默认规则集，AI 腔与术语选词是默认关闭的实验规则。
- **多实现、一套黄金 fixture (golden fixtures)**：黄金集在 `spec/fixtures/`，Python 版是参考实现 (reference implementation)，任何后续实现都对着同一套「输入 / 期望输出」跑，通过即合规。
- **对标 ruff 之于 Python**：长期大概率以 Rust 为主实现 —— 一个 Rust 写的 Markdown lint，可被 Python / Node 生态经 pre-commit、包管理器等集成，也能直接当命令行工具用。
- **配置走 toml**：独立配置文件 `limae.toml` 或 `pyproject.toml` 的 `[tool.limae]` 表，两者同构，用 `disable` / `enable` 两个键逐条开关规则；绝大多数规则默认启用，个别默认关闭的规则在规范条目里标明。每条规则另有可修复性 / 严重度 / 成熟度三个正交属性 (ADR-0006)，`severity` 覆盖单条规则的严重度、`enable_experimental` 一次纳入全部 experimental 规则。开关之外还有调整单条规则判定的键，当前是 zh-typography-5 的 `skip_zh_units` (中文计量单位豁免，默认不豁免)。

决策记录在 `docs/adr/`；agent 守则在 `AGENTS.md`。

## 现状

Python 参考实现已就位。默认规则集是中文排版一套：宽度转换 (zh-typography-1 CJK 旁的半角标点含句号、zh-typography-2 全角括号、zh-typography-10 全角数字)、空格 (zh-typography-3 半角括号外侧、zh-typography-4 CJK–拉丁字母、zh-typography-5 CJK–数字、zh-typography-6 数字–单位、zh-typography-7 行内代码定界符、zh-typography-8 破折号两侧、zh-typography-11 全角标点旁去空格、zh-typography-9 链接前，默认关)，全是 fixable · error · stable。另有一批默认关闭的实验规则：中文 AI 腔 zh-tell-1 套话、zh-tell-2 否定平行、zh-tell-3 互联网黑话、zh-tell-4 聊天残留，英文 tell en-tell-1 AI 词汇、en-tell-2 否定平行、en-tell-3 Claudish 专用词、中文造词 zh-tell-5「零 + 名词」，与术语选词 zh-word-1、zh-word-2「秘密」误用 (ADR-0007)，全是 warning · experimental，判定用的词表在 `spec/wordlists/`。规则不分语言 (ADR-0006)：英文 tell 出现在中文文档里同样报。每条规则都可单独开关 (ADR-0003 / ADR-0004)，并按可修复性 / 严重度 / 成熟度三轴标注 (ADR-0006)。逃生口两个：行内指令按行 × 规则就地关掉，`.limae-ignore` 把整份文件排除在输入之外。`spec/` 已建起来：规则规范在 `spec/rules.md`，黄金 fixture 在 `spec/fixtures/`，格式与 runner 的判定见 `spec/README.md`；Python 的薄 runner 是 `tests/test_fixtures.py`。

## 使用

作为 pre-commit 远端 hook (推荐)，`rev` 固定到一个 tag ([tag 列表](https://github.com/Luolc/limae/tags))：

```yaml
repos:
  - repo: https://github.com/Luolc/limae
    rev: <tag>
    hooks:
      - id: limae
```

默认只检查、不修复；要自动修复就自己加 `args: ["--fix"]`。

不接 pre-commit、手动或在 CI 里一次性跑：

```sh
uvx --from git+https://github.com/Luolc/limae@<tag> limae --all
uvx --from git+https://github.com/Luolc/limae@<tag> limae <file>...
```

### 开关某条规则

启用集 = ((默认集 ∪ experimental 集) ∪ `enable`) − `disable`，experimental 集只在 `enable_experimental = true` 时并入；不写配置就是默认行为。配置模型的正本是 `spec/rules.md`「配置」，这里只举例。

`pyproject.toml` 里 (Python 项目)：

```toml
[tool.limae]
disable = ["zh-typography-3"]
enable = ["zh-typography-9"]
```

或者仓库根放一个 `limae.toml` (键结构相同，只是不带表头)：

```toml
disable = ["zh-typography-3"]
enable = ["zh-typography-9"]
```

`skip_zh_units` 列出的中文计量单位字，紧跟其前的那段数字不加空格 (`2011年5月15日` 保持原样)，默认 `""` 即不豁免：

```toml
skip_zh_units = "年月日天号时分秒"
```

`severity` 把单条规则降成 `warning`：照常进报告、`--fix` 照常修，只是不再让退出码变成非零 (`zh-typography` 一族默认 `error`，实验规则默认 `warning`)。

```toml
severity = { zh-typography-8 = "warning" }
```

`enable_experimental = true` 一次纳入全部 experimental 规则 —— 误报率还没验够、默认不进启用集的那些；纳入之后可以用 `disable` 逐条关、用 `severity` 逐条覆盖。

```toml
enable_experimental = true
```

### 实验规则 (默认关)

当前的 experimental 规则是中文 AI 腔的 `zh-tell-1` 到 `zh-tell-5`、英文 AI 腔的 `en-tell-1` 到 `en-tell-3`、中文用词的 `zh-word-1` 与 `zh-word-2`，全部默认严重度 `warning`：照常进报告，但**不让退出码变成非零**，所以打开它们不会把 CI 变红。要它们参与退出码就用 `severity` 逐条改成 `error`；要单独关掉某一条就写进 `disable`。

| id | 管什么 | 可修复 |
| --- | --- | --- |
| zh-tell-1 | 套话：综上所述、值得注意的是、众所周知…… | 否 |
| zh-tell-2 | 否定平行：「不是 … 而是 …」 | 否 |
| zh-tell-3 | 互联网黑话：赋能、抓手、赛道…… | 否 |
| zh-tell-4 | 聊天残留：希望这对你有帮助…… | 否 |
| en-tell-1 | 英文 AI 词汇：`tapestry`、`testament`、`pivotal`…… | 否 |
| en-tell-2 | 英文否定平行：`not just X, but Y`、`it's not X, it's Y` | 否 |
| en-tell-3 | Claudish 专用词：`load-bearing` | 否 |
| zh-tell-5 | 「零 + 名词」造词：零秘密、零额外请求、零重复…… | 否 |
| zh-word-1 | 术语选词：`token` 语境里的「代币」→「令牌」 | 是 |
| zh-word-2 | 「秘密」误用：按语境是密钥 / 凭证 / 敏感信息 | 否 |

判定用的词表在 `spec/wordlists/`，是规范的一部分：加一条词只改那里，不动任何实现。中文词表按字面子串匹配，英文词表 (en-tell-1 / en-tell-3) 按整词、大小写不敏感 —— 落在行内代码、链接与 URL 里的词照全局豁免不报 —— 英文词在标识符与路径里太常见。英文词表短得刻意：收词要过一道自检 —— 给候选词造一句无修辞意图的常规技术行文，造得出就不收，所以 `realm`、`robust`、`gated on` 这类有日常用法的词一个都不在表里。zh-word-1 只在同一行出现语境锚点 (`token` / `OAuth` / `cache` 这类) 时才报、才改 —— 讲钱的语境里的「代币」不动。zh-tell-5 与 zh-word-2 的词表方向相反，是**豁免表**：`zh-tell-5-allow.txt` 收「零售」「从零」「零信任」这类汉语里本来就有的词与固定搭配，`zh-word-2-allow.txt` 收「保守秘密」这类用本义的搭配，盖住命中处就不报。

```toml
enable_experimental = true
disable = ["zh-tell-3"]
severity = { zh-word-1 = "error" }
```

临时在命令行上开关，整体覆盖配置文件：

```sh
limae --disable zh-typography-3 <file>...
limae --disable zh-typography-1,zh-typography-3 --all
limae --enable zh-typography-9 --all
```

### 单点豁免与整份跳过

单点误报不必动配置：行内指令是独占一行的 HTML 注释，就地关掉规则，语义正本是 `spec/rules.md`「行内指令」。

```markdown
<!-- limae-disable-next-line zh-typography-4 -->
サンプルIT推進部の例

<!-- limae-disable zh-typography-4 zh-typography-5 -->
成段的原样文本，zh-typography-4、zh-typography-5 关到下面这行为止。
<!-- limae-enable -->
```

整份文件不该被检查就放一个 `.limae-ignore`，语法与 `.gitignore` 相同，向上查找的顺序与配置文件一样；对 `--all` 与命令行显式传入的文件都生效 (正本是 `spec/rules.md`「忽略文件」)。

```gitignore
vendor/
docs/generated/*.md
!docs/generated/index.md
```

### LLM 语义润色 (`limae polish`)

`limae polish -` 从 stdin 读一段文本，交给一个**本机已登录的 CLI** 改写，把结果写 stdout。它与 `check` / `--fix` 是两段不同的东西 (ADR-0005 §四、ADR-0008 §二)：排版由规则确定性地修，语义由模型改写，两段互不触发；`polish` 永远不进 CI 的 required check。

```sh
uv run limae polish - < draft.md > polished.md
cat draft.md | uv run limae polish - --engine codex
```

润色 prompt 是两层 (ADR-0008 §九)：通用层 `spec/polish/general.md` (英文) 加每种语言一份用该语言写的层 (中文是 `spec/polish/zh.md`)，输入里有中文就自动叠上中文那层。两份都在 `spec/` 下，跟规则规范一样可以整体替换。词表不进 prompt (ADR-0007)。

引擎在 `[polish]` 表里配，键三个：

```toml
[polish]
engine = "auto"    # auto | claude | codex | grok | custom
model = ""         # 留空 = 该预设自带的档
command = []       # engine = "custom" 时的完整命令
```

- **`claude` / `codex` / `grok`** 各是一份内置的命令模板：本工具不自建 HTTP 请求、不解析任何一家的凭证，只调你已经登录的那个 CLI。
- **预设引擎在一次性临时目录里跑，环境变量按白名单给**：引擎是外部服务，`polish` 交出去的只有你喂进 stdin 的那段文本，周围仓库的内容不在授权范围内 (实测：在仓库里启动的引擎会去读那个仓库，连未提交的改动一起)。所以工作目录是临时目录，环境只放行 `PATH` / `HOME` / `TMPDIR` / locale / `TZ` 与该家自己的凭证与 base URL 变量，`PWD` 指向临时目录，`GIT_*` 与宿主会话变量一概不给。细节与实测见 `docs/research/polish-engine-cli-behavior.md`。`custom` 保留调用方的目录与环境 —— 那是你自己的命令，边界由你自己划；预设的白名单挡住了你的 provider 真正需要的变量时，也走 `custom`。
- **`custom`** 是逃生口：自建服务、公司网关、自写脚本都用一条完整命令接进来，`{spec_file}` 会被换成拼好的 prompt 文件路径、`{text}` 换成正文；命令里没有 `{text}` 时正文走 stdin。
- **`auto`** (默认) 按 ADR-0008 §三 的六步找引擎：`LIMAE_ENGINE` 指定的直接用 → 正跑在谁的会话里就先探谁 → 二进制不存在直接出局 (唯一的硬否定) → 凭证线索只用来排序 (只判存在性，不读值) → 依次探活、第一个答上来的用 (缓存的是每个引擎各自的探活结果，判定顺序每次都重走，所以换一个会话选中的引擎会跟着变；成功记 1 小时、失败只记 5 分钟 —— 陈旧的失败会让刚登录完的你等一小时，代价比陈旧的成功大得多，而「未安装」重算是零成本、根本不缓存) → 全失败就逐引擎报诊断 (未安装 / 凭证失效 / 未发现凭证 / 网络不可达)，每种给下一步；结论若来自缓存，会写明是几分钟前探到的，并给出立刻重试的办法 (`--engine`，或删掉诊断里写出路径的那个缓存文件)。

`--engine` 与 `--model` 覆盖配置，`--engine` 也覆盖 `LIMAE_ENGINE`。失败一律非零退出并说清哪一步失败 (ADR-0008 §六)；诊断只说状态，不回显引擎的任何输出。

**还没做的**：hook 形态与 A/B 采集、文件参数与 `--diff` / `--check`、改写前后的结构不变量核对 (ADR-0008 §七、§八、§十)。

## 本地开发

```sh
uv sync                          # 建 .venv、装 dev 依赖
uv run pre-commit install        # 装本地钩子 (只需一次)
uv run limae --all               # 检查全部 tracked Markdown
uv run limae --all --fix         # 自动修复大部分违规后复查
uv run limae <file>...           # 检查指定文件
uv run limae polish - < draft.md # 用 LLM 润色一段文本 (stdin 进、stdout 出)
uv run pytest -q                 # 测试
```
