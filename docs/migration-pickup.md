# 接续说明 (临时文档，接续后删除)

> **临时文档。** 写于 2026-08-31，为开发机迁移 (dev-oregon) 而写：迁移后所有会话 transcript 丢失，新会话只有这个仓库可读。新会话在新机上开工、把状态接上之后，删掉本文件 (`docs/migration-pickup.md`)，它不是本仓的长期文档。

本文只写「没有任何本会话记忆的人 / agent 要怎么接着干」。所有长期事实的正本都在别处，本文只指路。

## 一、这个仓是什么

`lo-md-lint`：从中文技术写作排版规则起步的 Markdown linter，public 仓 (`git@github.com:Luolc/lo-md-lint.git`)。规范先于实现，黄金 fixture 跨实现共用，Python 版是参考实现，长期主实现方向是 Rust。

关键路径，按「先读什么」排：

| 路径 | 是什么 |
| --- | --- |
| `AGENTS.md` (`CLAUDE.md` 软链到它) | 本仓专属守则：隐私边界、目录约定、质量门、合并方式、多 agent 约定。开工前必读 |
| `README.md` | 对外说明：定位、现状、怎么当 pre-commit hook 用、怎么开关规则 |
| `spec/rules.md` | **规则语义的正本**：R1–R11、全局豁免、修复顺序、配置模型 |
| `spec/fixtures/` | 黄金集 (golden fixtures)；格式与 runner 判定见 `spec/README.md` |
| `src/lo_md_lint/`、`tests/` | Python 参考实现与测试；`tests/test_fixtures.py` 是驱动黄金集的薄 runner |
| `docs/tracker.md` | **backlog 的正本**：还没做的事，每条一句去向；由 `mdlint-orchestra` 在合入后记账 |
| `docs/adr/` | 决策记录 0001–0005；0005 是定位与愿景 |
| `docs/research/` | 两份调研，是规则与愿景的证据链 |
| `docs/incidents/` | 事故记录 |

质量门只有两条命令，CI (required check 名 `check`) 跑的是同一套：

```sh
uv run pre-commit run --all-files
uv run pytest -q
```

新克隆之后先 `uv sync` 与 `uv run pre-commit install` —— 钩子是本地状态，不随仓库分发。

## 二、当前版本与 tag 语义

`pyproject.toml` 的版本是 **0.3.0**，最新 tag 是 `v0.3.0`。消费方引用的永远是 tag 而不是 `main` —— 不改版本的改动 (比如文档) 合入后 `main` 会领先于最新 tag，那不代表有新版本可升。四个 tag 的语义：

| tag | 内容 | 对消费方是否 no-op |
| --- | --- | --- |
| `v0.1.0` | 迁入 + pre-commit hook manifest | — |
| `v0.1.1` | R3 修正 (英文 token 内括号不报、`)(` 补空格) | 否 (判定改了，一处少报一处多报) |
| `v0.2.0` | 规则 flag 化 + toml 配置 | 是，no-op |
| `v0.3.0` | R1–R11 全集 (空格家族 R4–R9、标点家族 R10–R11、R1 补句号) | **否** |

**升到 `v0.3.0` 不是 no-op**：默认集从 R1–R3 扩到 R1–R8、R10、R11 (R9 默认关)，消费方仓库升 `rev` 之后大概率会新标出一批违规。升级方要么接受 `--fix` 的改动并人工过一遍 diff，要么用 `disable` 关掉不想要的规则。

## 三、进行中 / 等待中的事

- **消费方升 `rev` 不归本仓**：`wealth-management`、`machine-setup`、`butler` 的接入与升级由 `butler-orchestra` 派各仓自己的 orchestra 做。本仓只发 tag、不改别人的仓。
- `machine-setup` 目前停在 `v0.1.1`；它有一个「用反引号包住 credential(s)」的权宜之计，自 `v0.1.1` 起可以撤掉。
- **本仓没有其它在飞的任务**：迁移前最后一个 PR 就是引入本文件的那个。没有等审的 PR、没有半成品分支、没有待回报的 agent。
- backlog 全部在 `docs/tracker.md`，没有只存在于聊天记录里的待办 —— 如果本文与 tracker 冲突，以 tracker 为准。

## 四、新会话从哪接

1. `git fetch origin`，确认 `main` 与 `origin/main` 一致；开工前 `git branch --show-current` 确认不在 `main` 上。
2. 读 `~/.agents/AGENTS.md` (跨仓守则) 与本仓 `AGENTS.md`，再读 `docs/tracker.md` 与 `docs/adr/`。
3. 有派活就按派活做；没有就从 `docs/tracker.md` 里挑一条，跟用户确认优先级之后再动手。tracker 的条目只写「去向一句话」，动手前要先把设计定案 (需要就起新 ADR)。
4. 协作命名照旧：herdr workspace 名即仓名 `lo-md-lint`，简称 `mdlint`，常驻 `mdlint-orchestra` / `mdlint-shell`，临时任务对是 `<slug>-impl` / `<slug>-review`，通用词 slug 要带 `mdlint-` 前缀。
5. 一切非琐碎改动走分支 + PR，本地过完两条质量门再开 PR，拿到 `Verdict: LGTM` 再按 approved SHA 合并 (`AGENTS.md`「合并」一节)。

接上之后删掉本文件。
