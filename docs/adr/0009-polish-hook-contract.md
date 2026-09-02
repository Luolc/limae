# ADR-0009：polish 的 hook 合同 —— MessageDisplay、批缓存、A/B 采集与 Stop 回注

- 日期：2026-09-01
- 承接：ADR-0008 §十 P0 (「hook 里的一次输出润色」)、§三 (引擎即命令模板)、§五 (模型默认由 A/B 决定)、§六 (失败语义分场景)、§七 (不做旁路目录)

## 背景

ADR-0008 §十把落地顺序定成 P0 → P2 四步，P0 写的是「hook 里的一次输出润色 —— 局部文本、不写盘、没有跨文件问题，正好用来 evolve prompt。A/B 模式挂在这里」，但只给了这一句话：挂在哪个 hook 上、按什么粒度调模型、A/B 怎么呈现、编号怎么起、结果怎么喂回会话、失败了怎么办、先在哪试，都还没有答案。

用户 2026-09-01 当面拍板定了以下八条决定，本 ADR 是这次拍板的正式落库，供实现任务 (下一个任务) 照此实现。证据链：

- `docs/research/llm-polish-survey.md` §1.4 (Gvozdev 的 `MessageDisplay` hook 按流式 chunk 缓存、`.final` 才调模型) 与 §1.5 (fail-open)。
- **本机实测**：Claude Code 二进制里的第一方 schema 描述串。这些串**不是公开文档**，是本机该版本二进制内嵌的 schema 描述文本，随版本变化；本 ADR 引用的每一句都已在合入前用 `python3` 按字节在二进制里复核过，版本号见下节。**关于本 ADR 与调用方仓库的关系**：`limae` 的 P0 hook 消费的是 Claude Code 这个宿主程序对外暴露的 hook 事件，与 ADR-0008 §三「预设引擎即命令模板」调用的三家外部 CLI 是两回事 —— 这里量的是宿主，那里量的是被调用方。

本 ADR 只做决定。**不动代码、不动 `spec/`**；hook 脚本、prompt 文件、词表、状态目录格式的实现都是下一个任务的事。

## 决定

### 一、挂 `MessageDisplay`，改写只上屏

P0 挂 Claude Code 的 `MessageDisplay` hook 事件，不挂其它事件、不做旁路文件。

**依据是本机 Claude Code 二进制 (`2.1.252`，路径 `/home/ubuntu/.local/lib/node_modules/@anthropic-ai/claude-code/node_modules/@anthropic-ai/claude-code-linux-x64/claude`，与 `docs/research/polish-engine-cli-behavior.md` 记录的实测版本一致) 内嵌的 schema 描述串**，2026-09-01 按字节复核：

- input 侧 (`MessageDisplayHookInput` 一类的描述)：

  > Hook input for the MessageDisplay event. Fired with each batch of newly completed lines while an assistant message streams. Display-only: the stored message and what the model sees are untouched.

- output 侧 (`MessageDisplay` 的 hook-specific output)：

  > Hook-specific output for the MessageDisplay event. Display-only: replaces the delta on screen without changing the stored message.

**推论 (本 ADR 的支点)**：被润色的文本永远不会写回 transcript、也不会进下一轮模型的上下文 —— 两句原文都明确写着「the stored message and what the model sees are untouched」「without changing the stored message」。所以：

- **agent 自己看不见被改写后的版本**：它当轮说了什么，屏幕上呈现的可能是润色过的文字，但它对此一无所知，下一轮也不会因为读到润色稿而改变行为。
- **agent 更看不见 A/B 的两个候选** —— A/B 只是同一次 `displayContent` 替换里排版出来的多栏文本，模型侧没有任何机制区分「这是 A 还是 B」。

这条推论是下面第三、四、五条决定存在的原因：既然模型看不到 A/B、也看不到自己被改写的样子，若要让模型知道「刚才那轮做了 A/B、哪个赢了」，就必须走另一条通道 (§五)；既然人是唯一看得见改写结果的一方，A/B 的呈现与编号就要为人的判断与反馈优化 (§三、§四)。

### 二、按批缓存，末批整段改写

`MessageDisplay` 是「随流式消息每完成一批新行就触发一次」("Fired with each batch of newly completed lines while an assistant message streams")，不是整条消息触发一次。逐批各自调一次模型会把一段话切成互不相干的碎片，破坏语感也浪费调用次数。

照 Gvozdev 的做法 (`llm-polish-survey.md` §1.4，`rewrite.sh:189`、`:218`)：hook 按 `message_id` 把每一批 delta 缓存下来，只在收到标记消息结束的那一批 (对应 Gvozdev 的 `.final == true`) 时，把已缓存的全部内容拼成整段，调一次模型改写，输出替换最后这一批的 `displayContent`。中间各批照原文放行，不单独改写、不单独调模型。

