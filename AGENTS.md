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

- **规则规范与黄金 fixture 语言无关、所有实现共用**，放仓根 `spec/`：规范在 `spec/rules.md`，黄金集在 `spec/fixtures/`，AI 中文词典在 `spec/lexicon/zh.toml`，prompt spec 在 `spec/polish/`，规则词表在 `spec/wordlists/`；不放进任何单一实现的私有目录 (`src/`、`tests/`)。各部分的职责与格式见 `spec/README.md`，位置与理由见 `docs/adr/0001-standalone-repo-spec-first-shared-fixtures.md`。
- **Python 参考实现 (reference implementation) 在仓根**：`pyproject.toml`、`src/limae/`、`tests/`；包 `limae`，命令 `limae-python`；用 uv 管理，锁文件 `uv.lock` 全仓唯一。ADR-0015 阶段 E 之后它是 deprecated 的参考实现 —— 正式命令是 Rust binary `limae`，Python 入口改名让开这个名字，但入口与测试继续保留、继续跑，它是差分臂的对照一侧。放仓根而不是 `python/` 子目录，是因为 pre-commit `language: python` 与 `uvx --from git+…` 都把仓根当作可安装的 Python 项目。
- **Rust 是仓根 Cargo package，也是正式实现**：根 `Cargo.toml`、`Cargo.lock` 与 `rust-toolchain.toml` 配套，源码在 `rust/`，集成测试在 `rust/tests/`；唯一发布的 binary 是 `limae`，开发期工具 `diff-probe`、`render-lexicon` 与 `lint-lexicon` 的源码在 `rust/tools/`、以 `[[example]]` target 声明 —— 目录名说它们是什么 (与仓根 `tools/` 同义：凡叫 `tools` 的都是开发期工具、不进发布)，`[[example]]` 是保证它们不随 `cargo install` 分发的机制；两者不必同名，因为 `Cargo.toml` 把每个 `path` 写死，不走 Cargo 对 `examples/` 的自动发现；Rust 的 unit test、integration test 与 doctest 使用 Cargo 内建测试框架，仍对着同一套 `spec/` 与黄金 fixture 跑。toolchain pin 的唯一配置来源是 `rust-toolchain.toml`，不在此重复版本值，详见 ADR-0015 §六。根 `Cargo.toml` / `Cargo.lock` / `rust-toolchain.toml`、`rust/lib.rs` 接线、CI 与共享测试入口由当时的集成任务单独持有；加依赖与接线串行。
- **静态站点在 `site/`**：Cargo example `render-lexicon` (`rust/tools/render_lexicon.rs`，版式在 `rust/templates/lexicon.html`) 从 `spec/lexicon/zh.toml` 生成 `site/index.html`，`cargo run --example render-lexicon` 重新生成；`site/` 只放生成产物。可编辑的页面正文 (标题、副标题、引子、判据与门槛、词条) 在 toml；模板保留结构性标签 (栏目名、白 / 解 / 病 / 原 / 改这类标签、生成说明) 与版式。
- **内容类 Markdown 在 `docs/`**：`docs/adr/` (决策记录)、`docs/knowledge/` (操作手册)、`docs/research/` (调研)。
- 项目级 skill 只放在 `.agents/skills/<name>/`，见 `.agents/skills/README.md`。

## 质量标准 (quality bar)

