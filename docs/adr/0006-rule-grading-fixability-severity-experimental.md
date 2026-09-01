# ADR-0006：规则分级 —— 可修复性 / 严重度 / 成熟度三轴正交

- 日期：2026-08-31
- 承接：ADR-0005 §五 (规则分 fixable / non-fixable，non-fixable 报 warning) —— 本 ADR 取代它；配置层沿用 ADR-0003 (`disable` 键与两种同构载体) 与 ADR-0004 (默认关闭的规则与 `enable` 键)

## 背景

ADR-0005 §五把「规则能不能自动修复」与「违规报得多重」当成同一件事：fixable 的规则改文本，non-fixable 的规则只报 warning；§五末尾又把 warning 的积累接到大模型上 —— 「warning 积累到一定量时，由 agent 主动去调 model-based 的修复」。

用户 2026-08-31 口头裁决纠正了这两处：可修复性与严重度是两回事，不可修复的规则 (破折号占全文字符比例超阈值、`secret` 该不该译成「密钥」这类) 用户照样可以定成 error；LLM 润色是独立的一条线，不挂在 lint 的 warning 上，也不以 claudish 检测为前提。同一次口述还补了三条边界：experimental 到底指什么、规则不分语言、`bracket_style = "contextual"` 不做。本 ADR 是这次口述的正式落库。

证据链是两份调研：

- `docs/research/claudish-and-ai-slop-survey.md` §3.4 —— 业界做去 AI 腔的默认切分是两段，「确定性 floor」(词表、句式、密度 → 报告，多数不可确定性修复) 与「可选 LLM pass」(只在用户要润色时跑，产物是改写或建议，不进 required check)；不做第三种「拿 LLM 当 linter 判定器」。§6 的分层表进一步指出，同一层里词表类规则可 fix、句式类多半不可 —— 可修复性是逐条规则的性质，不是某一层的属性。
- `docs/research/autocorrect-zhlint-internals.md` §1.2 与 §4.2 第 4 条 —— AutoCorrect 的严重度是配置里的 `HashMap<String, SeverityMode>` (0 = off、1 = error、2 = warning)，`format` (fix) 只跑 error、warning 的规则修复时整条跳过，即「warning = 只报不修」；zhlint 则根本没有 error / warning 分级。调研的判断是：机制本身干净，但把「开不开」与「多严重」缠进同一张表不可照抄，non-fixable 宜做成规则属性 (ruff 的 fixable)。

本 ADR 只定分级模型：键名与形状在这里定死，落进 `spec/rules.md` 与各实现是后续任务，本次不动 `spec/`、不动 fixture、不动代码。

## 决定

### 一、每条规则三个正交属性

| 轴 | 取值 | 谁定 | 含义 |
| --- | --- | --- | --- |
| 可修复性 (fixability) | fixable / non-fixable | 规则自身的性质，写死在 `spec/rules.md` 的条目里，用户不可配 | fixable = 有唯一、确定性的修复，`--fix` 会改；non-fixable = 只报不改 (密度、选词这类没有唯一修法的规则) |
| 严重度 (severity) | error / warning | 规范给每条规则一个默认值，用户可按规则 id 覆盖 | error 参与退出码，warning 不参与；两者都进报告 |
| 成熟度 (maturity) | stable / experimental | 规则自身的性质，在 `spec/rules.md` 标注，用户不逐条指定 | experimental = 还在开发、误报率未达标的 flaky 规则；默认不进启用集，用户侧只有一个总开关 `enable_experimental` (布尔，默认关)，打开即全部纳入 |

三轴互不推导，任意组合都合法：不可修复的规则可以是 error (这正是裁决要纠正的那一点)，experimental 的规则可以是 fixable、也可以默认 error。

