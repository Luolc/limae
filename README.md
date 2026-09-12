# limae

[![CI](https://github.com/Luolc/limae/actions/workflows/ci.yml/badge.svg)](https://github.com/Luolc/limae/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Luolc/limae)](https://github.com/Luolc/limae/releases)
[![License](https://img.shields.io/github/license/Luolc/limae)](LICENSE)

简体中文 | [English](README.en.md)

一个 Markdown linter，从中文技术写作的排版规则起步：中英文之间的空格、半角标点、括号的形态与它外侧的空格，逐条检查，大部分能直接改好。名字取自贺拉斯 (Horace) 的 *limae labor* —— 「锉刀的功夫」，把写完的稿子一遍遍磨到干净。

## 它改什么

原稿：

```markdown
这个CLI工具支持3种模式,提供2GB缓存(默认关闭)。
```

`limae draft.md` 报出来的是：

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

不经包管理器就用安装脚本，它按 `uname` 选本机的 tarball、用 Release 旁边的 `.sha256` 边车校验，装进 `~/.local/bin`：

```sh
curl -fsSL https://limae.luolc.com/install.sh | sh
```

装到别处、钉一个版本、或者一个文件都别写，三个环境变量各管一件事，说明见 [Rust 分发预演](docs/knowledge/rust-binary-distribution.md)「一键安装的旋钮与 PATH 行为」。

Python 或 Node 项目从各自的包管理器装同一个二进制：

```sh
uv add --dev limae      # 或 pip install limae
npm i -D @limae/cli     # 之后 npx limae
```

预构建二进制只有四个平台：Linux x86_64 / aarch64 (静态 musl，glibc 系统照跑)、macOS x86_64 / arm64。**Windows 没有预构建二进制**，从源码装：

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

退出码：干净是 0，有 error 级发现是 1，配置错误另有一档 (正本是 [`spec/rules.md`](spec/rules.md)「退出码」)。

`--all` 走的是**文件系统**而不是 git 索引 (index)，刚写出来、还没 `git add` 的文件照查；点开头的目录 (`.github/`、`.agents/`) 也在内，唯一按名字排除的是 `.git`，被 `.gitignore` / `.ignore` / `.limae-ignore` 忽略的不查。

单点误报就地关掉，独占一行的 HTML 注释：

```markdown
<!-- limae-disable-next-line zh-typography-4 -->
サンプルIT推進部の例
```

整份文件不该被检查就放一个 `.limae-ignore`，语法与 `.gitignore` 相同。两者的语义正本都在 [`spec/rules.md`](spec/rules.md)「行内指令」「忽略文件」。

## 接 pre-commit

默认走镜像仓 [`Luolc/limae-pre-commit`](https://github.com/Luolc/limae-pre-commit)，`rev` 固定到它的一个 tag ([tag 列表](https://github.com/Luolc/limae-pre-commit/tags))。消费方什么都不用额外装，pre-commit 在自己的缓存里装下 PyPI 上那个 wheel 里的预构建二进制，不编译：

```yaml
repos:
  - repo: https://github.com/Luolc/limae-pre-commit
    rev: <tag>     # 一个 tag 对应一个 limae 版本：v0.13.2 即 limae 0.13.2
    hooks:
      - id: limae
```

**Windows 上换本仓这条**：Release 没有 Windows 产物，镜像仓那条在 Windows 上装不上；本仓这条用 Cargo 现编，任何平台都能用，代价是机器上要有 Rust 工具链、第一次要等一次编译。

```yaml
repos:
  - repo: https://github.com/Luolc/limae
    rev: <tag>
    hooks:
      - id: limae
```

两条 hook 的 id 都是 `limae`，行为相同：默认只检查、不修复，要自动修复就加 `args: ["--fix"]`。为什么是两个仓、Python 与 Rust 两条 language 合同各自要求什么，见 [Rust 分发预演](docs/knowledge/rust-binary-distribution.md)。

**规则开关一律写进配置文件，不要放进 `args:`** —— `--disable` / `--enable` 是**替换**配置文件而不是叠加，命令行上一旦出现其中之一，`limae.toml` 与 `pyproject.toml` 的 `[tool.limae]` 根本不会被读，而且不报错、不警告 (正本是 [`spec/rules.md`](spec/rules.md)「来源与优先级」)。

## 配置

仓库根放一个 `limae.toml`，或者写进 `pyproject.toml` 的 `[tool.limae]` 表，两者键结构相同：

```toml
disable = ["zh-typography-3"]        # 关掉某条规则
enable = ["zh-typography-9"]         # 打开某条默认关闭的规则
enable_experimental = true           # 一次纳入全部实验规则
severity = { zh-typography-8 = "warning" }   # 覆盖单条规则的严重度
```

启用集 = ((默认集 ∪ experimental 集) ∪ `enable`) − `disable`，experimental 集只在 `enable_experimental = true` 时并入。全部的键、发现顺序与配置错误的判定，正本在 [`spec/rules.md`](spec/rules.md)「配置」。

## 规则

规则 id 按家族分前缀：`zh-typography` 中文排版、`zh-tell` / `en-tell` AI 腔、`zh-word` 术语选词。中文排版一族默认启用，全是可修复的 `error`；AI 腔与术语选词是 experimental，默认关闭、默认 `warning` —— 打开它们不会把 CI 变红。规则不分文档语言，英文 tell 出现在中文文档里同样报。

每条规则的判定、例句与三轴属性 (可修复性 / 严重度 / 成熟度) 都在 [`spec/rules.md`](spec/rules.md)；判定用的词表在 [`spec/wordlists/`](spec/wordlists/)，加一条词只改那里、不动任何实现。AI 腔那一族的取词标准与词条释义另有一部[机器文言大词典](https://limae.luolc.com/)。

规则先于实现：每条规则是一段语言无关的规范，任何实现都对着 [`spec/fixtures/`](spec/fixtures/) 的同一套黄金 fixture (golden fixtures) 跑 (ADR-[0001](docs/adr/0001-standalone-repo-spec-first-shared-fixtures.md) / [0006](docs/adr/0006-rule-grading-fixability-severity-experimental.md) / [0007](docs/adr/0007-ai-tells-rule-family-and-wordlists.md))。

## LLM 语义润色 (`limae polish`)

`limae polish -` 从 stdin 读一段文本，交给一个本机已登录的 CLI (Claude Code / Codex / Grok，也可以自己配一条命令) 改写，结果写 stdout：

```sh
limae polish - < draft.md > polished.md
```

它与 `check` / `--fix` 是两段互不触发的东西：排版由规则确定性地修，语义由模型改写，`polish` 永远不进 CI 的 required check。引擎怎么选、凭证怎么处理、送出去的到底是哪些字节，见 [ADR-0008](docs/adr/0008-limae-polish-cli.md) 与 [引擎行为实测](docs/research/polish-engine-cli-behavior.md)。

## 参与开发

开发者请读 [`AGENTS.md`](AGENTS.md)：目录约定、质量门与它们的顺序都在那里。决策记录在 [`docs/adr/`](docs/adr/)，待办在 [`docs/tracker.md`](docs/tracker.md)。

## 许可证

Apache License, Version 2.0，见 [`LICENSE`](LICENSE)。
