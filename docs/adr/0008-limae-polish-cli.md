# ADR-0008：更名 limae、三个子命令与 polish 的引擎模型

- 日期：2026-09-01
- 承接：ADR-0002 (Rust 主实现方向)、ADR-0005 §四 (确定性段与语义段的两段式边界) 与 §六 (heuristic learning)、ADR-0006 §四 (LLM 语义润色是独立一条线)、ADR-0007 (A / T 家族与词表形制)

## 背景

ADR-0005 §四把 LLM 语义润色确认为未来特性并划了两段式边界，ADR-0006 §四把它从 warning 触发上解开，但两份都只写了「不做什么」：不进 required check、不拿模型当 CI 判定器、不接进 `--fix`。真要动手，还有一整层问题没有答案 —— 这个能力叫什么、怎么进命令行、调谁的模型、探不到引擎时怎么办、模型把标题改坏了谁来拦、prompt 长什么样、先做哪一步。

用户 2026-08-31 至 09-01 与 orchestra 的设计讨论逐条定了案，本 ADR 是这次讨论的正式落库。证据链有两条：

- `docs/research/llm-polish-survey.md` —— 两个 claudish 项目的源码级机制 (Gvozdev 的 hook + fail-open + 可整体替换的 prompt 文件、Deng 的 205 行分层 spec 与「词典不进 prompt、禁止机械换词」)，以及它给出的形态建议与不建议做的事。
- **本机实测**，2026-09-01 在开发机上直接跑三家 CLI 得到的事实，下文逐条标注。实测是这一批决定的主要依据 —— 用户的原话是「实践是检验真理的唯一标准」。

本 ADR 只做决定。**不动代码、不动 `spec/`、不动 `pyproject.toml`、不改任何名字的实际引用**；更名与三个子命令的落地都是后续任务。

## 决定

### 一、更名 limae

工具名、binary、包名、crate 名统一为 **`limae`**。取自贺拉斯 (Horace) 的 *limae labor*「锉刀之功」 —— 反复打磨文字，正是这三个子命令的共同隐喻。

**占用核实** (2026-09-01)：crates.io 明确回 `crate limae does not exist`，PyPI 与 npm 均 404，GitHub 仓名 `Luolc/limae` 可用；GitHub 用户名 `limae` 已被占用，这只影响将来是否能起同名组织，不影响仓名与包名。

**避让**：与重点参考对象 AutoCorrect、zhlint 没有字面或语义撞车；名字里不含 `lint`、`correct`、`md`，不把天花板钉死在「Markdown 的 linter」上 —— `polish` 一旦成立，这个工具就不只是 linter 了。

**迁移范围** (本 ADR 只列清单，不执行)：

| 面 | 今天 | 更名后 |
| --- | --- | --- |
| 仓名 | `lo-md-lint` | `limae` |
| Python 包 | `lo_md_lint` | `limae` |
| 命令名 | `lo-md-lint` | `limae` |
| pre-commit hook id | `lo-md-lint` | `limae` |
| 配置表 / 文件 | `[tool.lo-md-lint]`、`lo-md-lint.toml` | `[tool.limae]`、`limae.toml` |
| 行内指令前缀 | `lo-md-lint-disable` 等 | `limae-disable` 等 |
| 忽略文件 | `.lo-md-lint-ignore` | `.limae-ignore` |

**过渡期保留旧名别名**：命令、配置表与文件名、hook id、行内指令前缀、忽略文件都要在一段时间内新旧两可，消费仓 (`machine-setup` / `wealth-management` / `butler`) 各出一个小 PR 跟进。旧别名什么时候移除由后续任务定，本 ADR 只要求「有过渡期」，不定期限。

### 二、三个子命令，对标 ruff

| 子命令 | 做什么 | 谁在改文本 |
| --- | --- | --- |
| `limae check` | 只报违规，不改 | 无 |
| `limae format` | 确定性排版修复 (今天的 `--fix`) | 规则 |
| `limae polish` | LLM 语义改写 | 模型 |

三档正好是「只报 / 机械改 / 模型改」，风险递增，用户一眼分得清谁动了自己的文字。这也是两段式边界 (ADR-0005 §四) 在命令行上的形状：`check` 与 `format` 是确定性段，`polish` 是语义段，两段各占一个子命令、不互相触发。

