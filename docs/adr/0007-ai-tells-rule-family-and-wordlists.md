# ADR-0007：AI 腔规则家族的规范位置与第一批

- 日期：2026-08-31
- 承接：ADR-0005 §四 (确定性段 vs 语义段) 与 §六 (从语言习惯长出的规则先做 experimental)、ADR-0006 (可修复性 / 严重度 / 成熟度三轴与 `enable_experimental`)；配置层沿用 ADR-0003 与 ADR-0004

## 背景

ADR-0006 把 experimental 这条轴定死了，但规则集里一条 experimental 规则都没有，`enable_experimental` 至今是空操作。ADR-0005 §六给的路线是「从润色 pair 蒸馏 experimental 规则」，落脚点应该是中文 AI 腔与术语选词 —— 也正是 `docs/research/claudish-and-ai-slop-survey.md` §7.2 第 1 / 5 条建议的下一步实验：手工蒸馏一小批 experimental warning，加一张带语境锚点的术语小表。

真要动手，有三件事 ADR-0005 / ADR-0006 都没回答：新规则的 id 长什么样、词表这类判定数据放在仓库的哪一层、以及逐行模型能装下 AI 腔的哪一部分。本 ADR 回答这三件事，并记下第一批的内容与它在真实语料上的误报率。

## 决定

### 一、规则 id 按家族分前缀，但前缀不是配置维度

规则 id 从此按家族 (family) 分前缀，学 ruff 的做法：

| 前缀 | 家族 | 第一批 |
| --- | --- | --- |
| `R` | 中文排版 (typography)：字符宽度、空格、标点 | R1–R11 (已有) |
| `A` | 中文 AI 腔 (AI tells)：套话、句式、黑话、聊天残留 | A1–A4 |
| `T` | 术语选词 (terminology)：`wrong = right` 的词表替换 | T1 |

- **前缀只是命名**：全部规则共用同一个 `disable` / `enable` / `severity` 命名空间，加一个家族不需要新键。这与 ADR-0003 §二「键按规则 id 组织」和 ADR-0006 §五「规则不分语言」都一致 —— `A` 不代表「中文规则包」，将来英文 tells 是另一批词表，不是另一个开关。
- id 在家族内从 1 开始，稳定且不复用，与 `R` 一样。
- 术语选词单开 `T` 而不是塞进 `A`：它是唯一 fixable 的一支，判定数据的形状 (`wrong` / `right` / `anchors`) 也与 A 家族的短语表不同。

### 二、词表放 `spec/wordlists/`，是规范的一部分

A1 / A3 / A4 / T1 的判定靠词表。词表放仓根 `spec/wordlists/`，与 `spec/rules.md`、`spec/fixtures/` 平级：

- `A1.txt` / `A3.txt` / `A4.txt` —— UTF-8 纯文本，一行一条，`#` 注释、空行忽略。
- `T1.toml` —— 一个 `entries` 数组，每条 `wrong` / `right` / `anchors`。

理由与 ADR-0001 把规范和黄金集放进 `spec/` 是同一条：**词表是判定的一部分，所以是规范的一部分**，所有实现共用同一份，不进任何单一实现的私有目录。**加一条词就是改规范，不改任何实现的代码** —— 这正是 ADR-0005 §六 heuristic learning 要的形状：规则与数据都是人读得懂、可回归、可删的显式软件。

Python 参考实现经 `importlib.resources` 读这些文件，`src/lo_md_lint/wordlists` 是指向 `spec/wordlists/` 的目录级软链 (与 `.claude/skills/` 指向 `.agents/skills/` 同一个手法)；editable 安装与打好的 wheel 都解析得到，`uv run lo-md-lint` 与 `uvx --from . lo-md-lint` 两条路都实测能找到词表。

### 三、第一批只做逐行可判的规则，文档级密度留下一批

`spec/rules.md`「处理单位」与黄金集的 `.findings` 都是逐行模型 (`<行号> <规则 id>`)。AI 腔里的**破折号密度、粗体密度、列表化行文**这类判定是文档级的 —— 一处违规不落在某一行上，报告位置无从写起。要做它们得先给规范加一种文档级 finding 的形制 (以及 `.findings` 怎么表达它)，那是独立的一步。

**本批只收逐行可判的规则**，文档级密度留待下一批。这与调研 §6 的分层表一致：确定性 floor 里同样有逐行与跨句之分。

### 四、第一批五条：全部 experimental + warning + 默认关

| id | 名 | 判定 (逐行) | 可修复性 | 默认严重度 | 成熟度 |
| --- | --- | --- | --- | --- | --- |
| A1 | 中文套话 | 行内出现 `A1.txt` 里的开场白 / 连接套话 | non-fixable | warning | experimental |
| A2 | 否定平行 | 同一行内「不是 … 而是 …」，相距 ≤ 20 字符 | non-fixable | warning | experimental |
| A3 | 互联网黑话 | 行内出现 `A3.txt` 里的黑话 | non-fixable | warning | experimental |
| A4 | 聊天残留 | 行内出现 `A4.txt` 里的整句级模板 | non-fixable | warning | experimental |
| T1 | 术语选词 | `T1.toml` 某条的 `wrong` 命中，且同行有该条锚点 | fixable | warning | experimental |

判定层面的几条取舍，都是为压误报：

