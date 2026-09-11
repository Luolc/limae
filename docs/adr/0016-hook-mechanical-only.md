# ADR-0016：hook 只做机械排版修复，按批就地修，跨批只带围栏状态

- 日期：2026-09-11
- 取代：ADR-0009 §二–§五、§八 (按批缓存末批整段改写、A/B 对照、编号、`Stop` 回注、引擎隐私边界)；ADR-0011 全部 (A/B 回注只带编号)；ADR-0012 全部 (单路润色的会话态记录)；ADR-0014 全部 (Codex 的 `Stop` hook)；ADR-0008 §十 P0 (hook 里的一次输出润色与 A/B 采集)。处置细节见 §八。
- 承接：ADR-0009 §一 (挂 `MessageDisplay`，改写只上屏)、§六 (失败 fail-open)、§七 (先在本仓自试，再交 machine-setup) —— 三节的内容由本 ADR 接过并成为正本；ADR-0005 §四 (确定性段与语义段的两段式边界)；ADR-0008 §二 (三个子命令) 与 §六 (hook 失败就用原文)。

## 背景

ADR-0009 把 hook 定成「按批缓存、末批整段送模型改写、按概率双跑 A/B 盲评」，ADR-0012 又让每一次单路润色落一份会话态记录，ADR-0014 把同一套东西搬到 Codex 的 `Stop` 上。这套设计在本仓自试了十天 (2026-09-01 起)，用户 2026-09-11 决定把它缩掉。六条决定是本 ADR 的输入，不是本 ADR 可以讨论的东西：

1. **polish 这个 feature 留着**：`rust/polish/` 一行不删，`limae polish` 子命令一行不动。改的只有「hook 不再调 polish」。
2. **hook 只保留纯机械的排版修复**，即 `limae --fix` 那一套，不再调模型重写。
3. **「润色版 vs 原版」的对比整个去掉**：A/B 盲评连同它的证据用途一起停。
4. **Codex 那侧的 hook 放弃**。理由是实测出来的 (ADR-0014「实测」一节)：Codex 没有等价于 `MessageDisplay` 的替换事件，hook 的输出只能作为 `systemMessage` 追加在回复**下方**，既不渲染 Markdown，也替换不掉原文 —— 一个「修排版」的功能在那个位置没有意义。
5. **polish 有两条并列的消费路径、一个正本**：它仍是 CLI 里的功能，想跑的人照样跑，可以当 headless tool 用；同时另外提供一份**生成出来的 skill**，供人直接装进自己的 agent，让模型在**写的时候**就守规则 —— 对 coding agent 来说 skill 是更 native 的用法。两条路径共用 `spec/polish/` 这一个正本。skill 的生成往后排 (用户要先把 polish 的 prompt 再测一测)，本 ADR 只留指针 (§七)。
6. **配置沿用现状，用户级配置本期不做**：hook 读的规则配置仍是今天 `find_config` 从 `cwd` 起找到的仓级配置，没有就是默认集。用户先前提过的用户级 `~/.config/limae/limae.toml` (仓级覆盖用户级、一个「不读仓级」的开关、存在就读永不创建) 已被用户自己收回 —— 包括他自己在内，大部分用户都用默认配置，所以先不实现；讨论里沉淀的判据留在 §三 的开放项里，不是本期契约。

前四条由 orchestra 在 brief 里转达；第五条与第六条是用户在 orchestra 的会话里口头补的 (语音输入，第六条前后说了两次、以收回那次为准)，经 orchestra 转述、本 ADR 未亲闻，措辞是意思不是逐字。用户同时点明：这次改造**最重要的就是跨批怎么判断在不在围栏里** (§二)，别的都是删代码。

改造有四条实测读数撑着，全部由 `limae-orchestra` 在 2026-09-10/11 本机 (dev-oregon) 亲手测得，本 ADR 引用时保留测法。**读数 B 是本设计的支点**，其余三条决定的是边界与预算。

