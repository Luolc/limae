# limae

Markdown linter，从中文技术写作的排版规则起步。名字取自贺拉斯 (Horace) 的 *limae labor* —— 「锉刀的功夫」，把写完的稿子一遍遍磨到干净。

## 定位与愿景

- **规则先于实现**：每条规则是一段语言无关的规范 (specification)，写在 `spec/rules.md`，有一个稳定的 id、可以逐条关掉；规则 id 按家族分前缀 —— `R` 中文排版、`A` AI 腔 (中英文同一家族)、`T` 术语选词 (ADR-0007)。中文排版规则 (中英文之间空格、数字与中文之间空格、半角括号外空格、全角标点……) 是默认规则集，AI 腔与术语选词是默认关闭的实验规则。
- **多实现、一套黄金 fixture (golden fixtures)**：黄金集在 `spec/fixtures/`，任何实现都对着同一套「输入 / 期望输出」跑，通过即合规。
- **对标 ruff 之于 Python**：主实现是 Rust —— 一个 Rust 写的 Markdown lint，可被 Python / Node 生态经 pre-commit、包管理器等集成，也能直接当命令行工具用 (ADR-0015)。
- **配置走 toml**：独立配置文件 `limae.toml` 或 `pyproject.toml` 的 `[tool.limae]` 表，两者同构，用 `disable` / `enable` 两个键逐条开关规则；绝大多数规则默认启用，个别默认关闭的规则在规范条目里标明。每条规则另有可修复性 / 严重度 / 成熟度三个正交属性 (ADR-0006)，`severity` 覆盖单条规则的严重度、`enable_experimental` 一次纳入全部 experimental 规则。开关之外还有调整单条规则判定的键，当前是 zh-typography-5 的 `skip_zh_units` (中文计量单位豁免，默认不豁免)。

决策记录在 `docs/adr/`；agent 守则在 `AGENTS.md`。

## 现状

Rust 实现是唯一的实现、正式的 `limae` 命令。默认规则集是中文排版一套：宽度转换 (zh-typography-1 CJK 旁的半角标点含句号、zh-typography-2 全角括号、zh-typography-10 全角数字)、空格 (zh-typography-3 半角括号外侧、zh-typography-4 CJK–拉丁字母、zh-typography-5 CJK–数字、zh-typography-6 数字–单位、zh-typography-7 行内代码定界符、zh-typography-8 破折号两侧、zh-typography-11 全角标点旁去空格、zh-typography-9 链接前，默认关)，全是 fixable · error · stable。另有一批默认关闭的实验规则：中文 AI 腔 zh-tell-1 套话、zh-tell-2 否定平行、zh-tell-3 互联网黑话、zh-tell-4 聊天残留，英文 tell en-tell-1 AI 词汇、en-tell-2 否定平行、en-tell-3 Claudish 专用词、中文造词 zh-tell-5「零 + 名词」，与术语选词 zh-word-1、zh-word-2「秘密」误用 (ADR-0007)，全是 warning · experimental，判定用的词表在 `spec/wordlists/`。规则不分语言 (ADR-0006)：英文 tell 出现在中文文档里同样报。每条规则都可单独开关 (ADR-0003 / ADR-0004)，并按可修复性 / 严重度 / 成熟度三轴标注 (ADR-0006)。逃生口两个：行内指令按行 × 规则就地关掉，`.limae-ignore` 把整份文件排除在输入之外。`spec/` 已建起来：规则规范在 `spec/rules.md`，黄金 fixture 在 `spec/fixtures/`，格式与 runner 的判定见 `spec/README.md`。

## 使用

### 一键安装 (macOS / Linux)

macOS 与 Linuxbrew 走 Homebrew：

```sh
brew install luolc/tap/limae
```