- **A1 / A3 / A4 一行只报一处**：这类词天然成串出现，逐处报只会刷屏。T1 每个命中各报一处，与修复一一对应。
- **A2 只收「不是 … 而是 …」，不收「不仅 … 更 …」**：递进句式在中文技术写作里是正常用法。20 字符的上限把跨句的巧合排除掉。
- **A3 不收「对齐」「沉淀」「闭环」「生态」**：它们在技术文档里有正当用法 (对齐两份配置、闭环控制、依赖生态)，dogfood 也证实了 (下节)。
- **T1 必须带锚点**：一个词该不该换取决于语境 (调研 §5.3)，所以只在同一行出现该条自己的锚点 (`token` / `OAuth` / `cache` 这类) 时才报、才改。锚点是语境证据不是违规，所以在整行范围内查找、不受全局豁免约束 —— 最常见的锚点正是行内代码里的 `` `token` ``；违规本身照旧受豁免约束。没有稳妥锚点或唯一替换的直译词 (「门控」「一等公民」「契约」) 不进表。
- **不做 error 级单点禁词、不灌大词表、不做繁中本地化**，与调研 §7.3 一致。

五条全部 experimental，唯一入口是 `enable_experimental = true` (ADR-0006 §三)；默认集不含它们，现有消费方零行为变化。默认严重度 warning：单点 tell 的假阳性高 (调研 §2.4)，让它们决定 CI 的退出码不合适；用户要更严可以用 `severity` 逐条改成 `error`。

### 五、dogfood 校准是这批的验收方式

experimental 的定义是「误报率还没在真实语料上验证过」(ADR-0006 §三)。这批的做法是：在四份真实中文 Markdown 语料上开 `enable_experimental = true` 跑一遍，逐条命中人工判定真阳性 / 误报，误报率高的词当场从词表里删掉，然后把数字记进本 ADR。四份语料是本仓与三个私有仓 (`machine-setup` / `wealth-management` / `butler`)，共 114 份 Markdown。

第一轮校准的结果 (2026-08-31)：

| 语料 | 份数 | A1 | A2 | A3 | A4 | T1 | 判定 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `lo-md-lint` (本仓) | 18 | 2 | 12 | 7 | 3 | 4 | 28 处里 27 处是规则自己的文档在举例或转述调研原文 (`spec/rules.md`、`README.md`、本 ADR、`docs/research/`)，即 mention 而非 use；剩下 1 处是本 ADR 正文真的用了那个句式。规则每一处都判对了 |
| `machine-setup` | 40 | 0 | 1 | 0 | 0 | 0 | 真阳性 |
| `wealth-management` | 44 | 0 | 6 | 1 | 0 | 0 | 真阳性 |
| `butler` | 12 | 0 | 1 | 0 | 0 | 0 | 真阳性 |

三个私有仓共 96 份 Markdown、9 处命中，逐条人工判定**全部是真阳性** —— 命中的确实是「不是 … 而是 …」这个句式本身、或词表里的那个黑话，没有一处是模式匹配的意外。密度上也不刺眼：平均每 10 份文档不到 1 处。

**校准中删掉的一条：`秘密` → `密钥`。** 删之前它在四份语料上命中 56 处 (`machine-setup` 43、本仓 9、`wealth-management` 3、`butler` 1)，**逐条判定全部是误报**。原因不是锚点选得不好，而是锚点在这里与错误**负相关**：一份讲凭证管理的文档必然满篇 `secret` / `token` / `credential`，而正是这种文档里「任何秘密」「零秘密」「程序化秘密」是完全正确的中文。调研 §5.3 早就说过 `secret` 要分语境，dogfood 进一步说明「同一行的英文锚点」还不足以当那个语境。它又恰好是 fixable 的，留着会真的改坏文本 —— 直接从词表里删掉，理由记在 `T1.toml` 的注释里。

**A1 与 A4 在三个私有仓上是零命中**：这说明这两张词表不误伤，但也说明它们的真阳性率还没验过 —— 这正是它们留在 experimental 的理由。

**这不是毕业判据。** 毕业成 stable 仍按 ADR-0006 §三由用户逐条裁决；本节只是把「验过没有」这件事从口头变成有数字的记录。

## 后果

- `spec/rules.md` 三处变化：「通用模型」加「词表」小节、「规则属性」加前缀约定与实验规则清单、末尾加 A1–A4 / T1 五条条目；「修复顺序」把 T1 排在最前 (选词换出的字要落进后面全部规则的判定范围)。
- `spec/wordlists/` 是新的一层规范数据，`spec/README.md` 说明各实现怎么读它。
- `enable_experimental` 从今天起不再是空操作，`severity` 的默认值也不再「全是 error」。
- 现有消费方零行为变化：不开 `enable_experimental` 就与 v0.6.0 完全一致，本仓自己的 dogfooding 钩子也不受影响。
- 下一批的两件事挂着：**文档级密度规则**要先给规范加文档级 finding 的形制；**英文 tells 词表**按 ADR-0006 §五是同一套规则机制下的另一份词表，不是新的语言开关。两者由 orchestra 在合入后记进 `docs/tracker.md`。
- 本 ADR 不改 LLM 润色那条线 (ADR-0006 §四)：warning 不触发任何 model-based 修复。

## 状态

proposed (2026-08-31)。