**读数 A —— `MessageDisplay` 的批次边界永远落在行边界上。** 测法：把 `.claude/settings.local.json` 里 hook 的命令包一层 `tee`，把宿主送来的原始 payload 落盘再喂给 `limae hook`，行为不变。一个 session 采到 58 条真 payload (6 条消息)：`final=false` 的中间批次 52 条，**52 条以 `\n` 结尾，0 反例**；`final=true` 的末批 6 条，**0 条以 `\n` 结尾** —— 这是本观察的对照臂，它证明「以 `\n` 结尾」这个判据分得开两种情况、不是恒真。围栏代码块内部也是逐行发的：一个 ``` 块会拆成「开栏行 / 每行内容 / 收栏行」若干批。payload 的字段：`delta`、`final`、`index`、`message_id`、`cwd`、`session_id`、`transcript_path`、`scratchpad_dir`、`prompt_id`、`turn_id`、`hook_event_name`。

**读数 B —— 逐行修等于整篇修，当且仅当带着「我在不在围栏里」的状态。** 测法：取 session transcript 里 orchestra 自己的回复原文 250 篇 (transcript 存的是未经 hook 改动的原文)，每篇跑两次 `--fix`：一次整篇，一次逐行送入再拼回，归一化尾换行后逐字节比。**248 篇完全一致，2 篇不一致；2 篇的差别全部在围栏代码块里**，且同一形状 —— 整篇修得到 `result.status.success()`、`..limits() }`，逐行修得到 `result.status.success ()`、`..limits () }`：逐行送入时规则不知道这行在围栏里，把「英文与括号之间加空格」套到了 Rust 代码上。不带这个状态的失败剖面是最坏的一种：0.8% 的概率、静默、改坏的是代码。另记一条方法论的坑：这个实验的第一版读数是「250 篇 0 篇一致」，那是假的，差别全在测量循环自己补的那一个尾换行字节 —— **100% 的读数本身就是可疑信号**，翻开看才对。

**读数 C —— 机械修的成本比 polish 低三个数量级。** `target/release/limae --fix` 处理 3.4 KB 中文正文，20 次减去纯 `cp` 对照臂后**约 12 ms 一次** (含进程启动)；debug 构建约 59 ms。对照：`rust/polish/engines.rs` 的 `RUN_TIMEOUT` 是 600 s，`rust/hook/block.rs` 的 `TIMEOUT` 默认 60 s，`.claude/settings.local.json` 给 `MessageDisplay` 的 `timeout` 是 120 s。

**读数 D —— 机械修不是可有可无：一半的回复需要它。** 同一批回复原文 791 条，677 条够长 (≥ 60 字符)，跑 `--fix` 后 **353 条被改动、324 条本来就干净，改动率 52.1%**。抽样看改的是什么，全是排版：`**十二道门**(含…` → `**十二道门** (含…`、`判据文化——没有` → `判据文化 —— 没有`，一条语义都没动。

一条旧读数不能拿来当本 ADR 的证据，写在这里免得误用：同一 session 的 `diagnostics.jsonl` 里 `fix/repaired` 103 次。`rust/hook/block.rs` 的 `shown()` 吃的是引擎的重写，**那 103 记的是机械修改动了 polish 的输出，不是改动了模型原文**；读数 D 才是「原文需要机械修」的读数。

本 ADR 只做决定，**一行 Rust 都不改**；代码改造是下一个任务，它拿本 ADR 当规范，验收判据在「验收判据」一节。

## 决定

### 一、hook 的新合同：每批就地修，输出替换该批

hook 仍只挂 Claude Code 的 `MessageDisplay` (ADR-0009 §一 的依据不变：它是 display-only，「the stored message and what the model sees are untouched」，被修的文本永远不写回 transcript、不进模型上下文)。变的是每批做什么：

- **输入**：一条 `MessageDisplay` 事件。用到的字段只有 `hook_event_name`、`session_id`、`message_id`、`index`、`final`、`delta`、`cwd`；其余字段读到也不用。
- **处理**：把这一批 `delta` 当一段 Markdown，跑与 `limae --fix` 完全相同的确定性修复 (同一条 pipeline、同一个不动点语义)，只多带一个入口状态 —— 这一批开头是不是在围栏代码块里 (§二)。不再有短消息阈值：机械修对任何长度都正确且便宜，`LIMAE_HOOK_MIN_CHARS` 随之退役。
- **输出**：修后文本与 `delta` 不同时，输出 `{"hookSpecificOutput": {"hookEventName": "MessageDisplay", "displayContent": <修后文本>}}`；相同时**什么都不输出**。修后文本的结尾必须与 `delta` 逐字节一致 —— 有 `\n` 就有、没有就没有，因为 `displayContent` 替换的是这一批整段，丢一个换行会把两批粘成一行。
- **中间批与末批同等对待**：末批不再有特殊地位，它只是最后一个要修的批次；`delta` 为空 (消息以换行结尾时末批为空) 则无输出。
- **失败 fail-open**：任何一步失败，这一批无输出、原文照显示、退出码 0，并在会话态 `diagnostics.jsonl` 记一行 (§五)。这一条从 ADR-0009 §六 原样接过，宿主自己对 `MessageDisplay` 失败也是这样处理的。
- **`Stop` 事件不再处理**：收到即无输出。它此前只为 A/B 回注而挂，A/B 停了它就没有内容。Codex 的 `Stop` 同样无输出 —— `.codex/config.toml` 随下一个任务删除，但即便有残留配置仍指向 `limae hook`，进程也只是静默退出 0。
- **`LIMAE_HOOK_DISABLE` 保留**：非空即整个进程立即退出 0，是会话级的总开关。它此前还兼任递归防护 (hook 调起的引擎自己也是 agent)，现在 hook 不再起任何子进程，那半个用途消失，开关本身留下。
- **预算**：hook 自己不再有任何超时旋钮，`LIMAE_HOOK_TIMEOUT`、`LIMAE_HOOK_AB_RATE` 随引擎与 A/B 一起退役。一批的成本是读数 C 的十几毫秒加 §二 的兄弟批等待 (上限 2 s)，落在宿主给 `MessageDisplay` 的默认 10 s 之内，`.claude/settings.local.json` 的 `timeout` 不再需要。

这条合同与 ADR-0005 §四 的两段式边界一致：hook 只做确定性段，语义段 (polish) 留在 `limae polish` 与将来的 skill 里 (§七)。

### 二、跨批状态只有一个布尔：在不在围栏里，经会话态目录传递

读数 B 说明：把一条消息拆成批次来修，与整篇修唯一的分歧来自围栏代码块 —— `spec/rules.md`「全局豁免」第 1 条，围栏行本身与两条围栏之间的全部内容豁免，而 `rust/markdown.rs` 里这个豁免是一个逐行翻转的布尔 (`in_fence`)：去掉行首空白后以 ``` 或 `~~~` 开头的行翻转一次。所以**一批开头的围栏状态 = 它之前所有批次里围栏行数之和的奇偶**。读数 A 保证批次边界落在行边界上，一行不会被切成两半，这个奇偶因此定义良好。

**每批是一个新进程，状态只能经磁盘传**。hook 沿用今天的会话态目录 (`$TMPDIR/limae-hook/<session_id>/parts/<message_id>/`，位置无配置项、不许落在任何 checkout 里、目录 `0700`、文件 `0600`、写入先写临时名再 rename，全部沿用 `rust/hook/state.rs` 既有规则)，但**文件内容从「这一批的 `delta` 原文」改为「这一批里的围栏行数」**，一个十进制整数。于是：

- 第 `k` 批先写下自己的围栏行数，再等第 `0` 到 `k-1` 批的文件齐 (第 `0` 批没有可等的) (宿主并发派发各批、不等前一批结束，2026-09-01 在 Claude Code 2.1.257 上实测，`rust/hook/parts.rs` 的 `assemble` 已在处理同一件事；等待上限沿用 `SIBLING_WAIT` 2 s、`SIBLING_POLL` 20 ms)，求和取奇偶得到自己的入口状态，然后修。
- **等不齐就不修**：到期仍缺任何一批，这一批原文放行、记一行 `siblings` / `incomplete` (§五)。不知道在不在围栏里的时候，正确的动作是不动，因为两种猜法里错的那一种改坏的是代码。
- **中间批不以 `\n` 结尾时也不修**：这是读数 A 的前提被宿主打破的信号 (末批以外的批次不以 `\n` 结尾)，这一批原文放行、记一行 `fix` / `partial`。它不是防御性判断，是本设计唯一依赖的宿主行为的哨兵 —— 读数 A 的对照臂证明这个判据分得开两种情况。
- **磁盘上从此没有回复原文**。这是本条最实的副产品：ADR-0009 §八 与 ADR-0012 为「台账 / 记录里有助手回复原文」立的整套边界 (会话态、不入库、不跨 agent、24 小时过期) 仍然沿用，但被它守着的东西只剩下几十个整数与诊断行。`state::root` 拒绝落在 checkout 里、`identifier` 折叠 id 里的非常规字符，这些不因内容变小而放松 —— 规则简单，去掉它们不省什么。
- **消息的分片目录不再由末批删除**，改由既有的 `prune` 按 `ORPHAN_RETENTION` (1 小时) 扫掉。理由是新的竞争：末批与它前面的某一批可能同时在跑，末批删目录会让那一批读到「目录不存在」、白等到期。整数文件一小时后再清，代价是零。
- **不再需要的**：`parts::assemble` 拼整篇的逻辑 (没有整篇了)、`transcript_path` (不读 transcript，流式中它是否含当前消息未核过，且它是回复原文，本 ADR 刻意不碰)。

**围栏之外的跨行状态明确不带**。规则规范里另一处跨行的东西是行内指令 (`<!-- limae-disable -->` … `<!-- limae-enable -->` 的成对形态)，一条助手回复里几乎不会出现，读数 B 的 250 篇里也没有；第一批里的 `limae-disable` 管不到第二批，后果是该被保护的区段被修了排版 —— 方向与围栏相反 (多修而不是修坏代码)，且触发概率接近零。按「第三次重复才允许抽象」，它不进本 ADR；真出现再议。段落级的分组 (`rust/markdown.rs` 的 `paragraph` 与引用块延续判断) 也不带，读数 B 就是它不影响结果的证据 —— 证据等级要说清：250 篇、一位作者、一个 session。

入口状态怎么进 pipeline 由实现任务定：`Pipeline::fix` 加一个「起始在围栏内」的参数是干净的做法；在 `delta` 前拼一行合成围栏、修完再剥掉也能得到同样的结果，但它让 `fix` 的输入与用户看到的东西不再是同一段文本，出问题时不好查。两种都要过「验收判据」一节的判据。

### 三、配置沿用现状；用户级配置是开放项，本期不实现

hook 读的规则配置**一个字不改**：从事件的 `cwd` 起按今天的 `find_config` 发现 —— 逐层向上，每层先 `limae.toml` 再 `pyproject.toml` 的 `[tool.limae]`，**碰到含 `.git` 的那一层就停**，不跳出仓库继续上行；一层都没找到就是默认集。与 `limae [--fix] FILE...` 读到的是同一份，README「规则选择写进配置文件，仓库自己跑的 limae 与这条 hook 读到的才是同一份」照旧成立。

**配置错误的处理与 CLI 相反**：CLI 报错退出 2，hook fail-open —— 这一批原文放行，记一行 `fix` / `config` (§五)。这条不是新决定，是 ADR-0008 §六「失败语义分场景」在配置这一项上的落实。

**开放项 (不在本次决定范围内，实现任务不得当待办做)**：用户曾提出用户级配置 `~/.config/limae/limae.toml`，随后自己收回 —— 大部分用户 (包括他) 都用默认配置。若将来真有人需要，下面几条是那次讨论里已经想清楚、丢掉可惜的判据，届时另起 ADR 时从这里接：

- **只读、永不创建**：`install.sh` 不 `mkdir -p`，首次运行不写模板，缺文件不报 warning、不记诊断 —— 缺席是正常状态，不是待修复状态。
- **「不存在」与「存在但读不了」必须是两种不同的行为**：前者静默走默认、照修；后者这一批不修、记 `config`。否则用户改错一个键，行为悄悄退回默认、而他以为改生效了 —— 分辨力来自「屏幕上排版全部停修」这个可见变化。
- **两层之间整份覆盖、不逐键合并**，沿用 `spec/rules.md`「来源与优先级」的「一次运行只有一个来源生效」；再发明逐键合并 (`disable` 是并集还是替换、`severity` 怎么叠) 只会多出一批要定义、要测、要解释的边角。
- **只有 hook 读用户级，CLI 不读**：CLI 的输出要在本机、CI 与消费仓的 pre-commit 之间可复现，一份只在某人家目录里的文件会让本地跑与 CI 跑分叉；hook 修的是这个人屏幕上的回复。
- 一个「不读仓级、只认用户级」的开关，键名与默认值届时再定。

### 四、退役的旋钮与状态

- 环境变量：`LIMAE_HOOK_MIN_CHARS`、`LIMAE_HOOK_AB_RATE`、`LIMAE_HOOK_TIMEOUT` 退役；`LIMAE_HOOK_DISABLE` 保留。`LIMAE_ENGINE` 与 `[polish]` 表继续只归 `limae polish` 管。
- 会话态目录下的 `ab/` (A/B 台账) 与 `polish/` (单路记录) 不再写入；`pending.json` 不再产生。它们的边界规则由 §二 接过，用来守剩下的整数文件与诊断行。
- 中文双字编号词表 (`rust/hook/ab.rs` 的 `CODE_NAMES`)、候选池 (`CANDIDATES`)、`Stop` 回注 (`ab::context`) 全部失去对象，随下一个任务删除。

### 五、diagnostics 仍记，只记失败，Step 与 Kind 重定

`diagnostics.jsonl` 的四个字段 (`at`、`message_id`、`step`、`kind`) 与「只写 step 与 kind、不写正文」的边界不变。变的是取值集合 —— 本节是 orchestra 在 brief 里预判的地方，预判是「`Single` / `Ab` / `Record` / `Assemble` 与 `Incomplete` / `Misconfigured` 失去来源」，读完代码后前四个 Step 同意、两个 Kind 推翻、另加一个：

| | 处置 | 理由 |
| --- | --- | --- |
| Step `single` / `ab` / `record` | 删 | 没有引擎调用、没有 A/B、没有记录，三步都不存在了 |
| Step `assemble` → `siblings` | 改名 | 它不再拼整篇，只等兄弟批次的围栏计数；名字要说它做的事 |
| Step `fix`、`display` | 留 | 跑规则、顶层崩溃，都还在 |
| Kind `Engine(…)` 九种 | 删 | 没有引擎；`FailureReason` 本身留在 `rust/polish/` 给 CLI 用 |
| Kind `repaired` | 删 | 它记「规则改动了模型的重写」，是选型信号；现在改动就是 hook 的本职 (读数 D：一半的批次会改)，记它只是噪音 |
| Kind `incomplete` | **留** (预判说删) | 兄弟批次到期未齐仍会发生，而且从「只在末批」变成「每一批都可能」，它是 §二「等不齐就不修」的名字 |
| Kind `config` (`Misconfigured`) | **留、换来源** (预判说删) | 旧来源是 `[polish]` 表；新来源是 §三 的仓级规则配置读不了 (不是合法 toml，或触犯 `spec/rules.md`「配置错误」清单)。把它并进 `crashed` 会把用户自己的 toml 笔误报成「这是 bug，请报」，让人去错的地方找 |
| Kind `partial` | **新增** | §二 的哨兵：中间批不以 `\n` 结尾。它的下一步与其它几种都不同 —— 宿主的分批行为变了，去核读数 A |
| Kind `crashed` | 留 | 顶层崩溃与状态目录不合作 |

改后 Step 三个 (`siblings`、`fix`、`display`)，Kind 四个 (`incomplete`、`partial`、`config`、`crashed`)。`tools/check_repo_contracts.sh` 承诺 2 (手册 `kind` 表覆盖代码全集，全集由 Cargo example `hook-kinds` 打印) 保留，改造时它会正确地红，直到手册的表跟上 —— 这是设计好的红。`hook-kinds` 里 `Engine(REASONS)` 那一臂随 `Kind::Engine` 一起删。

### 六、Codex hook 废弃，连同它的契约

`.codex/config.toml` 删除；`rust/hook/cli.rs` 的 `codex_stop` 与 `is_codex_stop` 删除；`tools/check_repo_contracts.sh` 承诺 1 (Codex `Stop` 钩子未构建时 fail open、构建后透传退出码) 删除。删这条契约本身要两臂验收：先删 `.codex/config.toml` 而不动脚本，门必须红 (证明它在看)；再删脚本里承诺 1 那一段，门绿。`docs/knowledge/polish-hook-self-trial.md` 的 Codex 一节与 ADR-0014 的「实测」一节保留为历史记录，前者随手册改写时整节删除，后者 ADR 不改。

### 七、polish 不删也不被取代：hook 不再调它，它自己有两条路径

**改的只有一件事：hook 不再调 polish。** `rust/polish/`、`limae polish` 子命令、`spec/polish/` 的两层 prompt、`[polish]` 表、`LIMAE_ENGINE`、引擎探活与隐私边界 (ADR-0008 §三、§四、§六 CLI 部分、§七、§八、§九) 一概不变。hook 改造后 `rust/` 下引用 `crate::polish` 的只剩 `rust/cli.rs` 那一处子命令分发；今天 `rust/hook/` 从 polish 借用的两个小工具 (`polish::value` 读环境变量、`HOOK_DISABLE_VARIABLE` 常量) 搬不搬由实现任务定，它们与引擎无关。

polish 从此有**两条并列的消费路径、一个正本**：

- **CLI，headless**：`limae polish -` 照旧，谁想跑就跑，脚本与别的工具都能调。
- **生成出来的 skill**：供人装进自己的 agent，让模型写的时候就守规则 —— 与 ADR-0005 §一 的 agent-native 定位同一个方向，coding agent 用 skill 比事后调一个 CLI 更 native。

两条共用 `spec/polish/general.md` 与 `spec/polish/zh.md` 这一个正本，它们本来就是「可整体替换的 spec 文件」(ADR-0008 §九)；skill 是从它们生成的，不是另写一份。**本 ADR 不把 polish 描述成「被 skill 取代」**，那是错的。skill 的形态、生成方式、位置与分发属于后续 ADR，排期在 polish 的 prompt 再测一轮之后，这里不写任何契约细节。

### 八、被取代的 ADR 逐份处置

按本仓规矩，ADR 被新 ADR 取代而不就地改写，各份只在「状态」节记一行指回这里：

- **ADR-0009**：§二 (按批缓存、末批整段改写、短消息阈值)、§三 (A/B 双模型对照)、§四 (中文双字编号)、§五 (`Stop` 回注)、§八 (引擎隐私边界与台账风险面) 由本 ADR 取代。§一 (挂 `MessageDisplay`、display-only 的推论)、§六 (fail-open)、§七 (先本仓自试再交 machine-setup) 的内容由本 ADR §一 接过并成为正本 —— 被取代的 ADR 不能再当正本，指回去只会把读者弹到一份大半作废的文件。§七 那条路径本身仍然成立：`.claude/settings.local.json` 先在本仓试，稳定后经 `machine-setup` 分发到用户级，本 ADR 不排期。
- **ADR-0011**：整份取代。它的对象 (A/B 回注) 没有了。它「后果」里那条「已采到的两轮判词不是盲评结果、不能当型号证据」仍是事实记录，不因取代而改变。
- **ADR-0012**：整份取代。brief 预判它是「部分修订而非取代，因为 `limae polish` 仍产出记录」，读代码后推翻：`record_run` 与 `RUN_DIRECTORY` 的唯一调用方是 `rust/hook/block.rs`，`limae polish` 子命令从不写会话态记录，所以 ADR-0012 的对象随 hook 的引擎调用一起消失。它对 ADR-0008 §十「不写盘」的澄清 (指不动用户文件、不产生旁路产物，不指不许有会话态临时状态) 仍然正确，本 ADR §二 的整数文件正是按这个理解存在的。
- **ADR-0014**：整份取代 (§六)。
- **ADR-0008**：§十 P0 (hook 里的一次输出润色、A/B 挂在这里、§五 的判据靠它采集) 由本 ADR 取代；§二 (三个子命令) 与 §六 (hook 失败就用原文) 不变。**§五 (模型默认由 A/B 决定) 的证据路径不复存在**，处置写在「后果」第一条。
- **ADR-0015**：不改。阶段 D 的表记录的是当时交付了什么，本 ADR 改造的就是那份交付物。

## 后果

- **ADR-0008 §五 的洞，明写而不是装作不存在**：§五 把默认型号的判据钉在「用本仓与消费仓的真实中文段落做 10–20 条盲对照，采集手段就是 P0 那个 hook 的 A/B 模式」。这条采集手段没有了，而它从来没有产出过合格的证据 —— ADR-0011 记着此前采到的两轮判词不是盲评，随后 A/B 就被用户关掉 (`LIMAE_HOOK_AB_RATE=0`) 直到今天。处置是**冻结当前暂定默认** (codex `gpt-5.6-terra`、claude `sonnet`、grok `grok-4.6`，型号一律可配)，判据本身 (「降到便宜档必须有自家证据」) 不动，但**证据来源转为开放问题**：若将来要动默认型号，得另建离线对照 (例如对同一批语料跑两次 `limae polish` 再人评)，不再有活路径替它采样。§七 的两条路径里只有 CLI 那条还需要一个默认型号 —— skill 那条由写回复的模型自己守规则，没有型号可选 —— 所以这个开放问题的分量变轻，但没有消失。记进 `docs/tracker.md`。
- **hook 从此不能被观测「做了什么」**：ADR-0012 的记录是为比较模型改写的分布而存在的，机械修是确定性函数，同一输入永远同一输出，不需要分布；要量它的改动率，按读数 D 的测法对 transcript 离线跑一遍 `--fix` 即可，且不需要 hook 参与。诊断只记失败，这一点不变。
- **代码大幅缩小，是预期不是要求**：今天 `rust/hook/` 共 6724 行 (含测试)。A/B (`ab.rs` 与测试 1194 行)、块构建与引擎接线 (`block.rs` 与测试 1437 行)、改动渲染 (`render.rs` 及其 cases / tests 973 行，它渲染的是「润色改了几处」，机械修没有这块要渲染) 整体失去对象；`state.rs`、`parts.rs`、`cli.rs` 各留下变薄的一版。实现任务按本 ADR 删，不以行数为目标。
- **`docs/knowledge/polish-hook-self-trial.md` 整份要改写**：Codex 一节、A/B 台账两节、反馈一节、旋钮表、`kind` 表都过期；`.claude/settings.local.json` 的示例去掉 `Stop` 与 `timeout`。README「LLM 语义润色」一节末尾那句「还没做的：hook 形态与 A/B 采集」也过期。两处由实现任务一起改，不单独开 PR。
- **本仓的 `.claude/settings.local.json` 不入库**、正挂着 `tee` 采样，本 ADR 与实现任务都不碰它；改造合入后由用户自己去掉 `Stop` 那一项。
- **`tools/check_repo_contracts.sh` 从四条承诺变成三条**：承诺 1 删 (§六)，承诺 2 保留并会在改造中红一次 (§五)，承诺 3、4 (两个 pre-commit 钩子的 `files`) 不涉及。
- **ADR-0009 §七 的分发计划不变**：稳定后经 `machine-setup` 把 hook 配置装进用户级 Claude Code 设置，不由本仓直接写家目录。
- **不做的**：不改 `spec/`，规则语义一个字不动；不加用户级配置 (§三 的开放项)；不带围栏之外的跨批状态；不为 Codex 找替代事件。

## 验收判据 (给实现任务)

每条都要有对照臂，「改坏则红」与「真产物仍被 accept」一起测：

1. **逐批等于整篇**：对黄金 fixture 与一组含围栏代码块的合成消息 (围栏内放读数 B 那两个形状 —— `status.success()`、`..limits() }`)，逐行喂给 hook (带 `session_id` / `message_id` / `index` / `final`，中间批以 `\n` 结尾) 再拼回，必须与整篇 `--fix` 逐字节相等。对照臂：把围栏状态强制为「不在围栏里」(删掉兄弟批次的计数文件、或直接传 0)，同一组用例里围栏那几条必须分叉、非围栏的必须仍相等 —— 这证明状态是承重的，也证明其余用例不依赖它。语料用合成文本，不用 transcript (回复原文不进仓库)。
2. **结尾逐字节保留**：末批不带 `\n`、中间批带 `\n`，修后输出的结尾与输入一致；末批为空则无输出。
3. **等不齐就不修**：缺一批的目录，到期后该批无输出，`diagnostics.jsonl` 多一行 `siblings` / `incomplete`；补齐后同一批有输出。
4. **哨兵**：一条不以 `\n` 结尾的中间批，无输出，记一行 `fix` / `partial`；同一内容加上 `\n` 则正常修。
5. **配置照旧、配置错误 fail-open**：`cwd` 所在仓库有一份关掉某条规则的 `limae.toml` 时那条规则不修、没有时修；仓级文件不是合法 toml 时这一批无输出、记一行 `fix` / `config`，而不是退回默认集照修。对照臂是同一批文本在合法配置下有输出。
6. **不碰家目录**：跑完一批，`$HOME` 下没有多出任何 `limae` 相关的文件或目录 (§三 的开放项没有被顺手做掉)。
7. **`Stop` 与 Codex `Stop`** 各喂一条，无输出、退出 0。
8. **契约门两臂**：§六 的两步。承诺 2 在 `hook-kinds` 改后、手册改前红，改后绿。
9. **十三道门裸跑全绿**，一条都不接管道，逐条看退出码。

## 状态

accepted (2026-09-11)。