**短消息跳过**：低于一个字符数阈值的消息不润色，直接放行原文。起步阈值抄 Gvozdev 的 `CLAUDISH_MIN_CHARS` 默认值 200 (剥掉围栏代码块后按非空白字符计)，键名与最终数值留给实现任务，**注明可调**。

### 三、A/B 是双模型对照，不是「改前 / 改后」二选一

呈现形态：屏幕上同时给**原文、候选 A、候选 B** 三栏，不是「润色前 vs 润色后」的二选一对照。

候选池 (7 个)：`gpt-5.6-luna`、`gpt-5.6-terra`、`gpt-5.4`、`grok-4.5`、`grok-4.6`、Claude Haiku、Claude Sonnet。

**只在采样命中的那一轮双跑**，起步采样率约 1/10 (**注明可调，最终值不由本 ADR 定死**)；未命中的轮次单跑 (跑哪一个引擎/型号是实现细节，不在本 ADR 决定范围)。

**用途**：这不是给用户挑一个更好看的回复，是为 ADR-0008 §五「模型默认不冻结，由实测决定」采集判据 —— §五写明「判据与测法：用本仓与消费仓的真实中文段落做 10–20 条盲对照，人评哪个更像人话；采集手段就是 §十 P0 那个 hook 的 A/B 模式」，本条是那句话的落实。

### 四、每轮 A/B 挂一个中文双字名词编号

每次命中采样、触发 A/B 的那一轮，配一个编号，形如 `[A/B 灯塔]`，取自一份固定词表 (双字中文名词)，**同一会话内不重复**。

**理由**：用户用语音给反馈 (「灯塔那轮 B 更好」)。编号要满足两个条件：人能不含糊地念出来，语音转写能准确还原。十六进制 UUID、序号加时间戳一类的编号，语音既念不顺、转写也大概率错行 (同音字、断词错位，跟 `~/.agents/AGENTS.md`「与用户沟通」里提到的语音转写风险是同一类问题)。中文双字名词是这两条约束的交集：词表可控、发音清楚、常见词转写准确率高。

词表的具体内容 (选哪些词、多少个) 本 ADR 不定，留给实现任务起步、按需扩充。

### 五、编号与型号经 `Stop` 的 `additionalContext` 回注会话上下文

只在触发了 A/B 的那一轮 (即第四条编了号的那一轮)，把编号与 A、B 各自用的型号名，通过 `Stop` hook 的 `additionalContext` 注回会话。未触发 A/B 的轮次不回注。

**依据同样是本机二进制的 schema 描述串**，2026-09-01 复核：

> Hook-specific output for the Stop event. additionalContext is non-error feedback delivered to the model; the conversation continues so the model can act on it.

这句与第一条 `MessageDisplay` 的「untouched」在**同一个二进制**里被明确分开写，两句是有意的对照：`MessageDisplay` 的输出「不改变已存消息、模型看不到」，`Stop` 的 `additionalContext` 则「delivered to the model」「conversation continues so the model can act on it」。第五条决定正是靠这个对照成立的 —— 用 `Stop` 补上 `MessageDisplay` 天生留下的空档：模型自己看不见 A/B 长什么样，但可以从 `Stop` 注回的编号与型号名知道「刚才那轮发生过 A/B，编号是灯塔，A 是某型号、B 是某型号」，从而在用户后续用编号给反馈时接得上话。

**A/B 台账 (编号、型号、原文、两个候选、时间) 落在会话态目录，不进本仓库**。这与 ADR-0008 §七「不做旁路目录」不冲突：§七说的是**文档润色的产物** (`limae polish <file>` 改出来的文本)，那类产物默认原地改文件、走 `git diff` 人审，明确不搞旁路目录；A/B 台账是**运行时的调试 / 判据采集数据**，性质不同，与 §七 无关。台账目录的具体位置与格式留给实现任务定，本 ADR 只定「会话态、不入库」这条边界。

### 六、失败 fail-open

hook 挂了 (进程崩、探测失败、引擎超时等) 就照原样显示原文，不打断用户、不报错卡住交互。

二进制自己就是这样处理 `MessageDisplay` 失败的 (2026-09-01 复核到的字符串)：

> MessageDisplay hook failed for completed message; emitting original text

本条不是新决定，是照抄宿主已有的行为，并确认这与 ADR-0008 §六「hook (实时改写一次输出)：失败就用原文、不打断用户，即 Gvozdev 的 fail-open」是同一语义 —— 两边独立得出同一个答案。