今天的 `lo-md-lint [--fix]` 在过渡期继续可用，与 §一 的旧名别名同一批退场。

### 三、polish 的引擎模型：预设即命令模板

配置形状如下 (键名可在落地时微调，语义不可改)：

```toml
[polish]
engine = "auto"     # auto | claude | codex | grok | custom
model  = ""         # 留空 = 预设自带的档
command = []        # engine = "custom" 时的完整命令，支持占位符
```

`claude` / `codex` / `grok` 三个预设各是一份内置的**命令模板展开** —— 不自建 HTTP 请求、不自己解析各家凭证，只调本机已登录的 CLI。这一条是从调研里学来的：Gvozdev 的 `providers.sh` 自建四种 provider 的请求，连带把 oauth、Keychain、ambient key 的整个攻击面搬进了工具 (`llm-polish-survey.md` §5.2 已判断本仓不该重复它)。

三份模板把本机实测踩到的坑固化进去 (实测日期 2026-09-01)：

| 预设 | 展开 | 实测要点 |
| --- | --- | --- |
| `claude` | `claude -p --system-prompt-file <spec> --model <model>`，正文走 stdin | `--system-prompt` 能整体替换内置人格 (换完再问身份，答的是「Claude Agent SDK」)；另有 `--append-system-prompt` 与 `--exclude-dynamic-system-prompt-sections` 两个更细的口子 |
| `codex` | `codex exec --skip-git-repo-check --ephemeral -c model=<model> -c model_reasoning_effort=<档> --output-last-message <tmp>`，spec **前置进正文** | **没有 system 通道**，spec 只能拼进 user 正文 (Gvozdev 在 `providers.sh:280` 也是这么处理的)；`gpt-5.6-luna` 配 `model_reasoning_effort=minimal` 会被服务端 400 拒 (`"param": "reasoning.effort"`)，`low` 可用 |
| `grok` | `grok --system-prompt-override <spec> -m <model> -p <正文>` | `--system-prompt-override` (别名 `--system-prompt`) 与 `--rules` 都能生效，**但只在 4.6 上**；`grok-4.5` 会无视覆盖、继续用自带的 agent 人格并把仓库上下文也带上 |

`custom` 是逃生口：自建推理服务、公司网关、自写脚本都不必等本项目支持，给一条完整命令加占位符即可。

**`auto` 的判定顺序**如下 (用户 2026-09-01 两次修正后的最终版)：

1. `LIMAE_ENGINE` 显式指定 → 直接用，不再探。
2. **宿主自标环境变量** → 正跑在谁的会话里就优先探谁。本机实测 (让各 CLI 自己把 `env` 打出来)：Claude Code 是 `CLAUDECODE=1`，Codex 是 `CODEX_SESSION_ID`，Grok 是 `GROK_SESSION_ID`。**只认这类必定存在的变量**：`CODEX_CI`、`CODEX_MANAGED_BY_NPM` 随安装方式与运行环境变化，不可依赖。
3. `command -v` → **唯一的硬否定**：二进制不存在就出局。
4. 凭证线索**只用于排序，不判死**。看的是这些位置的存在性：`~/.claude.json` 的 `oauthAccount`、`~/.codex/auth.json`、`~/.grok/auth.json`，以及各家的 key 与 base URL 变量 —— claude 侧 `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_BASE_URL` 及 Bedrock、Foundry 系列；codex 侧 `OPENAI_API_KEY` / `CODEX_API_KEY` / `CODEX_ACCESS_TOKEN` / `CODEX_URL`；grok 侧 `GROK_CODE_XAI_API_KEY` / `GROK_CLI_CHAT_PROXY_BASE_URL` / `GROK_AUTH_PROVIDER_COMMAND`。**文件或变量不存在不等于没登录** —— 用户可能用 API key 加自定义 base URL，甚至由一条外部命令现取凭证，所以这一层只排序。
5. 依次探活，第一个通过的用；结果带 TTL 缓存，不每次都探。
6. 全失败 → **逐引擎诊断报错**，区分四种情形：未安装、有凭证但失效 (401 / 403)、未发现任何凭证、网络不可达；每种都给下一步 (去登录、`--engine` 指定、配 `custom`)。

