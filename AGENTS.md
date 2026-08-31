# Agent 守则 (本仓专属)

全局守则见 `~/.agents/AGENTS.md` (由 `machine-setup` 仓库经 chezmoi 管理，三家 harness 的用户级入口都软链到它)；本文件只写本仓专属。任何 AI agent / 会话在本仓库工作前必须先读本文件。`CLAUDE.md` 是指向本文件的软链接——永远编辑本文件。

**语言：情况 A (中文仓)**——见全局守则「语言规范」。

## 隐私边界

本仓是 public 仓库，工具代码、无业务数据：仓内绝不能出现任何凭证或个人身份信息 (PII)——全局守则「安全红线」在这里没有「仓库访问控制」兜底，历史里一旦出现就按红线处理 (先轮换、再清历史)。

**测试 fixture 只用合成数据** (如 `ACME` / `$1,000` / `Foo`)：不得出现用户真实理财、账户或身份信息，即使只是当作待检查的字符串。

**敏感值不进任何文本，只按位置指代**：brief、PR 描述、commit message、评论等一切文本里都不写敏感值本身，只指代其位置 (文件、行号、测试 id 等)；脱敏映射只留在裁决人手里，不经任何 agent 间通道传递 (事故记录见 `docs/incidents/2026-08-31-sensitive-values-in-commit-text.md`)。

## 目录约定

布局与 ruff 仓相同：每种语言的实现都以仓根为项目根，源码进各自的子目录；规范与 fixture 独立于任何实现。

- **规则规范与黄金 fixture 语言无关、所有实现共用**，放仓根 `spec/` (规范 `spec/rules.md`，黄金集 `spec/fixtures/`)；不放进任何单一实现的私有目录 (`src/`、`tests/`、`crates/`)。位置与理由见 `docs/adr/0001-standalone-repo-spec-first-shared-fixtures.md`。
- **Python 参考实现 (reference implementation) 在仓根**：`pyproject.toml`、`src/lo_md_lint/`、`tests/`；包 `lo_md_lint`，命令 `lo-md-lint`；用 uv 管理，锁文件 `uv.lock` 全仓唯一。放仓根而不是 `python/` 子目录，是因为 pre-commit `language: python` 与 `uvx --from git+…` 都把仓根当作可安装的 Python 项目。
- **将来其它语言实现同样以仓根为项目根**：如 Rust 在仓根放 `Cargo.toml`，crate 源码在 `crates/` 之类的子目录；各用自己语言的原生机制，都对着同一套 `spec/` 跑。
- **内容类 Markdown 在 `docs/`**：`docs/adr/` (决策)、`docs/knowledge/` (手册)、`docs/research/` (调研)、`docs/incidents/` (事故记录)，按全局守则「决策记录」三分。
- 项目级 skill 正本在 `.agents/skills/<name>/`，`.claude/skills/<name>` 逐 skill 软链，见 `.agents/skills/README.md`。

## 质量门 (quality bar)

CI (`.github/workflows/ci.yml`，required check 名为 `check`) 在 PR 与 main 上跑同一套检查；本地就是这两条：

```sh
uv run pre-commit run --all-files   # ruff + pyink + isort + basedpyright + pydoclint + uv-lock + 用本仓 linter lint 本仓的 Markdown
uv run pytest -q                    # 测试套件 (含对 `spec/fixtures/` 黄金集的比对)
```

- 首次 clone 后先 `uv run pre-commit install`：钩子是本地状态，不随仓库分发，漏装则 commit 无任何拦截。
- 本仓用自己的 linter 检查自己的 Markdown (dogfooding)。规则一改、文档标红时，先判断是文档错还是规则错，按全局守则「linter 是工具，不是法律」处理：规则错就改规则与 `spec/` 下的规范和黄金集，不改文档迁就。

## 合并

LGTM 后从评论取 approved SHA，确认本地 tip 与之相同 (`git rev-parse HEAD`)，`gh pr merge N --squash --delete-branch --match-head-commit <approved-sha>`。仓库设上 required check 之后可加 `--auto`：`check` 绿自动进 main，红永不合并；轮询 PR 状态时同时监听失败态 (FAILURE / CANCELLED / TIMED_OUT 即退出)，不要只等 MERGED。

## 多 agent 协作 (本仓实例)

- orchestra 简称 `mdlint`：常驻 `mdlint-orchestra` / `mdlint-shell`；任务对、research 与将来的仓级 skill (如 `mdlint-pr-review`) 一律用这个前缀。仓名 `lo-md-lint` 与包名 / 命令名不受此影响。
- 本仓暂无 tracker；依赖锁文件是 `uv.lock` (与仓根 `pyproject.toml`)。
- 审查标准 = 用户级 `pr-review` skill (`~/.agents/skills/pr-review`，machine-setup 分发)，触碰 Python 时叠加用户级 `python-review`；本仓暂无仓级 `mdlint-pr-review` / `mdlint-python-review`，需要加严时再建，仓级只写增量。不指向任何其它仓库的文件。
