# limae

[![CI](https://github.com/Luolc/limae/actions/workflows/ci.yml/badge.svg)](https://github.com/Luolc/limae/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Luolc/limae)](https://github.com/Luolc/limae/releases)
[![License](https://img.shields.io/github/license/Luolc/limae)](LICENSE)

简体中文 | [English](README.en.md)

一个 Markdown linter，从中文技术写作的排版规则入手：逐条检查中英文之间的空格、半角标点、括号形态及其外侧空格，大部分能直接修好。名字取自贺拉斯 (Horace) 的 *limae labor* —— 「锉刀的功夫」，把写完的稿子一遍遍磨干净。

## 它改什么

原稿：

```markdown
这个CLI工具支持3种模式,提供2GB缓存(默认关闭)。
```

`limae draft.md` 报出：

```text
draft.md:1: error: [zh-typography-1 halfwidth punct next to CJK] …这个CLI工具支持3种模式,提供2GB缓存(默认关闭…
draft.md:1: error: [zh-typography-3 no space before (] …持3种模式,提供2GB缓存(默认关闭)。…
draft.md:1: error: [zh-typography-4 no space between CJK and Latin] …这个CLI工具支持3种模式,…
draft.md:1: error: [zh-typography-4 no space between CJK and Latin] …这个CLI工具支持3种模式,提供2…
draft.md:1: error: [zh-typography-4 no space between CJK and Latin] …支持3种模式,提供2GB缓存(默认关闭)。…
draft.md:1: error: [zh-typography-5 no space between CJK and digit] …这个CLI工具支持3种模式,提供2GB缓存…
draft.md:1: error: [zh-typography-5 no space between CJK and digit] …这个CLI工具支持3种模式,提供2GB缓存(…
draft.md:1: error: [zh-typography-5 no space between CJK and digit] …I工具支持3种模式,提供2GB缓存(默认关闭)。…
draft.md:1: error: [zh-typography-6 no space between number and unit] …I工具支持3种模式,提供2GB缓存(默认关闭)。…

9 error(s), 0 warning(s). --fix auto-fixes most.
```

`limae --fix draft.md` 之后：

```markdown
这个 CLI 工具支持 3 种模式，提供 2 GB 缓存 (默认关闭)。
```

## 安装

按包管理器选一条，装到的都是同一批预构建二进制 —— 与 [GitHub Release](https://github.com/Luolc/limae/releases) 上的是同一份字节，发布时逐个比对过 sha256 (机制见 [发布手册](docs/knowledge/release.md))。

Homebrew (macOS 与 Linuxbrew)：

```sh
brew install luolc/tap/limae
```

不经包管理器可用安装脚本。它按 `uname` 选择本机的 tarball，用 Release 旁边的 `.sha256` 边车校验，装进 `~/.local/bin`：

```sh
curl -fsSL https://limae.luolc.com/install.sh | sh
```

装到别处、钉一个版本，或者一个文件都别写，三个环境变量各管一件事，说明见 [Rust 分发预演](docs/knowledge/rust-binary-distribution.md)「一键安装的旋钮与 PATH 行为」。

Python 或 Node 项目从各自的包管理器安装同一个二进制：

```sh
uv add --dev limae      # 或 pip install limae
npm i -D @limae/cli     # 之后 npx limae
```

预构建二进制只有四个平台：Linux x86_64 / aarch64 (静态 musl，glibc 系统照跑)、macOS x86_64 / arm64。**Windows 没有预构建二进制**，请从源码安装：

```sh
cargo install --locked --git https://github.com/Luolc/limae --tag <tag> limae
```

## 用法

三条命令：

```sh
limae <file>...   # 检查指定文件
limae --all       # 检查当前目录下全部 Markdown
limae --fix --all # 先修再查
```

退出码：干净时为 0，有 error 级发现时为 1，配置错误另有一档 (以 [`spec/rules.md`](spec/rules.md)「退出码」为准)。

`--all` 查的是**文件系统**，不是 git 索引 (index)。刚写出来、还没 `git add` 的文件也会检查；点开头的目录 (`.github/`、`.agents/`) 也在内，唯一按名字排除的是 `.git`，被 `.gitignore` / `.ignore` / `.limae-ignore` 忽略的文件不查。

单点误报可就地关闭，使用独占一行的 HTML 注释：

```markdown
<!-- limae-disable-next-line zh-typography-4 -->
サンプルIT推進部の例
```

整份文件不该检查时，放一个 `.limae-ignore`，语法与 `.gitignore` 相同。两者的语义都在 [`spec/rules.md`](spec/rules.md)「行内指令」「忽略文件」。

## 接 pre-commit

默认使用镜像仓 [`Luolc/limae-pre-commit`](https://github.com/Luolc/limae-pre-commit)，`rev` 固定到它的一个 tag ([tag 列表](https://github.com/Luolc/limae-pre-commit/tags))。消费方不用额外安装任何东西，pre-commit 会在自己的缓存里安装 PyPI 上那个 wheel 中的预构建二进制，不编译：

```yaml
repos:
  - repo: https://github.com/Luolc/limae-pre-commit
    rev: <tag>     # 一个 tag 对应一个 limae 版本：v0.13.2 即 limae 0.13.2
    hooks:
      - id: limae
```

**Windows 上改用本仓这条**：Release 没有 Windows 产物，镜像仓那条在 Windows 上装不上；本仓这条用 Cargo 现编，任何平台都能用，代价是机器上要有 Rust 工具链，第一次还要等一次编译。

```yaml
repos:
  - repo: https://github.com/Luolc/limae
    rev: <tag>
    hooks:
      - id: limae
```

两条 hook 的 id 都是 `limae`，行为相同：默认只检查、不修复，要自动修复就加 `args: ["--fix"]`。为什么有两个仓库，以及 Python 与 Rust 两条 language 合同各自要求什么，见 [Rust 分发预演](docs/knowledge/rust-binary-distribution.md)。

**规则开关一律写进配置文件，不要放进 `args:`** —— `--disable` / `--enable` 会**替换**配置文件，而不是叠加；命令行上一旦出现其中之一，`limae.toml` 与 `pyproject.toml` 的 `[tool.limae]` 根本不会被读取，而且不报错、不警告 (以 [`spec/rules.md`](spec/rules.md)「来源与优先级」为准)。

## 配置

在仓库根放一个 `limae.toml`，或者写进 `pyproject.toml` 的 `[tool.limae]` 表，两者键结构相同：

```toml
disable = ["zh-typography-3"]        # 关掉某条规则
enable = ["zh-typography-9"]         # 打开某条默认关闭的规则
enable_experimental = true           # 一次纳入全部实验规则
severity = { zh-typography-8 = "warning" }   # 覆盖单条规则的严重度
```

启用集 = ((默认集 ∪ experimental 集) ∪ `enable`) − `disable`，experimental 集只在 `enable_experimental = true` 时并入。全部键、发现顺序和配置错误的判定，见 [`spec/rules.md`](spec/rules.md)「配置」。

## 规则

规则 id 按家族分前缀：`zh-typography` 中文排版、`zh-tell` / `en-tell` AI 腔、`zh-word` 术语选词。中文排版一族默认启用，全部可修复，严重度为 `error`；AI 腔与术语选词为 experimental，默认关闭、默认 `warning` —— 打开它们不会让 CI 变红。规则不分文档语言，英文 tell 出现在中文文档里同样会报。

每条规则的判定、例句与三轴属性 (可修复性 / 严重度 / 成熟度) 都在 [`spec/rules.md`](spec/rules.md)；判定使用的词表在 [`spec/wordlists/`](spec/wordlists/)，加一条词只改那里，不动任何实现。AI 腔那一族的取词标准与词条释义另有一部[机器文言大词典](https://limae.luolc.com/)。

规则先于实现：每条规则都是一段语言无关的规范，任何实现都针对 [`spec/fixtures/`](spec/fixtures/) 的同一套黄金 fixture (golden fixtures) 运行 (ADR-[0001](docs/adr/0001-standalone-repo-spec-first-shared-fixtures.md) / [0006](docs/adr/0006-rule-grading-fixability-severity-experimental.md) / [0007](docs/adr/0007-ai-tells-rule-family-and-wordlists.md))。

## LLM 语义润色 (`limae polish`)

`limae polish -` 从 stdin 读入一段文本，交给本机已登录的 CLI (Claude Code / Codex / Grok，也可以自己配一条命令) 改写，再把结果写到 stdout：

```sh
limae polish - < draft.md > polished.md
```

它与 `check` / `--fix` 是互不触发的两段：排版由规则确定性地修，语义由模型改写，`polish` 永远不进 CI 的 required check。引擎怎么选、凭证怎么处理、实际送出去哪些字节，见 [ADR-0008](docs/adr/0008-limae-polish-cli.md) 与 [引擎行为实测](docs/research/polish-engine-cli-behavior.md)。

### 文件模式：`limae polish --share-repo-with-engine <files>` / `--all`

原地改写仓库中的文件，并让引擎能读取仓库里的其他文件作参考 (术语、名字、交叉引用)：

```sh
limae polish --share-repo-with-engine docs/guide.md README.md
limae polish --share-repo-with-engine --all
```

**每次都要带这个 flag。先读完它授予的权限再使用。** 没有它而给了文件或 `--all`，limae 会在任何引擎启动之前退出并解释；配置文件与环境变量都不能替你带上它。

- **这个模式会让本机的 coding agent 读到仓库里的内容，并把读到的内容发给该引擎的服务方。** 引擎不在你的仓库中启动，而是在一份一次性的导出视图中启动：仓库 tracked 文件 (`git ls-files`) 的拷贝，**没有 `.git`、没有被忽略与未跟踪的文件、没有 `.claude` / `.codex` / `.grok` 目录与 `.mcp.json`** —— 仓库自带的 hook、setting 与 MCP server 因此不会被加载，这是结构上的排除，不依赖各家的 flag。三家的只读工具集与关闭项目配置的 flag 也已加上，作为纵深。
- **视图挡住的是执行，不是指令。** 仓库里的 `AGENTS.md` / `CLAUDE.md` / `GROK.md` 是 tracked 文件，仍在视图中；它们不会因为在场而运行什么，但自动加载关不关闭取决于上面那层纵深 flag (随各家版本变化)，而且引擎随时能用工具读到，读了会不会照做由模型当场决定。这是有意的：一个仓库的 `AGENTS.md` 往往写着自己的行文约定。
- **排除不等于隔离。** 引擎带着文件工具，仍可读取按绝对路径交给它的、或它猜到的视图外文件 (实测 grok 读到了)；它按 `HOME` 自动加载的自家配置与登录态照旧。不要在含机密的仓库里使用它：被 `.gitignore` 忽略的文件不在视图中，但已经提交进仓库的机密在。
- **limae 自己的写回只写你点名 (或 `--all` 选出) 的文件**；引擎只回传正文。写回前，limae 核对目标仍是运行前那一版；否则拒绝写入，保留你的版本 (`not written`，退出码 1)。视图另有一道绊线：引擎如果在视图中写了东西，说明只读设置没有生效，这一轮不写，整个运行停止。**这是事后检测加冲突拒绝，不是写入隔离**：引擎进程本身，以及你在家目录里配置的东西 (hook、MCP server；实测 `~/.claude/settings.json` 的 hook 在文件模式下照跑) 仍可能在视图外写盘，绊线只检查视图。这句保证的主语是仓库，不是这台机器。
- **这个 flag 挡不住它被写进 Makefile、justfile、`.pre-commit-config.yaml` 的 `args` 或 CI step** —— 这些文件都在仓库里，之后的调用者只会看到 `make polish`。这是我们知情选择的代价 (为什么不做信任记录，见 [ADR-0017](docs/adr/0017-polish-file-mode.md))。

文件模式只运行三家预设 (它们带只读工具集)；`custom` 引擎在这里会被拒绝，因为它的命令来自仓库自己的配置，limae 约束不了它 —— 自建引擎请走 `limae polish -`，那里 `custom` 仍是你自己的命令，边界由你自己划。上面几条承诺只针对文件模式，不针对 `limae polish -`。`--all` 选的是当前目录下每一个 Markdown 文件，与 `limae --all` 完全相同，所以 `.gitignore` 与 `.limae-ignore` 都会被尊重；但这两份 ignore 只决定**改写哪些文件**，不决定**引擎读到哪些**：视图是 `git ls-files`，被 `.limae-ignore` 排除的 tracked 文件仍在视图中，被 `.gitignore` 排除的不在。符号链接不会改写：命令行点名的会被拒绝 (请点名它的目标)，`--all` 选出的会跳过并说明。每个目标输出一行 (`polished:` / `unchanged:`)，诊断写进 stderr。`limae polish -` 一行不变，也永远不需要这个 flag。

## 给 agent 的 skill (`skills/write-naturally/`)

`polish` 用于写完之后改；skill 用于让模型**写的时候**就遵守规则。把 `skills/write-naturally/` 整个目录复制或软链进你的 agent 的 skill 目录 (格式按 [agentskills.io](https://agentskills.io/specification)，目录名就是 skill 名 `write-naturally`)，模型写中文前会先读 `references/zh/guide.md` 与 `references/zh/lexicon.md`。词典与 `polish` 使用同一份 `spec/lexicon/zh.toml`，`skills/write-naturally/` 由 `spec/skill/` 与它生成，改源文件后运行 `cargo run --example render-skill` 重新生成。

## 参与开发

开发者请读 [`AGENTS.md`](AGENTS.md)：目录约定、质量门与其顺序都在那里。决策记录在 [`docs/adr/`](docs/adr/)，待办在 [`docs/tracker.md`](docs/tracker.md)。

## 许可证

Apache License, Version 2.0，见 [`LICENSE`](LICENSE)。