### 七、先在本仓自试，再交 machine-setup

**先挂本仓 `.claude/settings.local.json`**：这个文件按 Claude Code 的约定不进版本控制 (gitignored)、只影响在本仓工作的这个会话，不影响其它仓或其它 agent。

**稳定后再打包交 `machine-setup` 仓，进用户级配置**，跨仓分发给 `wealth-management`、`butler` 等其它仓使用 (与 `~/.agents/AGENTS.md`「家目录层的 agent 资产不直改」一致：用户级配置经 `machine-setup` 的 PR 与 `--apply-agent-rules` 装出，不由 `limae` 直接写)。

**理由**：失败面是用户可见的回复被改坏、卡住，或者被不想要的改写污染。先在本仓兜住这个失败面，代价是只影响 `limae` 自己的开发会话；等行为验证稳定，再放大到全局影响所有仓的所有会话，代价才划算。

### 八、隐私边界继承 PR #37

hook 调引擎的隐私约束与 ADR-0008 §三 + PR #37 落地的 `src/limae/engines.py` 一致，不额外放宽也不额外收紧 —— 一次性临时目录、白名单环境变量的具体清单已经是 `docs/research/polish-engine-cli-behavior.md` §一 的正本，本 ADR 不重复抄一遍，只链过去。

hook 侧新增的一条约束是：送进引擎的**只有待润色的那一段文本** (拼好的整条助手消息)，不带仓库上下文、不带其它待处理内容 —— 这与 §一 的临时目录/白名单边界是同一条隐私红线在 hook 这一层的体现。

**A/B 台账是这条边界的一个新增风险面**：台账里会留存助手回复的原文 (可能涉及用户当时在讨论的仓库内容)，比引擎调用本身多存了一份数据。所以第五条已经定了台账**只落会话态目录，不入库、不跨仓传递** —— 这条边界比「引擎只看见喂给它的文本」更严，因为台账连本地磁盘上的留存都要控制在会话范围内，不能变成又一处需要单独治理隐私的持久化存储。

## 后果

- **本 ADR 只定合同，不定实现细节**：批缓存的键名、短消息阈值的最终数值、A/B 采样率的最终值、中文双字词表的具体内容与规模、会话态台账目录的路径与文件格式，均留给实现任务，且都标了「可调 / 待定」。
- **默认型号不由本 ADR 决定**：候选池是 ADR-0008 §五 已经定的 7 个，最终各引擎的默认型号由 A/B 采集的数据按 §五 判据决定，不需要另起 ADR 去改型号本身；需要新 ADR 的是形制变化 (沿用 ADR-0008「后果」一节定的口径：候选池构成、hook 挂载的事件、回注机制本身如果要换，才需要新 ADR)。
- **P0 完成后，模型侧仍然看不见自己被润色过的文字**：第一条的推论是本 ADR 的地基，也是长期存在的限制 —— 除非将来改挂别的 hook 事件或改变 Claude Code 自身行为 (不在本仓控制范围内)，agent 永远只能通过 `Stop` 回注的编号与型号名间接得知「刚才发生过 A/B」，看不到实际改写内容。
- **本仓自试阶段**，`.claude/settings.local.json` 里的 hook 配置只影响本仓会话；跨仓推广是 `machine-setup` 仓的后续任务，不在本 ADR 范围内，也不由本 ADR 排期。
- **A/B 台账的会话态存储**引入了一类新的运行时状态，不受 `spec/` 或 `docs/` 的版本控制覆盖，其保留期限、清理策略留给实现任务定；本 ADR 只要求它不进本仓库、不跨仓传递。
- tracker 条目由 orchestra 在合入后记账，至少包括：P0 hook 实现 (挂载、批缓存、fail-open)、A/B 采集与中文编号词表、`Stop` 回注、本仓 `.claude/settings.local.json` 自试、machine-setup 打包跟进。本 ADR 不改 `docs/tracker.md`。

## 状态

accepted (2026-09-01)。

§八 划定的会话态存储范围由 ADR-0012 扩展 (2026-09-01)：从「只有采样命中的 A/B 台账」扩到「每一次成功的单路润色」，边界 (会话态、不入库、不跨 agent、随会话过期) 逐条不变。

§五 (回注内容：编号**与型号名**) 由 ADR-0011 取代 (2026-09-01)：回注只带编号，型号对应只留在会话态台账。取代的原因是本 ADR §一 的一处推论被实测推翻 —— `Stop` 的 `additionalContext` 并非只通向模型，它同时渲染在用户屏幕上。回注机制本身、只在命中 A/B 的那一轮回注、台账落会话态，均不受影响。
