# Agent 守则 (本仓专属)

任何 AI agent / 会话在本仓库工作前必须先读本文件。`CLAUDE.md` 是指向本文件的软链接 —— 永远编辑本文件。

## 语言规范

**本仓属于跨仓守则里的情况 A：仓库语言中文。** 面向人读的仓内文字用中文，代码、commit message 与 PR 标题一律用英文，中文行文的标点与空格规则也照搬守则那一节，这里不再重复。本仓增量两条：

- **历史里已经存在的中文 PR subject 一律不动**，只管往后新开的 PR。守则那条记的是往后怎么做，没说存量怎么办，本仓的决定是不回头改 —— 改它要重写已推送的历史，代价远大于收益。
- **README 两份，完整度以中文版为准**：`README.md` (中文) 决定这个项目对外讲到什么程度，`README.en.md` 是给英文发现层的精简版，**不承诺对译**。英文版允许少写章节；**不允许出现中文版没有的内容** —— 那样英文版就成了另一个权威版本，而两个权威版本迟早会走样。判据落在改动的性质上：动到**用户可见的行为** (新增或删除命令、选项、安装方式、平台支持) 时两份都要改；只是把中文表述写得更细、更准，可以只改中文。另有一处容易想当然的接线：`Cargo.toml` 的 `readme` 指向 `README.en.md` (crates.io 上的读者是英文发现层)，而 GitHub 首页仍然是 `README.md` —— **这是两个独立字段，改一个不会动另一个**。

## 隐私边界

本仓是 public 仓库，工具代码、无业务数据：仓内绝不能出现任何凭证或个人身份信息 (PII)。这是硬性红线，不因仓库访问控制而放松 —— 历史里一旦出现就必须先轮换凭证、再清理历史。公开发表作品的作者署名与公开链接属引用 (attribution)，不在 PII 之列。

**测试 fixture 只用合成数据** (如 `ACME` / `$1,000` / `Foo`)：不得出现用户真实密钥、账户或身份信息，即使只是当作待检查的字符串。

**敏感值不进任何文本，只按位置指代**：brief、PR 描述、commit message、评论等一切文本里都不写敏感值本身，只指代其位置 (文件、行号、测试 id 等)；脱敏映射只留在裁决人手里，不经任何 agent 间通道传递。

## 目录约定

