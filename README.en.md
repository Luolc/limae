# limae

[![CI](https://github.com/Luolc/limae/actions/workflows/ci.yml/badge.svg)](https://github.com/Luolc/limae/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Luolc/limae)](https://github.com/Luolc/limae/releases)
[![License](https://img.shields.io/github/license/Luolc/limae)](https://github.com/Luolc/limae/blob/main/LICENSE)

[简体中文](https://github.com/Luolc/limae/blob/main/README.md) | English

A Markdown linter that starts from the typography of Chinese technical writing: spacing between CJK and Latin text, halfwidth punctuation next to CJK, the shape of parentheses and the spaces around them. It checks rule by rule, and fixes most of what it finds. The name comes from Horace's *limae labor*, "the work of the file" — the slow filing-down of a finished draft.

## What it does

A draft:

```markdown
这个CLI工具支持3种模式,提供2GB缓存(默认关闭)。
```

`limae draft.md`:

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

After `limae --fix draft.md`:

```markdown
这个 CLI 工具支持 3 种模式，提供 2 GB 缓存 (默认关闭)。
```

## Install

Every channel below installs the same prebuilt binary that ships in the [GitHub Release](https://github.com/Luolc/limae/releases).

```sh
brew install luolc/tap/limae            # macOS and Linuxbrew
curl -fsSL https://limae.luolc.com/install.sh | sh
uv add --dev limae                      # or: pip install limae
npm i -D @limae/cli                     # then: npx limae
```

Prebuilt binaries cover Linux x86_64 / aarch64 (static musl) and macOS x86_64 / arm64. There is no Windows binary; on Windows, build from source:

```sh
cargo install --locked --git https://github.com/Luolc/limae --tag <tag> limae
```

## Use

```sh
limae <file>...   # check the given files
limae --all       # check every Markdown file below the current directory
limae --fix --all # fix first, then check
```

Rule selection goes in a `limae.toml` (or the `[tool.limae]` table of a `pyproject.toml`):

```toml
disable = ["zh-typography-3"]
enable_experimental = true
```

It also ships a [pre-commit](https://pre-commit.com/) hook, a `.limae-ignore` file, inline `<!-- limae-disable-next-line -->` comments, and a `limae polish` subcommand that hands prose to a locally logged-in LLM CLI.

## Where the details are

This is a summary. The full documentation is in Chinese, because the rules it enforces are rules of Chinese typography.

- [Chinese README](https://github.com/Luolc/limae/blob/main/README.md) — installation options, pre-commit setup, configuration, `limae polish`.
- [`spec/rules.md`](https://github.com/Luolc/limae/blob/main/spec/rules.md) — the language-agnostic rule specification: every rule, its examples, its fixability / severity / maturity, the configuration keys, inline directives and ignore files. Implementations are checked against the shared golden fixtures in [`spec/fixtures/`](https://github.com/Luolc/limae/tree/main/spec/fixtures).
- [`AGENTS.md`](https://github.com/Luolc/limae/blob/main/AGENTS.md) — for contributors: layout, quality gates, conventions.
- [limae.luolc.com](https://limae.luolc.com/) — a dictionary (in Chinese) of the odd words LLMs write in Chinese, which is where the experimental AI-tell rules get their wordlists.

## License

Apache License, Version 2.0. See [`LICENSE`](https://github.com/Luolc/limae/blob/main/LICENSE).
