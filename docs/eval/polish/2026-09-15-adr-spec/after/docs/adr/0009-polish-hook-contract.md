# ADR-0009：polish 的 hook 合同 —— MessageDisplay、批缓存、A/B 采集与 Stop 回注

- 日期：2026-09-01
- 承接：ADR-0008 §十 P0 (「hook 里的一次输出润色」)、§三 (引擎即命令模板)、§五 (模型默认由 A/B 决定)、§六 (失败语义分场景)、§七 (不做旁路目录)

## 背景

ADR-0008 §十把落地顺序定为 P0 → P2 四步。P0 写的是「hook 里的一次输出润色 —— 局部文本、不写盘、没有跨文件问题，正好用来 evolve prompt。A/B 模式挂在这里」，但只说明了这一点：挂哪个 hook、按什么粒度调模型、A/B 如何呈现、编号怎么起、结果怎么回到会话、失败怎么处理、先在哪试，都还没有答案。

用户 2026-09-01 当面确定了以下八条决定。本 ADR 正式记录这次决定，供实现任务 (下一个任务) 据此实现。证据链：

- `docs/research/llm-polish-survey.md` §1.4 (Gvozdev 的 `MessageDisplay` hook 按流式 chunk 缓存、`.final` 才调模型) 与 §1.5 (fail-open)。
- **本机实测**：Claude Code 二进制里的第一方 schema 描述串。这些串**不是公开文档**，而是本机该版本二进制内嵌的 schema 描述文本，会随版本变化；本 ADR 引用的每一句都已在合入前用 `python3` 按字节在二进制里复核过，版本号见下节。**本 ADR 与调用方仓库的关系**：`limae` 的 P0 hook 消费的是 Claude Code 这个宿主程序对外暴露的 hook 事件；这与 ADR-0008 §三「预设引擎即命令模板」调用的三家外部 CLI 是两回事 —— 这里测量的是宿主，那里测量的是被调用方。

本 ADR 只做决定。**不动代码、不动 `spec/`**；hook 脚本、prompt 文件、词表、状态目录格式都留给下一个任务实现。

## 决定

### 一、挂 `MessageDisplay`，改写只上屏

P0 挂 Claude Code 的 `MessageDisplay` hook 事件，不挂其它事件，也不做旁路文件。

**依据是本机 Claude Code 二进制 (`2.1.252`，路径 `/home/ubuntu/.local/lib/node_modules/@anthropic-ai/claude-code/node_modules/@anthropic-ai/claude-code-linux-x64/claude`，与 `docs/research/polish-engine-cli-behavior.md` 记录的实测版本一致) 内嵌的 schema 描述串**，2026-09-01 按字节复核：

- input 侧 (`MessageDisplayHookInput` 一类的描述)：

  > Hook input for the MessageDisplay event. Fired with each batch of newly completed lines while an assistant message streams. Display-only: the stored message and what the model sees are untouched.

- output 侧 (`MessageDisplay` 的 hook-specific output)：

  > Hook-specific output for the MessageDisplay event. Display-only: replaces the delta on screen without changing the stored message.

**推论 (本 ADR 的支点)**：被润色的文本永远不会写回 transcript，也不会进入下一轮模型上下文 —— 两句原文都明确写着「the stored message and what the model sees are untouched」「without changing the stored message」。所以：

- **agent 自己看不见改写后的版本**：它当轮说了什么，屏幕上可能呈现润色后的文字，但它对此一无所知，下一轮也不会因读到润色稿而改变行为。
- **agent 更看不见 A/B 的两个候选** —— A/B 只是同一次 `displayContent` 替换中排出的多栏文本，模型侧没有机制区分「这是 A 还是 B」。

这条推论解释了下面第三、四、五条决定：模型既看不到 A/B，也看不到自己被改写的样子；若要让模型知道「刚才那轮做了 A/B、哪个赢了」，就必须走另一条通道 (§五)。人是唯一看得见改写结果的一方，A/B 的呈现与编号也应为人的判断和反馈优化 (§三、§四)。

### 二、按批缓存，末批整段改写

`MessageDisplay` 是「随流式消息每完成一批新行就触发一次」("Fired with each batch of newly completed lines while an assistant message streams")，不是整条消息只触发一次。每批各调一次模型，会把一段话切成互不相干的碎片，既破坏语感，也浪费调用次数。

照 Gvozdev 的做法 (`llm-polish-survey.md` §1.4，`rewrite.sh:189`、`:218`)：hook 按 `message_id` 缓存每批 delta，只在收到标记消息结束的那一批 (对应 Gvozdev 的 `.final == true`) 时，把缓存的全部内容拼成整段，调一次模型改写，再用输出替换最后这一批的 `displayContent`。中间各批按原文放行，不单独改写，也不单独调模型。