不经包管理器就用安装脚本，它取的是同一批 [GitHub Release](https://github.com/Luolc/limae/releases) 预构建二进制：

```sh
curl -fsSL https://limae.luolc.com/install.sh | sh
```

脚本按 `uname` 选本机对应的 tarball、用 Release 旁边的 `.sha256` 边车校验、装进 `~/.local/bin`；那个目录还不在 `PATH` 上时，再往当前 shell 的 rc 文件里写一段带成对标记的块，重复执行不会写第二遍。装完通常**开一个新终端就行**，想在当前这个终端里立刻用，脚本会把那一行 `export` 打出来。认不出的 OS / CPU、对不上的校验和、机器上一个 sha256 工具都没有，三种情况都是**报错退出、什么都不装** —— 不猜平台，也不在校验不了的时候跳过校验。

三个环境变量是留给要自己安排的人的口子：

| 变量 | 作用 | 默认 |
| --- | --- | --- |
| `LIMAE_VERSION` | 装指定的 Release tag，如 `v0.13.2` | 最新的那个 |
| `LIMAE_INSTALL_DIR` | 装到哪个目录 | `~/.local/bin` |
| `LIMAE_NO_MODIFY_PATH` | 非空则一个文件都不写，只把要加的那行打出来 | 未设 |

```sh
curl -fsSL https://limae.luolc.com/install.sh | LIMAE_VERSION=v0.13.2 LIMAE_NO_MODIFY_PATH=1 sh
```

rc 文件由 chezmoi 管理时 (脚本拿 `chezmoi source-path` 判定)，它**不写**那个文件，改为把该加的行打印出来：写进去也会在下次 `chezmoi apply` 时消失，而那在用户眼里就是「装过了又没了」。fish 以及其它脚本拿不准 rc 语法的 shell 同样只打印，给的是 `fish_add_path` 那一行。

Windows 没有预构建二进制，用下面的 `cargo install`。

### 接 pre-commit

两条远端 hook 并存，**不是替换关系** —— 差别在消费方要有什么、以及支不支持 Windows：

| hook | 消费方要有什么 | 首次耗时 | 覆盖的平台 |
| --- | --- | --- | --- |
| 镜像仓 `language: python` | 什么都不用额外装 | 几秒，下一个 wheel | Linux x86_64 / aarch64、macOS x86_64 / arm64，**没有 Windows** |
| 本仓 `language: rust` | Rust 工具链 | 一次完整编译 | 任何平台，含 Windows |

**默认走镜像仓** [`Luolc/limae-pre-commit`](https://github.com/Luolc/limae-pre-commit)，`rev` 固定到它的一个 tag ([tag 列表](https://github.com/Luolc/limae-pre-commit/tags))：

```yaml
repos:
  - repo: https://github.com/Luolc/limae-pre-commit
    rev: <tag>     # 一个 tag 对应一个 limae 版本：v0.13.2 即 limae 0.13.2
    hooks:
      - id: limae
```

镜像仓的 `pyproject.toml` 只做一件事：把 limae 钉到一个版本。pre-commit 按 [Python language 合同](https://pre-commit.com/#python) 在自己的缓存里 `pip install` 它，装下来的是 PyPI 上那个 wheel 里的**预构建二进制** —— 不编译、不需要 Rust 工具链。pre-commit 的 21 种 language 里没有「下载预编译 binary」这一种，`language: python` 之所以够用，是因为 pre-commit 自己就是 Python 写的：能跑 pre-commit 就一定有 Python (ruff 走的也是这条路)。**为什么是独立的一个仓**：这条合同要求在 hook 仓根上 `pip install .` 装得起来，也就是仓根要有 `pyproject.toml`，而本仓已经整体删掉了 Python (ADR-0015 补记)。镜像仓的 tag 与 pin 由本仓 release workflow 在 wheel 发上 PyPI 之后写进去，不是手改的。

**Windows 上用本仓这条。** Release 只有四个 target、没有 Windows 产物，所以 Windows 上 `pip install limae` 找不到 wheel，镜像仓那条会**装不上**；本仓的 [Rust language 合同](https://pre-commit.com/#rust) 用 Cargo 把 binary 编进 pre-commit 自己的缓存，因此在任何平台上都能用，代价是机器上要有 Rust 工具链、第一次安装要联网并等一次编译 (之后走缓存)。

```yaml
repos:
  - repo: https://github.com/Luolc/limae
    rev: <tag>
    hooks:
      - id: limae
```

两条 hook 的 id 都是 `limae`，跑起来行为相同：默认只检查、不修复；要自动修复就自己加 `args: ["--fix"]`。

#### 规则一律进配置文件，`args:` 里不放规则开关

`--disable` / `--enable` 是**替换**配置文件，不是叠加：命令行上一旦出现其中任何一个，`limae.toml` 与 `pyproject.toml` 的 `[tool.limae]` **根本不会被读** (`rust/config.rs` 的 `resolve`)，而且**不报错、不警告**。所以下面这样写，等于把仓库自己的整份 limae 配置静默关掉：

```yaml
# 错的：仓库根上的 limae.toml 会被整个忽略，且没有任何提示
hooks:
  - id: limae
    args: [--disable, zh-typography-3]
```

规则选择写进配置文件，仓库自己跑的 limae 与这条 hook 读到的才是同一份。配置文件的发现顺序 (`rust/config.rs` 的 `find_config`)：从被检查文件所在目录往上走，**碰到含 `.git` 的那一层就停**；每一层先看 `limae.toml`，再看 `pyproject.toml` 的 `[tool.limae]` 表，取先命中的那一个。两者键结构相同，只差 `pyproject.toml` 多一层表头 —— Node 项目因此只能用 `limae.toml`，`package.json` 不是配置来源。

不接 pre-commit、手动或在 CI 里一次性跑：

```sh
cargo install --locked --git https://github.com/Luolc/limae --tag <tag> limae
limae --all
limae <file>...
```

Python 或 Node 项目也可以从各自的包管理器装同一个二进制 (与 [GitHub Release](https://github.com/Luolc/limae/releases) 上的是同一份字节，发布时逐个比对过 sha256，机制见 [发布手册](docs/knowledge/release.md))：

```sh
uv add --dev limae      # 或 pip install limae
npm i -D @limae/cli     # 之后 npx limae
```

预构建二进制只有四个平台：Linux x86_64 / aarch64 (静态 musl，glibc 系统照跑)、macOS x86_64 / arm64。没有 Windows；Windows 上用上面的 `cargo install`。

`--all` 走的是**文件系统**而不是 git 索引 (index)：从当前目录往下递归收所有 `*.md`，刚写出来、还没 `git add` 的文件照查 —— 这是抄 [ruff](https://github.com/astral-sh/ruff) 的做法。**「所有」就是所有**：点开头的文件、点开头的目录 (`.github/`、`.agents/` 这类真放着文档的地方) 都在内，指向 Markdown 文件的符号链接 (symlink) 顺着链接查，指向目录的符号链接不进去 (所以不会绕圈)，**解析不了的链接照样选中、在读的时候报错** (与在命令行上直接点它的名字结果相同，不会静默消失)；唯一按名字排除的是 `.git`。被 `.gitignore` (在 git 仓里才生效)、`.git/info/exclude`、全局 gitignore、`.ignore` 或 `.limae-ignore` 忽略的不查。不在 git 仓里也能用。

没有内置的默认排除表 (ruff 有一张写死的目录名单)：`node_modules`、`target`、`.venv` 这类靠 `.gitignore` 排除，真实项目基本都写了。已知边界是**既不在 git 仓、又没有任何 ignore 文件**的目录 —— 这种目录里它会走进构建产物与虚拟环境。

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

临时在命令行上开关，**整体覆盖**配置文件 —— 带上其中任何一个，配置文件就根本不会被读，也不会有任何提示 (所以不要把它们写进 pre-commit 的 `args:`，见上)：

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
limae polish - < draft.md > polished.md
cat draft.md | limae polish - --engine codex
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
uvx pre-commit@4.2.0 install     # 装本地钩子 (只需一次；pre-commit 本身由 uvx 现取)
cargo build                       # 建出 target/debug/limae
target/debug/limae --all          # 检查当前目录下全部 Markdown
target/debug/limae --all --fix    # 自动修复大部分违规后复查
target/debug/limae <file>...      # 检查指定文件
target/debug/limae polish - < draft.md  # 用 LLM 润色 (stdin 进、stdout 出)
cargo test                        # 测试
```

全部质量门与它们的顺序见 [`AGENTS.md`](AGENTS.md)。

## 许可证

Apache License, Version 2.0，见 [`LICENSE`](LICENSE)。
