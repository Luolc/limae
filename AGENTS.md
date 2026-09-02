# Agent 守则 (本仓专属)

任何 AI agent / 会话在本仓库工作前必须先读本文件。`CLAUDE.md` 是指向本文件的软链接 —— 永远编辑本文件。

## 语言规范

面向人读的仓内文字 (文档、决策记录、日志) 用中文；代码、代码注释与 commit message 沿用英文工程惯例，与用户用什么语言说话无关。条款、规范、官方定义等关键引用直接 quote 原文而不强行翻译，可附一句概述 —— 准确性优先于翻译。一个事实只放在一处权威位置，其他地方链过去，不重复维护。不写会过期的值 (版本号、价格、当前状态)，除非注明获取日期。

中文行文规则：专有名词在首次出现和关键位置中英对照，如「幂等 (idempotent)」，没有通行中文译名或不确定时以英文原词为准；逗号、句号、顿号、分号、冒号用全角，不与半角混用；括号用半角，外侧与相邻文字之间留一个空格、内侧不留，如「监控机 (monitor)」；代码、路径与纯英文内容保留半角标点。

## 隐私边界

本仓是 public 仓库，工具代码、无业务数据：仓内绝不能出现任何凭证或个人身份信息 (PII)。这是硬性红线，不因仓库访问控制而放松 —— 历史里一旦出现就必须先轮换凭证、再清理历史。公开发表作品的作者署名与公开链接属引用 (attribution)，不在 PII 之列。

**测试 fixture 只用合成数据** (如 `ACME` / `$1,000` / `Foo`)：不得出现用户真实密钥、账户或身份信息，即使只是当作待检查的字符串。

**敏感值不进任何文本，只按位置指代**：brief、PR 描述、commit message、评论等一切文本里都不写敏感值本身，只指代其位置 (文件、行号、测试 id 等)；脱敏映射只留在裁决人手里，不经任何 agent 间通道传递。

## 目录约定

布局参考 [ruff](https://github.com/astral-sh/ruff) 仓：每种语言的实现都以仓根为项目根，源码进各自的子目录；规范与 fixture 独立于任何实现。

- **规则规范与黄金 fixture 语言无关、所有实现共用**，放仓根 `spec/` (规范 `spec/rules.md`，黄金集 `spec/fixtures/`)；不放进任何单一实现的私有目录 (`src/`、`tests/`)。位置与理由见 `docs/adr/0001-standalone-repo-spec-first-shared-fixtures.md`。
- **Python 参考实现 (reference implementation) 在仓根**：`pyproject.toml`、`src/limae/`、`tests/`；包 `limae`，命令 `limae` (过渡期保留 `lo-md-lint` 别名)；用 uv 管理，锁文件 `uv.lock` 全仓唯一。放仓根而不是 `python/` 子目录，是因为 pre-commit `language: python` 与 `uvx --from git+…` 都把仓根当作可安装的 Python 项目。
- **将来新增语言实现同样以仓根为项目根**，用该语言自己的原生工具链，都对着同一套 `spec/` 跑；具体布局等到真的写的时候再定。
- **内容类 Markdown 在 `docs/`**：`docs/adr/` (决策记录)、`docs/knowledge/` (操作手册)、`docs/research/` (调研)。
- 项目级 skill 只放在 `.agents/skills/<name>/`，见 `.agents/skills/README.md`。

## 质量标准 (quality bar)

CI (`.github/workflows/ci.yml`，required check 名为 `check`) 在 PR 与 main 上跑同一套检查；本地就是这两条：

```sh
uv run pre-commit run --all-files   # gitleaks + ruff + pyink + isort + basedpyright + pydoclint + uv-lock + 用本仓 linter lint 本仓的 Markdown
uv run pytest -q                    # 测试套件 (含对 `spec/fixtures/` 黄金集的比对)
```

- 首次 clone 后先 `uv run pre-commit install`：钩子是本地状态，不随仓库分发，漏装则 commit 无任何拦截。
- **凭证扫描 (gitleaks) 是这里唯一守红线而不是守风格的钩子** (见「隐私边界」)：别的检查上跳过一次只是欠一次格式，这一环上跳过一次就是让凭证有机会进这个 public 仓。`--no-verify` 会跳过全部 hooks，包括 gitleaks；如确需使用，必须在提交前 (文件已暂存后) 手工执行 `gitleaks git --staged --redact --no-banner --verbose .`。机器限频、钩子跑得慢的时候尤其要记得这一步 —— 机制不会替你拦住，这条只能靠人执行。
- 它扫的是**暂存区** (`--staged`)，也就是正要提交的这份内容：扫历史看不见它，而历史里的凭证已经跑掉了，只剩轮换与清史。命中时 `--redact` 只打印规则名与文件行号，不把命中的值打进终端或会话记录，这样验证凭证泄漏时也不会二次泄漏。版本钉在 `.pre-commit-config.yaml` 的 `rev`，pre-commit 用 Go 从源码装：首次约两分钟，之后每次约 2 秒。**升这个 `rev` 时必须重新核对上游 entry 仍带 `--staged`**：entry 会随 tag 变，`--staged` 一旦丢掉，钩子就退化成扫历史 —— 而扫历史看不见刚 `git add` 的 token，绿得像样却什么也没防住，正是这套配置要堵的那个洞。
- CI 的 `Credential scan` 那一步是同一把扫描的另一半：CI 没有暂存区，它改扫已经落进历史的内容 (整份 clone，`fetch-depth: 0`)，兜住漏装钩子、或绕过钩子推上来的分支。它的版本与校验和跟 `.pre-commit-config.yaml` 的 `rev` 一起动，两处必须同版本。
- 本仓用自己的 linter 检查自己的 Markdown (dogfooding)。规则一改、文档标红时，先判断是文档错还是规则错：检查器必然存在误报与漏报，判断是检查器错了就直接修它 (commit message 里说明理由)，规则确实错了就改规则与 `spec/` 下的规范和黄金集，不改文档迁就；拿不准的案例交给维护者裁决。

## 合并

LGTM 后从评论取 approved SHA，确认本地 tip 与之相同 (`git rev-parse HEAD`)，`gh pr merge N --squash --delete-branch --match-head-commit <approved-sha>`。仓库设上 required check 之后可加 `--auto`：`check` 绿自动进 main，红永不合并；轮询 PR 状态时同时监听失败态 (FAILURE / CANCELLED / TIMED_OUT 即退出)，不要只等 MERGED。

## 多 agent 协作

- `AGENTS.md` 与 `.agents/skills/` 的改动单独成任务且串行，不与其它改动混在同一个改动里，避免多个 agent 同时改同一份文件冲突。
- backlog 写在 `docs/tracker.md`，合入后记账，别处不重复。
- 依赖锁文件是 `uv.lock` (对应仓根 `pyproject.toml`)，同一时刻只允许一个改依赖的任务在跑。
- 如果你的运行环境里已经配置了通用的 PR review / Python review 一类审查规范，审查本仓改动时可以直接参照使用；本仓目前没有额外的仓库专属加严规则。
- 如果用 herdr 一类工具在多个 agent 间协调：终端 tab 的标签保持简洁 (`orchestra`、`shell`，或一个裸任务名)，而实际跑 agent 的 pane 标签则加上仓库简称前缀 (如 `limae-orchestra`、`limae-shell`)，这样多个仓库的 agent 混跑时才分得清哪个进程属于哪个仓。