- **现有 R1–R11 全部是 fixable + error + stable，本 ADR 零行为变化。** R9 已经默认关闭，但它是 stable 的「默认关」 —— 关的理由是上游规范自标争议 (ADR-0004)，不是误报率没验过。「默认开关」是配置层的事实，「成熟度」是规则的生命周期标签，两者独立：R9 照旧用 ADR-0004 的 `enable` 键打开，与下面的 `enable_experimental` 无关。
- 严重度只影响报告与退出码，不影响修复：**fixable 的规则被降成 warning，`--fix` 照样修**。这一条是与 AutoCorrect 分道的地方 (它的 warning 等于 fix 时跳过)，也是本仓既有契约的延续 —— 启用集之内 check 与 fix 看同一批规则。要一条规则不改文本，办法是关掉它 (`disable`) 或就地用行内指令，不是把它降成 warning。
- 退出码：启用集里有 error 违规就是非零；只有 warning 违规时退出码仍是 0，报告照常打印。配置错误的退出码不变 (`spec/rules.md`「配置错误」，Python 参考实现用 2)。
- 举两个例子说明三轴独立：破折号密度 (全文 `——` 占比超阈值) 是 non-fixable —— 超标了也不知道该删哪一个；术语选词是 fixable —— 词表就是 `wrong = right` 的替换 (`claudish-and-ai-slop-survey.md` §5.2 的 AutoCorrect 形制)。两者的默认严重度与阈值在各自规则定案时再定，这里只借它们说明「不可修复」不等于「不重要」。

### 二、严重度的配置形状：一个 `severity` 表，没有 CLI flag

用户覆盖默认严重度的键是 `severity`，一张 toml 表，键是规则 id、值是 `"error"` 或 `"warning"`：

```toml
severity = { R8 = "warning" }
```

- 未知规则 id、以及 `"error"` / `"warning"` 之外的取值，都是配置错误，实现必须报错退出而不是静默忽略 —— 与 `disable` / `enable` 的未知 id 同款 (ADR-0003 §二)。
- 表的形状按规则 id 组织，规则集长大不需要新键，与 `disable` / `enable` 一致。
- **不加 CLI flag。** 严重度是仓库级的长期口味，不是「临时试一把」的东西；沿用 ADR-0003 §五「一次运行只有一个来源」的语义，命令行上出现 `--disable` / `--enable` 时配置文件整体不生效，`severity` 与 `skip_zh_units` 一样随之回到默认值，不为它另开例外。
- 黄金 fixture 的 `.findings` 格式不变，仍是 `<行号> <规则 id>`：某条违规是 error 还是 warning，由规范的默认值与 case 的 `.conf` 唯一决定，runner 需要时自行推导，不写进 findings 行。要不要为退出码另加断言，留给落地任务定。

### 三、experimental：定义、入口与唯一的总开关 `enable_experimental`

**定义**：规则规范已经写进 `spec/rules.md`、有黄金 fixture，但误报率还没有在真实语料上验证到可以默认打开。它是生命周期标签，既不是严重度也不是可修复性。

**入口**：新规则的判定只要依赖词表、密度或句式模板 (即从语言习惯里长出来的)，一律先 experimental；纯字符级的排版规则 (R1–R11 这一类) 可以直接 stable。

**成熟度是规则自身的性质，只在 `spec/rules.md` 标注，用户不逐条指定。** 用户侧只有一个布尔配置键 `enable_experimental`，默认 `false`：

```toml
enable_experimental = true
```

- 打开就是把**全部** experimental 规则纳入启用集；纳入之后它们与普通规则同等对待 —— 可以用 `disable` 逐条关掉、用 `severity` 逐条覆盖严重度。
- **不复用 ADR-0004 的 `enable` 逐条打开 experimental 规则**：experimental 规则 id 无论出现在配置文件的 `enable` 键里、还是出现在命令行的 `--enable` 上，都是同款错误 (配置错误的那个退出码)，实现必须报错退出。两条路径都堵死，成熟度才不会变成用户逐条改写的东西。
- experimental 规则 id 出现在 `disable` 或 `severity` 里是合法的：`enable_experimental` 打开时生效，关闭时是空操作 —— 与 ADR-0004「列出默认启用的规则是允许的空操作」同理。
- 值不是布尔是配置错误。同样不加 CLI flag：命令行上出现 `--disable` / `--enable` 时配置文件整体不生效，`enable_experimental` 与 `severity`、`skip_zh_units` 一样回到默认值。**所以命令行上没有任何打开 experimental 规则的办法** —— 既没有逐条的 `--enable <experimental-id>` (上一条已定为错误)，也没有总开关；唯一入口是配置文件里的 `enable_experimental = true`，而它与 `--disable` / `--enable` 不能同时用。