**短消息跳过**：低于一个字符数阈值的消息不润色，直接放行原文。起步阈值采用 Gvozdev 的 `CLAUDISH_MIN_CHARS` 默认值 200 (剥掉围栏代码块后按非空白字符计)，键名与最终数值留给实现任务，**注明可调**。

### 三、A/B 是双模型对照，不是「改前 / 改后」二选一

呈现形式：屏幕上同时显示**原文、候选 A、候选 B** 三栏，不是「润色前 vs 润色后」的二选一对照。

候选池 (7 个)：`gpt-5.6-luna`、`gpt-5.6-terra`、`gpt-5.4`、`grok-4.5`、`grok-4.6`、Claude Haiku、Claude Sonnet。

**只在采样命中的那一轮双跑**，起步采样率约 1/10 (**注明可调，最终值不由本 ADR 定死**)；未命中的轮次单跑 (跑哪一个引擎/型号是实现细节，不在本 ADR 决定范围)。

**用途**：这不是让用户挑一个更好看的回复，而是为 ADR-0008 §五「模型默认不冻结，由实测决定」采集判据 —— §五写明「判据与测法：用本仓与消费仓的真实中文段落做 10–20 条盲对照，人评哪个更像人话；采集手段就是 §十 P0 那个 hook 的 A/B 模式」，本条落实了这句话。

### 四、每轮 A/B 挂一个中文双字名词编号

每次命中采样、触发 A/B 的那一轮，都配一个编号，形如 `[A/B 灯塔]`，取自一份固定词表 (双字中文名词)，**同一会话内不重复**。

**理由**：用户用语音给反馈 (「灯塔那轮 B 更好」)。编号要满足两个条件：人能清楚念出来，语音转写能准确还原。十六进制 UUID、序号加时间戳一类的编号，语音既念不顺，转写也大概率错行 (同音字、断词错位，跟 `~/.agents/AGENTS.md`「与用户沟通」里提到的语音转写风险属于同一类问题)。中文双字名词同时满足这两条约束：词表可控、发音清楚、常见词的转写准确率高。

词表具体选哪些词、多少个，本 ADR 不定，留给实现任务起步后按需扩充。

### 五、编号与型号经 `Stop` 的 `additionalContext` 回注会话上下文

只在触发 A/B 的那一轮 (即第四条编了号的那一轮)，把编号与 A、B 各自使用的型号名，通过 `Stop` hook 的 `additionalContext` 注回会话。未触发 A/B 的轮次不回注。

**依据同样是本机二进制的 schema 描述串**，2026-09-01 复核：

> Hook-specific output for the Stop event. additionalContext is non-error feedback delivered to the model; the conversation continues so the model can act on it.

这句与第一条 `MessageDisplay` 的「untouched」在**同一个二进制**里被明确区分，两句构成有意的对照：`MessageDisplay` 的输出「不改变已存消息、模型看不到」，`Stop` 的 `additionalContext` 则「delivered to the model」「conversation continues so the model can act on it」。第五条决定正是基于这个对照：用 `Stop` 补上 `MessageDisplay` 天生留下的空缺。模型看不见 A/B 长什么样，却能从 `Stop` 注回的编号与型号名知道「刚才那轮发生过 A/B，编号是灯塔，A 是某型号、B 是某型号」，从而在用户后续按编号给反馈时接得上话。

**A/B 台账 (编号、型号、原文、两个候选、时间) 放在会话态目录，不进本仓库**。这与 ADR-0008 §七「不做旁路目录」不冲突：§七说的是**文档润色的产物** (`limae polish <file>` 改出来的文本)，这类产物默认原地改文件、走 `git diff` 人审，明确不设旁路目录；A/B 台账是**运行时的调试 / 判据采集数据**，性质不同，与 §七 无关。台账目录的具体位置与格式留给实现任务决定，本 ADR 只定「会话态、不入库」这条边界。

### 六、失败 fail-open

hook 挂了 (进程崩、探测失败、引擎超时等) 就按原样显示原文，不打断用户，也不报错卡住交互。

二进制本身就是这样处理 `MessageDisplay` 失败的 (2026-09-01 复核到的字符串)：

> MessageDisplay hook failed for completed message; emitting original text

本条不是新决定，而是沿用宿主已有的行为，并确认这与 ADR-0008 §六「hook (实时改写一次输出)：失败就用原文、不打断用户，即 Gvozdev 的 fail-open」是同一语义 —— 两边独立得出同一个答案。

### 七、先在本仓自试，再交 machine-setup

**先挂本仓 `.claude/settings.local.json`**：这个文件按 Claude Code 的约定不进版本控制 (gitignored)，只影响在本仓工作的这个会话，不影响其它仓或其它 agent。