**安全约束**：以上一律只判存在性。凭证的值绝不进日志、错误信息或诊断输出 —— 跨仓守则的红线在这里没有例外，本仓又是 public 仓。

### 四、PONG 测试：新增引擎或换型号的验收

加一个预设、或把某个预设的默认型号换掉，都要先过一条最小验收：用一条**只可能来自我们 spec 的指令** (例如「只回一个词 PONG」) 去问，看这个型号压不压得住自己的内置人格与项目上下文。**不通过的型号不进预设表。**

这条是 grok-4.5 那次踩出来的 (§三 实测)：同一份覆盖参数，4.5 两种写法都无视、继续用自带人格，4.6 两种都听。测法便宜、可复现、与具体 prompt 内容无关，所以写进本 ADR 当常设验收，而不是留在某次调试记录里。

### 五、模型默认不冻结，由实测决定

本 ADR **不定死默认型号**，只记候选、证据与判据。

**本机可用性实测** (2026-09-01)：Codex 侧 `gpt-5.6-sol`、`gpt-5.6-terra`、`gpt-5.6-luna`、`gpt-5.5`、`gpt-5.4`、`gpt-5.4-mini` 可用；`gpt-5.6`、`gpt-4.1`、`gpt-5.2` 在当前 ChatGPT 账号下被拒 (HTTP 400)。Grok 侧只有 `grok-4.6` (默认) 与 `grok-4.5`，没有 4.1。

**社区证据** (research agent 2026-09-01 回报，只回报未入库；以下是**社区评价，非本仓实测**)：便宜档在散文质量上普遍掉档 —— Luna 在两个盲评里明显低于 Sol 与 Terra；Haiku 4.5 被社区口径列为「会被人读的成稿别用」；grok-4.5 在写作榜垫底，且有多处人格回潮的观察 (与 §三 的 PONG 实测同向)；`gpt-5.4-mini` 找不到任何写作专项评测。另外两条对设计直接有用：coding 特化模型的语言劣势主要是**对人偏好偏冷偏短**，不是爱列 bullet；OpenAI 称 5.6 默认已经更短、套话更少，**prompt 里再强调「简洁」会过头** —— 这一条要带进 §九 的 spec 迭代。

**暂定默认** (标明待 A/B 替换)：codex 用 `gpt-5.6-terra`，claude 用 `sonnet`，grok 用 `grok-4.6`。型号一律可配。

**判据与测法**：用本仓与消费仓的真实中文段落做 10–20 条盲对照，人评哪个更像人话；采集手段就是 §十 P0 那个 hook 的 A/B 模式。**降到便宜档必须有自家 A/B 证据，不能只凭公开评测或成本。**

### 六、失败语义分场景

- **CLI (`limae polish <file>`)**：失败就报错、非零退出码，明确说哪个文件没改成。文件级操作静默跳过等于骗人。
- **hook (实时改写一次输出)**：失败就用原文、不打断用户，即 Gvozdev 的 fail-open (`llm-polish-survey.md` §1.5)。热路径上挡住用户比改得好更糟。
- 两者都不得静默吞掉「引擎探测失败」 —— 该报的诊断按 §三 第 6 条报。
- `polish` **永不进 CI 的 required check** (ADR-0005 §四、ADR-0006 §四已定，本 ADR 不改)。

### 七、产物原地改，不做旁路目录

**默认原地改文件**，另给两个 flag：`--diff` 只打印不写，`--check` 有改动则非零退出。

这一条修正了 ADR-0005 §四「产物是旁路文件或建议」的形态判断 (边界本身不变：仍然人审后才算数、仍然不进 required check)。理由是仓库本来就在 git 里，`git diff` 就是天然的人审面；再造一个旁路目录，就得再管 `.gitignore`、再定义一个「接受」动作、再解释旁路文件与工作区谁是正本，全是白造的概念。

### 八、正确性怎么守：结构确定性加语义模型裁判

两道闸，性质不同。

**结构层，确定性比对，不花钱。** 改写前后**逐字不变**的至少要包括：围栏代码块、行内代码、链接目标与锚点、标题行、表格结构。任何一项变了，这次改写就判不合格 (按 §六 分场景报出或拒绝写入)。这挡住的是具体事故：**模型改了标题，别的文件对它的引用当场失效。** 同时 prompt 里默认就写明不要碰标题行 —— 标题是跨文件契约，散文才是润色对象。