布局参考 [ruff](https://github.com/astral-sh/ruff) 仓：每种语言的实现都以仓根为项目根，源码进各自的子目录；规范与 fixture 独立于任何实现。

- **规则规范与黄金 fixture 语言无关、所有实现共用**，放仓根 `spec/`：规范在 `spec/rules.md`，黄金集在 `spec/fixtures/`，AI 中文词典在 `spec/lexicon/zh.toml`，prompt spec 在 `spec/polish/`，规则词表在 `spec/wordlists/`；不放进任何单一实现的私有目录 (`src/`、`tests/`)。各部分的职责与格式见 `spec/README.md`，位置与理由见 `docs/adr/0001-standalone-repo-spec-first-shared-fixtures.md`。
- **Rust 是仓根 Cargo package，也是唯一实现** (Python 参考实现已于 2026-09-10 整体删除，见 ADR-0015 补记)：根 `Cargo.toml`、`Cargo.lock` 与 `rust-toolchain.toml` 配套，源码在 `rust/`，集成测试在 `rust/tests/`；唯一发布的 binary 是 `limae`，开发期工具 `hook-kinds`、`render-lexicon`、`lint-lexicon` 与 `render-skill` 的源码在 `rust/tools/`、以 `[[example]]` target 声明 —— 目录名说它们是什么 (与仓根 `tools/` 同义：凡叫 `tools` 的都是开发期工具、不进发布)，`[[example]]` 是保证它们不随 `cargo install` 分发的机制；两者不必同名，因为 `Cargo.toml` 把每个 `path` 写死，不走 Cargo 对 `examples/` 的自动发现；Rust 的 unit test、integration test 与 doctest 使用 Cargo 内建测试框架，仍对着同一套 `spec/` 与黄金 fixture 跑。toolchain pin 的唯一配置来源是 `rust-toolchain.toml`，不在此重复版本值，详见 ADR-0015 §六。根 `Cargo.toml` / `Cargo.lock` / `rust-toolchain.toml`、`rust/lib.rs` 接线、CI 与共享测试入口由当时的集成任务单独持有；加依赖与接线串行。
- **静态站点在 `site/`**：Cargo example `render-lexicon` (`rust/tools/render_lexicon.rs`，版式在 `rust/templates/lexicon.html`) 从 `spec/lexicon/zh.toml` 生成 `site/index.html`，`cargo run --example render-lexicon` 重新生成；`site/` 只放生成产物，且**不入库** —— 它在 `.gitignore` 里，`.github/workflows/ci.yml` 的 `build` job 每次现渲染，`deploy` job 只在 main 上把它发布到 GitHub Pages (`https://limae.luolc.com/`)。可编辑的页面正文 (标题、副标题、引子、判据与门槛、词条) 在 toml；模板保留结构性标签 (栏目名、白 / 解 / 病 / 原 / 改这类标签、生成说明) 与版式。
- **agent skill 在 `skills/write-naturally/`，是入库的生成产物**：front matter、正文与中文指南手写在 `spec/skill/` (`SKILL.md` 带 `name` / `description` 的 front matter 加语言无关正文、`zh.md` 中文指南)，词典仍是 `spec/lexicon/zh.toml`；Cargo example `render-skill` (`rust/tools/render_skill.rs`) 把三者装配成 `skills/write-naturally/SKILL.md` (front matter 原样带过，生成器只核对它合规、不持有文案) 与 `references/zh/{guide,lexicon}.md`，`cargo run --example render-skill` 重新生成。与 `site/` 相反，它**入库**：skill 是给人 clone 下来直接用的文件，不入库等于没发布；`tools/check_skill_render.sh` 那项检查保证入库的那份与源文件一致。词典进 polish prompt 与进 skill 走同一个渲染函数 (`rust/polish/lexicon.rs`)，两份产物的装配清单不同、源只有一份，任何一处都不手抄词条。按语言分目录，加英语是新建 `references/en/`，不往平铺文件里插。
- **内容类 Markdown 在 `docs/`**：`docs/adr/` (决策记录)、`docs/knowledge/` (操作手册)、`docs/research/` (调研)。
- 项目级 skill 只放在 `.agents/skills/<name>/`，见 `.agents/skills/README.md`。

## 质量标准 (quality bar)

CI (`.github/workflows/ci.yml`，required check 名为 `check`) 在 PR 与 main 上覆盖以下十四项质量检查；本地 push 前按 CI 的检查顺序裸跑：

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo test --locked
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked
tools/check_lexicon_render.sh
tools/check_skill_render.sh
cargo run --quiet --locked --example lint-lexicon -- spec/lexicon/zh.toml
tools/check_lexicon_lint.sh
tools/check_repo_contracts.sh
uvx pre-commit@4.2.0 run --all-files --show-diff-on-failure
tools/check_precommit_consumer.sh
tools/check_rust_package.sh package
tools/check_rust_package.sh target x86_64-unknown-linux-gnu
tools/check_rust_package.sh target x86_64-unknown-linux-musl
```

- 首次 clone 后先 `uvx pre-commit@4.2.0 install`：钩子是本地状态，不随仓库分发，漏装则 commit 无任何拦截。pre-commit 本身由 `uvx` 按钉住的版本现取，仓里没有 Python 项目、锁文件或 `.venv`；`uv` 是 Rust 写的工具管理器，不是 Python 资产，CI 装它只为这一件事。
- **凭证扫描 (gitleaks) 是这里唯一守红线而不是守风格的钩子** (见「隐私边界」)：别的检查上跳过一次只是欠一次格式，这一环上跳过一次就是让凭证有机会进这个 public 仓。`--no-verify` 会跳过全部 hooks，包括 gitleaks；如确需使用，必须在提交前 (文件已暂存后) 手工执行 `gitleaks git --staged --redact --no-banner --verbose .`。机器限频、钩子跑得慢的时候尤其要记得这一步 —— 机制不会替你拦住，这条只能靠人执行。
- 它扫的是**暂存区** (`--staged`)，也就是正要提交的这份内容：扫历史看不见它，而历史里的凭证已经跑掉了，只剩轮换与清史。命中时 `--redact` 只打印规则名与文件行号，不把命中的值打进终端或会话记录，这样验证凭证泄漏时也不会二次泄漏。版本钉在 `.pre-commit-config.yaml` 的 `rev`，pre-commit 用 Go 从源码装：首次约两分钟，之后每次约 2 秒。**升这个 `rev` 时必须重新核对上游 entry 仍带 `--staged`**：entry 会随 tag 变，`--staged` 一旦丢掉，钩子就退化成扫历史 —— 而扫历史看不见刚 `git add` 的 token，绿得像样却什么也没防住，正是这套配置要堵的那个洞。
- CI 的 `Credential scan` 那一步是同一把扫描的另一半：CI 没有暂存区，它改扫已经落进历史的内容 (整份 clone，`fetch-depth: 0`)，堵住漏装钩子、或绕过钩子推上来的分支这两个缺口。它的版本与校验和跟 `.pre-commit-config.yaml` 的 `rev` 一起动，两处必须同版本。
- **检查列表里有两道 pre-commit，方向相反，别把它们看成一件事**：`uvx pre-commit@4.2.0 run --all-files` 那道是**我们对自己** —— 拿本仓的钩子扫本仓的文件。`tools/check_precommit_consumer.sh` 那道是**别人对我们**：它是一次针对 pre-commit 这条分发路径的集成测试，从当前 `HEAD` 克隆出上游副本，在一个全新的空消费仓与隔离的 `PRE_COMMIT_HOME` 里装 `limae` 钩子，跑一遍 install / 违规 / 修复 / 修后 clean 四臂。它吃的是**已提交的 `HEAD`**，所以跑它之前先 commit，跑完核对退出 0 且末行 revision 等于 `HEAD`。分辨力来自 PATH 上那个同名失败探针 (`limae` 退 97)：任何一臂真绿了，跑的就一定是 pre-commit 自己缓存里装出来的那个，而不是机器上碰巧存在的某个 `limae`。详见 [Rust 分发预演](docs/knowledge/rust-binary-distribution.md)。
- **词典的中文正文也归自己的 linter 管**：`lint-lexicon` 那项检查与 `limae-lexicon` 钩子把 `spec/lexicon/*.toml` 里给人读的字段 (标题、引子、判据与门槛、每条词条的解释与例句) 送进同一套规则，报的是真实的 toml 行号；实现是 Cargo example `lint-lexicon` (`rust/tools/lint_lexicon.rs`)。两条判断写在 `.pre-commit-config.yaml` 那条钩子的注释里：**选文件按路径不按扩展名** (`^spec/lexicon/.*\.toml$`，`Cargo.toml` 这类配置一个都不碰，将来的 `en.toml` 自动进来)；**只开排版规则，实验规则全关** —— `examples[].before` 装的就是这部词典收录的病句，开着等于让词典自己撞自己 (这个理由只对 `examples[].before` 成立，对其余散文字段是保守取舍，见该文件模块注释)。
- **`tools/check_lexicon_lint.sh` 守的是 linter 自己，不是词典内容**：`spec/lexicon/zh.toml` 是干净的，所以拿它跑 `lint-lexicon` 那项检查，在行号映射正确、错误、乃至整个删掉时都打印 `OK`。`tools/check_lexicon_lint.sh` 拿行号固定的 fixture 驱动这个工具，逐条断言报出来的 toml 行；它把真词典再跑一遍是**自己的对照臂** (其余各臂都断言「应该报」，一个什么都报的工具能全过)，不是第二道内容检查。**只要检查列表里那条 `cargo test` 不带 `--all-targets`，example 里的 `#[cfg(test)]` 就是死的**：那条命令只编译 example，不跑它们的测试 (2026-09-10 本机两臂实测 —— 一个恒失败的 `#[test]` 在 `cargo test --locked` 下退出 0 且输出里连测试名都不出现，`--all-targets` 下退出 101)。所以这类回归测试写成 `tools/` 下的检查，别写进 example；写在那里的是一条永远不会红的空洞，比不写更坏，因为它看起来像一道检查。**这句话带着一个可以核的前提，别把它当常驻现状**：检查列表里那条一旦带上 `--all-targets`，前提就不成立，**连这一句一起删掉** —— 一句描述已修问题的现状说明，读者会照着它绕路。改的时候注意 `--all-targets` 不含 doctest (`cargo test --help` 原文 `Test all targets (does not include doctests)`)，必须同时保留一条 `--doc`，否则今天那 7 条 doctest 会静默消失，等于用一个新洞换掉旧洞。
- 本仓用自己的 linter 检查自己的 Markdown (dogfooding)。规则一改、文档标红时，先判断是文档错还是规则错：检查器必然存在误报与漏报，判断是检查器错了就直接修它 (commit message 里说明理由)，规则确实错了就改规则与 `spec/` 下的规范和黄金集，不改文档迁就；拿不准的案例交给维护者裁决。
- **`tools/check_repo_contracts.sh` 守的是仓库对维护者自己的承诺，不是 crate 的行为**：`docs/knowledge/polish-hook-self-trial.md` 的 `kind` 表覆盖代码里每一个 `Kind` (全集由 Cargo example `hook-kinds` 打印，它的穷尽 match 让新增变体在编译期就红)；`.pre-commit-config.yaml` 里 `cargo-fmt` 与 `limae-lexicon` 两个钩子的 `files` 各自正好叫醒该叫醒的文件。它们不进 `rust/tests/`，因为那些文件不是 crate 资源、不进 `Cargo.toml` 的 `include`，而一条「文件不在就跳过」的测试在打包检查里永远绿。改其中任何一条契约，验收都要两臂：改坏 → 检查报红，改回 → 检查通过。
- **Rust 侧的质量检查**：`cargo test --locked` 验收 doctest，`cargo doc` 单独验收文档构建。分发检查所需的 Rust 工具链与 musl target 见 [Rust 分发预演](docs/knowledge/rust-binary-distribution.md)；CI 的 target 安装是前提，不算一道检查。commit hook 仍只放秒级检查，完整构建与分发重检查不进 hook：Rust 侧进 hook 的只有 `cargo fmt --check` (2026-09-09 本机实测 0.34 s，含 pre-commit 自身开销约 1.1 s)，`cargo clippy` 要编译全部 target，远超这个预算，留在 push 前全量与 CI —— 判据是「够不够快」，不是「是不是 Rust」。
- **Rust lint 默认**：根 `[lints.rust]` 设 `unsafe_code = "forbid"` 与 `unused_must_use = "deny"`，根 `[lints.clippy]` deny `unwrap_used`、`expect_used`、`dbg_macro`、`print_stdout` 与 `print_stderr`。测试默认返回 `Result`，不开全局 `unwrap` / `expect` 例外；确需窄范围例外时，只包住所需表达式，并写明 lint 名与理由。

## 合并

合并流程见跨仓守则「Git 工作流」的「合并必须固定 LGTM 的 SHA」，这里只记本仓增量两条：required check 叫 `check`，设上之后可以加 `--auto` (绿了自动进 main，红永不合并)；**轮询 PR 状态用 `gh pr checks <N> --required --json name,bucket`，同时监听失败态** (`bucket` 为 `fail` 或 `cancel` 即退出)，不要只等 MERGED —— 只等 MERGED 的循环在 CI 红掉时会一直转到超时。`--required` 只看 required check (`check`)：本仓 `deploy` job 只在 main 上跑，在每个 PR 上的 `bucket` 都是 `skipping`，不属于失败态，不加 `--required` 会把它也轮询进来。

## 发版

**发版必须先有用户本人的同意。** 什么动作算发版、哪两种授权算数、为什么不可逆，都在跨仓守则「多 agent 协作」的发版条款里，这里不重复；本仓增量三条。

- **本仓今天只有一条受支持的自动发版路径**：推 `v<major>.<minor>.<patch>` tag，`.github/workflows/release.yml` 由这个 tag push 触发，此后 GitHub Release 与 crates.io / PyPI / npm 三家的发布全部自动发生 —— **tag 推出去之后没有一处还等着人点头**，所以不必逐个列举 workflow 内部的下游步骤。绕开 workflow 直接敲 `cargo publish` / `twine upload` / `npm publish` / `gh release create` 同样落在守则的判据里面 (crates.io 的 token 仍在用户手上，这条路是通的)。
- **跑 `workflow_dispatch` 的 launcher dry-run 不算发版**，与守则列的 bump 版本号、开 bump PR、合入 PR 同档。
- **条件式预授权说「做完测完」时，本仓怎么核**：「做完测完」是「做完」且「测完」，不是「合入」。本仓合入的前提是 required check `check` 绿，而那就是十四项质量检查，所以合入通常已经蕴含「测完」，不必在 CI 证过的事情上再叠一层人工确认；**但蕴含不是等同** —— 验收臂依赖已发布产物的 feature (安装路径这类，`brew install` 与 `curl … | sh` 都得先有一个真的 Release) 在合入那一刻根本还没跑过，「测完」就没兑现。逐项都成立才可以执行；有一项说不清就回去问用户，不往宽里读。三家的不可撤回窗口见 [发版手册](docs/knowledge/release.md)。

## 多 agent 协作

冲突磁铁、审查方只审不改、完成即回报、herdr 布局与派活验收这些都在跨仓守则里，这里只记本仓增量。

- **本仓的冲突磁铁具体是哪几处**：`AGENTS.md` 与 `.agents/skills/`；依赖锁文件是 `Cargo.lock` (对应仓根 `Cargo.toml`)，同一时刻只允许一个改依赖的任务在跑。
- **tracker 记账本仓有意偏离守则**：backlog 在 `docs/tracker.md`，由实现方在改动所属的 PR 里记账、而不是只由 orchestra 合入后记，理由与冲突处置写在该文件开头，别处不重复。
- **审查规范**：Rust 设计、实现 brief、自审与正式审查均加载用户级 `rust-review` 与 `pr-review`，再叠加本仓适用规则；不复制 skill 正文、不引用其它仓库路径。本仓目前没有额外的仓库专属加严规则。
- **`~/scratch/limae/` 的产物首行写明归属** (任务 slug、PR 编号或明写「无 PR」、消费者)。守则要求这个目录下每个文件名都带日期，本仓在此之上多要一行归属；范围只限这个目录，不覆盖 agent 自己的会话 scratchpad。写在 `AGENTS.md` 是因为本仓没有 brief 模板文件，这里是唯一不依赖派活方记得写的入口。
- **报账触发点与处置**：责任人是常驻 orchestra，触发点是任务终态与每次收工报账；到触发点对每个文件逐项二选一 —— 已入库则删，仍要留则写明还等谁消费、留下一个触发点，「回报已发」不单独作删除事件。隔离目录 (如 `undated-archive-<日期>/`) 是临时态，下次报账同样逐项二选一，空目录即删。