**稳定后再打包交 `machine-setup` 仓，进入用户级配置**，跨仓分发给 `wealth-management`、`butler` 等其它仓使用 (与 `~/.agents/AGENTS.md`「家目录层的 agent 资产不直改」一致：用户级配置经 `machine-setup` 的 PR 与 `--apply-agent-rules` 装出，不由 `limae` 直接写)。

**理由**：失败时，用户可见的回复可能被改坏、卡住，或被不想要的改写污染。先在本仓控制这个风险面，代价是只影响 `limae` 自己的开发会话；等行为验证稳定，再扩大到影响所有仓全部会话的全局配置，代价才值得承担。

### 八、隐私边界继承 PR #37

hook 调引擎的隐私约束与 ADR-0008 §三 + PR #37 落地的 `src/limae/engines.py` 一致，不额外放宽，也不额外收紧 —— 一次性临时目录、白名单环境变量的具体清单已经写在 `docs/research/polish-engine-cli-behavior.md` §一，本 ADR 不重复，只链接过去。

hook 侧新增的一条约束是：送进引擎的**只有待润色的那一段文本** (拼好的整条助手消息)，不带仓库上下文，也不带其它待处理内容 —— 这是 §一 的临时目录/白名单边界在 hook 这一层的体现。

**A/B 台账为这条边界增加了新的风险面**：台账会留存助手回复原文 (可能涉及用户当时讨论的仓库内容)，比引擎调用本身多存一份数据。因此第五条已经定了台账**只放会话态目录，不入库、不跨仓传递** —— 这条边界比「引擎只看见喂给它的文本」更严，因为台账连本地磁盘上的留存也要限制在会话范围内，不能变成另一处需要单独治理隐私的持久化存储。

## 后果

- **本 ADR 只定合同，不定实现细节**：批缓存的键名、短消息阈值的最终数值、A/B 采样率的最终值、中文双字词表的具体内容与规模、会话态台账目录的路径与文件格式，均留给实现任务，且都标为「可调 / 待定」。
- **默认型号不由本 ADR 决定**：候选池是 ADR-0008 §五 已经定的 7 个，最终各引擎的默认型号由 A/B 采集的数据按 §五 判据决定，不需要另起 ADR 改型号本身；需要新 ADR 的是形制变化 (沿用 ADR-0008「后果」一节定的口径：候选池构成、hook 挂载的事件、回注机制本身如需更换，才需要新 ADR)。
- **P0 完成后，模型侧仍然看不见自己被润色过的文字**：第一条的推论是本 ADR 的基础，也是长期存在的限制 —— 除非将来改挂别的 hook 事件或改变 Claude Code 自身行为 (不在本仓控制范围内)，agent 永远只能通过 `Stop` 回注的编号与型号名间接知道「刚才发生过 A/B」，看不到实际改写内容。
- **本仓自试阶段**，`.claude/settings.local.json` 里的 hook 配置只影响本仓会话；跨仓推广是 `machine-setup` 仓的后续任务，不在本 ADR 范围内，也不由本 ADR 排期。
- **A/B 台账的会话态存储**引入了一类新的运行时状态，不受 `spec/` 或 `docs/` 的版本控制覆盖；其保留期限、清理策略留给实现任务决定。本 ADR 只要求它不进本仓库、不跨仓传递。
- tracker 条目由 orchestra 在合入后记账，至少包括：P0 hook 实现 (挂载、批缓存、fail-open)、A/B 采集与中文编号词表、`Stop` 回注、本仓 `.claude/settings.local.json` 自试、machine-setup 打包跟进。本 ADR 不改 `docs/tracker.md`。

## 状态

accepted (2026-09-01)。

§八 划定的会话态存储范围由 ADR-0012 扩展 (2026-09-01)：从「只有采样命中的 A/B 台账」扩到「每一次成功的单路润色」，边界 (会话态、不入库、不跨 agent、随会话过期) 逐条不变。

§五 (回注内容：编号**与型号名**) 由 ADR-0011 取代 (2026-09-01)：回注只带编号，型号对应只留在会话态台账。取代原因是本 ADR §一 的一处推论被实测推翻 —— `Stop` 的 `additionalContext` 并非只通向模型，同时也渲染在用户屏幕上。回注机制本身、只在命中 A/B 的那一轮回注、台账放在会话态，均不受影响。

§二、§三、§四、§五、§八 由 [ADR-0016](0016-hook-mechanical-only.md) 取代 (2026-09-11)：hook 不再调模型，每批就地跑确定性修复，跨批靠前缀重放 (存原文、整篇重修、只取这一批的行)，A/B、编号、`Stop` 回注与台账全部停止。§一 (挂 `MessageDisplay`、display-only)、§六 (fail-open)、§七 (先本仓自试再交 machine-setup) 的内容由 ADR-0016 §一 接过并成为正本，本文不再是 hook 合同的正本。