**语义层，模型裁判 smoke test，永不进 CI。** 把原文、改写与 spec 一并给裁判模型，只问二元问题：事实有无增删？是不是变成了对输入的「回应」而不是「改写」(Deng 的 spec 第 2 条就在防这个)？**跑第二遍还有没有实质改动？**

最后一问是「不动点」的可用替代。`format` 的契约里有不动点 (跑第二遍逐字相等)，但润色在采样温度下逐字相等不可行 (用户 2026-09-01 指出)，所以改问「有没有实质漂移」 —— 一份已经润好的文档再润一次还大改，说明 spec 或型号有问题。

**黄金 fixture 用来测保护器，不用来锁模型散文**；也不为了迁就润色去放宽 R 家族「不增删行」的契约 (`spec/rules.md`「处理单位」)。

### 九、prompt 的三层结构

一份 **general spec (英文)**，加每种语言各一份 **distilled spec，用该语言自己写**。中文那份先从 Deng 的 `specs/claudish-to-english.md` 翻译起步，再自行 evolve，不预先定死内容。

迭代方式是 alpha-evolve 式的：先小、跑起来、看结果、再改。两个参考对象的哲学差异见 `llm-polish-survey.md` §1.2 与 §2.2 —— Gvozdev 是一句话 prompt 加一份可整体替换的文件，语言块追加在最后；Deng 是 205 行的分层 spec，且**词典不进 prompt、明确禁止机械换词**。本项目取 Deng 的分层与「禁止机械换词」，取 Gvozdev 的「spec 是一份可替换的文件」。

词表的分工照 ADR-0007 不变：`spec/wordlists/` 是 T / A 家族的可执行判定数据，**不因为有了 polish 就塞进 prompt**。

### 十、落地顺序

1. **P0：hook 里的一次输出润色** —— 局部文本、不写盘、没有跨文件问题，正好用来 evolve prompt。**A/B 模式挂在这里**：按概率同时给出改前与改后，人评之后收集反馈；其它仓可以挂同样的 hook，经 herdr 把反馈发回本仓。§五 的型号判据靠它采集。
2. **P1：`limae polish <file>`** —— 带 §八 的结构核对。
3. **P1：按 git 变更集批量** —— 只润色当前 dirty 或最近一个 commit 碰过的 Markdown，概念同 pre-commit 的 staged files。
4. **P2：真正的 multi-file 协调改写** —— 跨文件改名与引用同步。

## 后果

- **更名是一次跨仓迁移**，范围就是 §一 那张表加消费仓的三个小 PR；过渡期内新旧名并存，旧名的移除另起任务。本 ADR 不改任何一处实际引用，仓名、包名、命令名、配置键在本次提交后仍是 `lo-md-lint` / `lo_md_lint`。
- **`README.md`「定位与愿景」的名字与子命令描述在更名落地时再改**，本 ADR 不动它 —— 现在改会让文档描述一个还不存在的命令。
- `spec/rules.md` 本次不变。将来 `--fix` 改叫 `format` 时，规范里的「修复」措辞与 `--fix` 字样要一并过一遍，那是落地任务的事。
- ADR-0005 §四的形态判断 (产物是旁路文件) 被 §七 修正为原地改；§四的其余边界 (两段式、人审、不进 required check、不拿模型当 CI 判定器) 全部保留，0005 不就地改写。
- **tracker 条目由 orchestra 在合入后记账**，至少包括：更名迁移与过渡期别名、三个子命令的实现、P0 的 hook 与 A/B 采集、结构不变量保护器、模型默认的 A/B 决定。本 ADR 不改 `docs/tracker.md`。
- polish 相关的一切都不进 CI 的 required check，现有消费方零行为变化。
- 型号默认与 prompt 内容都是**可调项**，按 §四 的 PONG 与 §五 的 A/B 判据随时改，不需要新的 ADR；需要另起 ADR 的是形制变化 —— 子命令的划分、引擎模型的语义、`auto` 的判定顺序、结构不变量清单、失败语义。

## 状态

proposed (2026-09-01)。
