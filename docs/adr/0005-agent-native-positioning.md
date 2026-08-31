# ADR-0005：agent-native 定位与愿景边界

- 日期：2026-08-31
- 承接：ADR-0001 (规范先于实现、多实现共用黄金 fixture)、ADR-0002 (Rust 主实现方向)

## 背景

前四份 ADR 只回答「规则与配置怎么写」，没有回答「这个项目是什么、要长成什么样」。规则集长到 R11、配置模型定稿之后，接下来的取舍 —— 要不要管代码里的文本、要不要接大模型、拿谁当参考 —— 已经不是单条规则的讨论能决定的。

用户 2026-08-31 口头给出定位与愿景，本 ADR 是这次口述的正式落库。证据链是两份调研：`docs/research/zh-typography-guidelines-survey.md` (中文排版规范与同类工具盘点)、`docs/research/claudish-and-ai-slop-survey.md` (Claudish 语言现象、去 AI 腔工具、heuristic learning 出处核实)。

本 ADR 只定方向与边界，不展开设计：每个方向真要动手时另起 ADR，未做的条目挂在 `docs/tracker.md`。

## 决定

### 一、定位：面向全球的严肃开源项目，agent-native 开发

`lo-md-lint` 是一个严肃的、面向全球用户的开源项目，不是个人仓内的自用脚本；它同时是 agent-native 的 —— 由 agent 自主开发与维护，人只做裁决与验收。仓库里已有的那套约束 (规范先于实现、黄金 fixture 当回归、AGENTS.md 写清协作与质量门) 正是为了让 agent 能独立推进而人仍然看得懂、改得动，本 ADR 把它确认为项目的第一性约束，而不是当前阶段的临时安排。

### 二、参考对象：zhlint 与 AutoCorrect，distill 不回避

zhlint 与 AutoCorrect 是前 agentic 时代的成熟手作，列为本项目的重点参考对象。其中 AutoCorrect 权重更高：多语言 SDK 与多平台分发、工具链成熟度在同类里最高，而且它本身就是 Rust，与本仓的 Rust 终态 (ADR-0002) 一致。

「把优秀开源 distill 成符合自己要求的新项目」是 agent 时代最快的开发方式，本项目不回避这条路径，也不因此把参考对象当依赖。distill 的边界沿用调研已给出的判断：抄机制 (规则独立 id 与开关分级、配置发现、行内 disable、忽略文件、按文件类型只扫可见文本)，抄逃生口 (`skipZhUnits`、`skipAbbrs` 这类例外列表)，不抄别人的默认值与整体配置形态 —— 默认值以本仓家规为准 (`zh-typography-guidelines-survey.md` §4.1、§4.2、§5.5)。参考来源要在规范或 ADR 里注明出处。

### 三、第一阶段只做 Markdown

第一阶段的范围只有 Markdown 文档，不做代码内文本 (注释、字符串字面量) 的检查。理由是 agent 时代代码注释很少再给人读，投入产出不成比例；AutoCorrect 那套按文件类型解析 AST、只扫注释与字符串的能力，是参考对象的能力，不是本阶段的目标。

### 四、LLM 语义润色与确定性 lint 互补，两段式边界

未来加一类新特性：由 agent 或 API 调用大模型，对文档语言做语义 (semantic) 层的润色。它与 lint 是互补而不是替代 —— lint 快、公式化、确定性、可进 CI；model-based 慢、在语义层、结果要人审。

动机来自实际使用：模型写中文的质量不够，会把 `secret` 译成「秘密」而不是「密钥」，也会造出「正在飞」这类不成话的说法；英文一侧同样有 Claudish 问题，社区已经出现 Claudish-to-English 一类工具 (`claudish-and-ai-slop-survey.md` §1)。

边界是两段式，两段不混：

- **确定性段**：词表、句式模板、密度这类可以逐行判定、可以写进黄金集的东西，才能成为规则。
- **语义段**：语义压缩、降抽象、隐喻堆叠这类只有模型判得动的东西，走 LLM 润色，产物是旁路文件或建议，人审之后才回写；不进 required check，也不拿模型当 CI 判定器。

### 五、规则分 fixable / non-fixable，non-fixable 报 warning

学 ruff 的做法，规则分 fixable 与 non-fixable 两档：现有 R1–R11 这类逐行、修复唯一的规则是 fixable；从语言习惯里长出来的规则 (术语选词、破折号过量之类) 多数不可确定性修复，标为 non-fixable，只报 warning 不改文本。

这一档还接住了两段式的衔接：warning 积累到一定量时，由 agent 主动去调 model-based 的修复，而不是让 `--fix` 硬改。分级怎么落进 `spec/rules.md` 与输出格式，届时另起 ADR。

### 六、heuristic learning 路线：从润色 pair 蒸馏实验规则

积累语义润色前后的 pair (模型腔的原文 → 人话的改写)，从中蒸馏 experimental 的 rule-based lint，例如术语选词 (密钥 vs 秘密) 与破折号过量。这类规则先按上一条当 non-fixable warning，默认关闭。

方法上的参照是翁家翌 (Jiayi Weng, `@Trinkle23897`) 的 *Learning Beyond Gradients* (2026-05-08) 提出的 heuristic learning：学习的主体是显式的程序代码而不是网络权重，闭环由 coding agent 直接改规则、测试与配置来完成 (`claudish-and-ai-slop-survey.md` §4)。本项目要迁移的是它的运维模型，不是它的实验结果：

1. 规则写在 `spec/` 里，是人读得懂的显式软件；
2. 黄金 fixture 就是 golden trace，把旧能力固化成回归；
3. 从 pair 只**提议**规则，人审之后才进规范；
4. 新规则必须跑全量黄金集，回归失败就改规则或删规则 —— 规则可读、可回归、可删是这条路线的前提。

不做的是「让模型自己长出不可读的检测器」，也不做拿 LLM 当 AI 腔判定器的 CI。

### 七、工程化留位：website、多语言文档、社区贡献

三件事确认为未来方向，本阶段只留位不动手：文档站点与多语言文档 (对标 AutoCorrect 的站点组织)、接受社区贡献所需的 `CONTRIBUTING` 与治理约定、以及随之而来的分发面。一切渐进：设计新东西时给它们留出位置，不提前建空壳。

## 后果

- 本 ADR 是「这个项目是什么」的正本；`README.md` 的「定位与愿景」是它的摘要，两者不一致时以本文为准。
- 未做的条目在 `docs/tracker.md`「愿景」与「claudish 调研产出」两节；本 ADR 不新增 backlog，也不替代 tracker 记账。
- 落地顺序不在此定。上面每一条真要实现时另起 ADR：fixable / non-fixable 分级、experimental 规则的规范位置、LLM 润色原型的形态，都要各自定案后再动 `spec/`。
- 参考对象的机制可以照抄，默认值不照抄 —— 已有 R1–R11 的默认值不因为「上游怎么做」而变动，要变得有本仓自己的依据。

## 状态

accepted (2026-08-31)。