**毕业与回退**：experimental 毕业成 stable、以及 stable 出现系统性误报后降回 experimental，都由用户 (或用户与 orchestra 一起) 逐条手动裁决，本 ADR 不写量化判据；将来有外部用户、收到真实反馈之后，再考虑把判据规则化。

### 四、LLM 语义润色是独立一条线

明确取代 ADR-0005 §五末尾的耦合：**不存在「warning 积累到一定量就去调 model-based 修复」这条机制**，规范与实现里都不会有 warning 到 LLM 的绑定。lint 的输出可以是 agent 润色时的输入之一，但那是使用方式，不是产品契约；LLM 润色同样不以 claudish 检测为前提，没有 claudish 规则也可以润色。

其余边界沿用 ADR-0005 §四 (两段式、语义段产物是旁路文件或建议、人审后才回写、不进 required check、不拿模型当 CI 判定器)，本 ADR 不重复展开、也不改动。

### 五、规则不分语言

规则对文本本身生效：不做语言探测、不按文件或目录指定语言、不给规则加「语言」这一维度。文档天然中英混杂，一个句子里就可能两种语言都有，文档级的语言开关在这里没有意义。

英文侧的规则是 English-to-English 的 (例如 `load-bearing` 的过量使用、`delve` 的密度)，出现在哪就管到哪，中文文档里出现同样报。调研里说的「中英两套指纹」只是两份词表各管各的模式，不是让用户选一种语言。

与 backlog 里「裸日文段落的语言探测」的关系：留着的候选修法 (探测独立连续假名子串，跳过该子串邻接边界的 R4) 是字符类启发式加局部边界豁免，与本条不冲突；如果它将来长成「按段判定语言、整段换一套规则」，那就违反本条，不做。

### 六、不做：`bracket_style = "contextual"`

括号策略的第三档「有中文用全角、纯英文用半角」不做，R2 / R3 的默认 `half` 不变。tracker 里的条目保留作记录 (含已有的 debate verdict)，本 ADR 只记这个结论。

## 后果

- ADR-0005 §五被本 ADR 取代，0005 不就地改写，只在它的「状态」节记一行指向这里；§五之外的六节仍然有效。
- `docs/tracker.md`「规则 fixable / non-fixable 分级」条目由 orchestra 在合入后改写：要做的事从「non-fixable 当 warning 报」变成「`severity` 键与三轴标注落进 `spec/rules.md` 与实现」。
- 后续任务要动 `spec/rules.md` 三处：每条规则条目补三轴标注、「配置」一节加 `severity` 与 `enable_experimental` 两个键、「配置错误」清单加三条 (`severity` 的未知 id 与非法取值、experimental 规则 id 出现在 `enable` 键或 `--enable` flag、`enable_experimental` 值不是布尔)。本 ADR 不改 `spec/`，也不改任何实现或 fixture。
- 现有消费方不受影响：R1–R11 的可修复性、严重度、成熟度都是今天的行为，不加配置就没有任何变化。
- 新规则的默认路径从此是 experimental：默认不进启用集，要开 `enable_experimental` 才跑，什么时候毕业成 stable 由用户逐条裁决。这条路径是 ADR-0005 §六 heuristic learning 的落脚点。
- `README.md`「定位与愿景」没有与 ADR-0005 §五同义的句子，本次不改。

## 状态

accepted (2026-08-31)。