CI (`.github/workflows/ci.yml`，required check 名为 `check`) 在 PR 与 main 上覆盖以下十五道质量门；本地 push 前按 CI 的门顺序裸跑：

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo test --locked
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked
uv run pytest -q
tools/check_lexicon_render.sh
cargo run --quiet --locked --example lint-lexicon -- spec/lexicon/zh.toml
tools/check_lexicon_lint.sh
cargo build --locked --bin limae --example diff-probe
uv run pytest -q --rust-bin target/debug/limae
uv run pre-commit run --all-files --show-diff-on-failure
tools/check_precommit_consumer.sh
tools/check_rust_package.sh package
tools/check_rust_package.sh target x86_64-unknown-linux-gnu
tools/check_rust_package.sh target x86_64-unknown-linux-musl
```

- 首次 clone 后先 `uv run pre-commit install`：钩子是本地状态，不随仓库分发，漏装则 commit 无任何拦截。
- **凭证扫描 (gitleaks) 是这里唯一守红线而不是守风格的钩子** (见「隐私边界」)：别的检查上跳过一次只是欠一次格式，这一环上跳过一次就是让凭证有机会进这个 public 仓。`--no-verify` 会跳过全部 hooks，包括 gitleaks；如确需使用，必须在提交前 (文件已暂存后) 手工执行 `gitleaks git --staged --redact --no-banner --verbose .`。机器限频、钩子跑得慢的时候尤其要记得这一步 —— 机制不会替你拦住，这条只能靠人执行。
- 它扫的是**暂存区** (`--staged`)，也就是正要提交的这份内容：扫历史看不见它，而历史里的凭证已经跑掉了，只剩轮换与清史。命中时 `--redact` 只打印规则名与文件行号，不把命中的值打进终端或会话记录，这样验证凭证泄漏时也不会二次泄漏。版本钉在 `.pre-commit-config.yaml` 的 `rev`，pre-commit 用 Go 从源码装：首次约两分钟，之后每次约 2 秒。**升这个 `rev` 时必须重新核对上游 entry 仍带 `--staged`**：entry 会随 tag 变，`--staged` 一旦丢掉，钩子就退化成扫历史 —— 而扫历史看不见刚 `git add` 的 token，绿得像样却什么也没防住，正是这套配置要堵的那个洞。
- CI 的 `Credential scan` 那一步是同一把扫描的另一半：CI 没有暂存区，它改扫已经落进历史的内容 (整份 clone，`fetch-depth: 0`)，兜住漏装钩子、或绕过钩子推上来的分支。它的版本与校验和跟 `.pre-commit-config.yaml` 的 `rev` 一起动，两处必须同版本。
- **门列表里有两道 pre-commit，方向相反，别把它们看成一件事**：第 11 道 `uv run pre-commit run --all-files` 是**我们对自己** —— 拿本仓的钩子扫本仓的文件。第 12 道 `tools/check_precommit_consumer.sh` 是**别人对我们**：它是一次针对 pre-commit 这条分发路径的集成测试，从当前 `HEAD` 克隆出上游副本，在一个全新的空消费仓与隔离的 `PRE_COMMIT_HOME` 里装钩子，`limae` (Rust) 与 `limae-python` (回退路径) 各跑一遍 install / 违规 / 修复 / 修后 clean 四臂。它吃的是**已提交的 `HEAD`**，所以跑它之前先 commit，跑完核对退出 0 且末行 revision 等于 `HEAD`。分辨力来自 PATH 上那两个同名失败探针 (`limae` 退 97、`limae-python` 退 98)：任何一臂真绿了，跑的就一定是 pre-commit 自己缓存里装出来的那个，而不是机器上碰巧存在的某个 `limae`。详见 [Rust 分发预演](docs/knowledge/rust-binary-distribution.md)。
- **词典的中文正文也归自己的 linter 管**：第 7 道门与 `limae-lexicon` 钩子把 `spec/lexicon/*.toml` 里给人读的字段 (标题、引子、判据与门槛、每条词条的解释与例句) 送进同一套规则，报的是真实的 toml 行号；实现是 Cargo example `lint-lexicon` (`rust/tools/lint_lexicon.rs`)。两条判断写在 `.pre-commit-config.yaml` 那条钩子的注释里：**选文件按路径不按扩展名** (`^spec/lexicon/.*\.toml$`，`Cargo.toml` 这类配置一个都不碰，将来的 `en.toml` 自动进来)；**只开排版规则，实验规则全关** —— `examples[].before` 装的就是这部词典收录的病句，开着等于让词典自己撞自己 (这个理由只对 `examples[].before` 成立，对其余散文字段是保守取舍，见该文件模块注释)。
- **第 8 道门守的是 linter 自己，不是词典内容**：`spec/lexicon/zh.toml` 是干净的，所以第 7 道门在行号映射正确、错误、乃至整个删掉时都打印 `OK`。`tools/check_lexicon_lint.sh` 拿行号固定的 fixture 驱动这个工具，逐条断言报出来的 toml 行；它把真词典再跑一遍是**自己的对照臂** (其余各臂都断言「应该报」，一个什么都报的工具能全过)，不是第二道内容门。**只要上面第 3 道门那条 `cargo test` 不带 `--all-targets`，example 里的 `#[cfg(test)]` 就是死的**：那条命令只编译 example，不跑它们的测试 (2026-09-10 本机两臂实测 —— 一个恒失败的 `#[test]` 在 `cargo test --locked` 下退出 0 且输出里连测试名都不出现，`--all-targets` 下退出 101)。所以这类回归测试写成 `tools/` 下的门，别写进 example；写在那里的是一条永远不会红的空洞，比不写更坏，因为它看起来像一道门。**这句话带着一个可以核的前提，别把它当常驻现状**：门列表里那条一旦带上 `--all-targets`，前提就不成立，**连这一句一起删掉** —— 一句描述已修问题的现状说明，读者会照着它绕路。改的时候注意 `--all-targets` 不含 doctest (`cargo test --help` 原文 `Test all targets (does not include doctests)`)，必须同时保留一条 `--doc`，否则今天那 7 条 doctest 会静默消失，等于用一个新洞换掉旧洞。
- 本仓用自己的 linter 检查自己的 Markdown (dogfooding)。规则一改、文档标红时，先判断是文档错还是规则错：检查器必然存在误报与漏报，判断是检查器错了就直接修它 (commit message 里说明理由)，规则确实错了就改规则与 `spec/` 下的规范和黄金集，不改文档迁就；拿不准的案例交给维护者裁决。
- **Rust 质量门**：默认 `uv run pytest -q` 保持 Python-only；差分臂先构建 `limae` 与 `diff-probe`，再以 `--rust-bin target/debug/limae` 显式运行 Python / Rust 双臂。`cargo test --locked` 验收 doctest，`cargo doc` 单独验收文档构建。分发门所需的 Rust 工具链与 musl target 见 [Rust 分发预演](docs/knowledge/rust-binary-distribution.md)；CI 的 target 安装是前提，不计作质量门。commit hook 仍只放秒级检查，完整构建与分发重检查不进 hook：Rust 侧进 hook 的只有 `cargo fmt --check` (2026-09-09 本机实测 0.34 s，含 pre-commit 自身开销约 1.1 s)，`cargo clippy` 要编译全部 target、远超这个预算，留在 push 前全量与 CI —— 判据是「够不够快」，不是「是不是 Rust」。
- **Rust lint 默认**：根 `[lints.rust]` 设 `unsafe_code = "forbid"` 与 `unused_must_use = "deny"`，根 `[lints.clippy]` deny `unwrap_used`、`expect_used`、`dbg_macro`、`print_stdout` 与 `print_stderr`。测试默认返回 `Result`，不开全局 `unwrap` / `expect` 例外；确需窄范围例外时，只包住所需表达式，并写明 lint 名与理由。

## 合并

LGTM 后从评论取 approved SHA，确认本地 tip 与之相同 (`git rev-parse HEAD`)，`gh pr merge N --squash --delete-branch --match-head-commit <approved-sha>`。仓库设上 required check 之后可加 `--auto`：`check` 绿自动进 main，红永不合并；轮询 PR 状态时同时监听失败态 (FAILURE / CANCELLED / TIMED_OUT 即退出)，不要只等 MERGED。

## 多 agent 协作

- `AGENTS.md` 与 `.agents/skills/` 的改动单独成任务且串行，不与其它改动混在同一个改动里，避免多个 agent 同时改同一份文件冲突。
- backlog 写在 `docs/tracker.md`，记账规则见该文件开头 (本仓对全局守则有意偏离)，别处不重复。
- 依赖锁文件是 `uv.lock` (对应仓根 `pyproject.toml`)，同一时刻只允许一个改依赖的任务在跑。
- 如果你的运行环境里已经配置了通用的 PR review / Python review 一类审查规范，审查本仓改动时可以直接参照使用；本仓目前没有额外的仓库专属加严规则。
- Rust 设计、实现 brief、自审与正式审查均加载用户级 `rust-review` 与 `pr-review`，再叠加本仓适用规则；不复制 skill 正文或引用其它仓库路径。
- 如果用 herdr 一类工具在多个 agent 间协调：终端 tab 的标签保持简洁 (`orchestra`、`shell`，或一个裸任务名)，而实际跑 agent 的 pane 标签则加上仓库简称前缀 (如 `limae-orchestra`、`limae-shell`)，这样多个仓库的 agent 混跑时才分得清哪个进程属于哪个仓。
- `~/scratch/limae/` 下的产物落盘时首行写明归属 (任务 slug、PR 编号或明写「无 PR」、消费者)；范围只限这个目录，不覆盖 agent 自己的会话 scratchpad。写在这里是因为本仓没有 brief 模板文件，`AGENTS.md` 是唯一不依赖派活方记得写的入口。
- 责任人是常驻 orchestra，触发点是任务终态与每次收工报账。
- 到触发点对每个文件逐项二选一：已入库则删，仍要留则写明还等谁消费、留下一个触发点；「回报已发」不单独作删除事件。
- 隔离目录 (如 `undated-archive-<日期>/`) 是临时态，下次报账同样逐项二选一，空目录即删。
