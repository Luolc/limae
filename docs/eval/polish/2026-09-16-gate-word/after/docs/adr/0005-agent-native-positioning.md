# ADR-0005：agent-native 定位与愿景边界

- 日期：2026-08-31
- 承接：ADR-0001 (规范先于实现、多实现共用黄金 fixture)、ADR-0002 (Rust 主实现方向)

## 背景

前四份 ADR 只回答了「规则与配置怎么写」，没有回答「这个项目是什么、要成为什么样」。规则集扩展到 R11、配置模型定稿后，接下来的取舍 —— 要不要检查代码中的文本、要不要接大模型、以谁为参考 —— 已经不是讨论单条规则能决定的。

用户于 2026-08-31 口头说明了项目定位与愿景，本 ADR 将这次口述正式归档。证据链包括两份调研：`docs/research/zh-typography-guidelines-survey.md` (中文排版规范与同类工具盘点)、`docs/research/claudish-and-ai-slop-survey.md` (Claudish 语言现象、去 AI 腔工具、heuristic learning 出处核实)。

本 ADR 只确定方向与边界，不展开设计：各项方向真正实施时另起 ADR，尚未实施的条目记在 `docs/tracker.md`。

## 决定

### 一、定位：面向全球的严肃开源项目，agent-native 开发

`lo-md-lint` 是面向全球用户的严肃开源项目，不是个人仓库里的自用脚本；它也是 agent-native 项目 —— agent 自主开发和维护，人只负责裁决与验收。仓库现有的约束 (规范先于实现、黄金 fixture 作为回归、AGENTS.md 明确协作方式与质量检查) 就是为了让 agent 能独立推进，同时让人能看懂、能修改。本 ADR 将这些约束确认为项目的第一性约束，而非当前阶段的临时安排。

### 二、参考对象：zhlint 与 AutoCorrect，distill 不回避

zhlint 与 AutoCorrect 是前 agentic 时代成熟的人工项目，列为本项目的重点参考对象。其中 AutoCorrect 的权重更高：它提供多语言 SDK 与多平台分发，工具链在同类项目中最成熟，而且本身使用 Rust，与本仓的 Rust 终态 (ADR-0002) 一致。

「把优秀开源 distill 成符合自己要求的新项目」是 agent 时代最快的开发方式，本项目不回避这条路径，但也不会因此把参考对象当作依赖。distill 的边界沿用调研中的判断：借鉴机制 (规则独立 id 与开关分级、配置发现、行内 disable、忽略文件、按文件类型只扫描可见文本)，借鉴逃生口 (`skipZhUnits`、`skipAbbrs` 这类例外列表)，不照搬别人的默认值与整体配置形式 —— 默认值以本仓家规为准 (`zh-typography-guidelines-survey.md` §4.1、§4.2、§5.5)。参考来源要在规范或 ADR 中注明出处。

### 三、第一阶段只做 Markdown

第一阶段只处理 Markdown 文档，不检查代码中的文本 (注释、字符串字面量)。原因是 agent 时代的代码注释很少再由人阅读，投入产出不成比例；AutoCorrect 按文件类型解析 AST、只扫描注释与字符串的能力，是参考对象的能力，不是本阶段的目标。

### 四、LLM 语义润色与确定性 lint 互补，两段式边界

未来会增加一类新特性：由 agent 或 API 调用大模型，润色文档语言的语义 (semantic) 层。它与 lint 互补，不互相替代 —— lint 快、公式化、确定性，可进 CI；model-based 慢，处理语义，结果需要人审。

动机来自实际使用：模型写中文的质量不够，会把 `secret` 译成「秘密」而不是「密钥」，也会造出「正在飞」这类不成话的说法；英文也有 Claudish 问题，社区已经出现 Claudish-to-English 一类工具 (`claudish-and-ai-slop-survey.md` §1)。

边界分为两段，两段不混：

- **确定性段**：词表、句式模板、密度等可以逐行判定、能写进黄金集的内容，才能成为规则。
- **语义段**：语义压缩、降低抽象程度、隐喻堆叠等只有模型能判断的内容，走 LLM 润色，产物为旁路文件或建议；人审后才能回写，不进 required check，也不把模型当作 CI 判定器。

### 五、规则分 fixable / non-fixable，non-fixable 报 warning

借鉴 ruff 的做法，规则分为 fixable 与 non-fixable 两档：现有 R1–R11 这类逐行且修复方式唯一的规则属于 fixable；从语言习惯中归纳出的规则 (术语选词、破折号过量等) 大多无法确定性修复，标为 non-fixable，只报 warning，不改文本。

这一档也衔接了两段式边界：warning 积累到一定量时，由 agent 主动调用 model-based 修复，而不是让 `--fix` 强行改写。分级如何写入 `spec/rules.md` 与输出格式，届时另起 ADR。

### 六、heuristic learning 路线：从润色 pair 蒸馏实验规则

积累语义润色前后的 pair (模型腔的原文 → 人话的改写)，从中蒸馏 experimental 的 rule-based lint，例如术语选词 (密钥 vs 秘密) 与破折号过量。这类规则先按上一条作为 non-fixable warning，默认关闭。

方法上参考翁家翌 (Jiayi Weng, `@Trinkle23897`) 在 *Learning Beyond Gradients* (2026-05-08) 中提出的 heuristic learning：学习的主体是显式程序代码，不是网络权重；闭环由 coding agent 直接修改规则、测试与配置完成 (`claudish-and-ai-slop-survey.md` §4)。本项目迁移的是它的运维模型，不是实验结果：

1. 规则写在 `spec/` 中，是人能读懂的显式软件；
2. 黄金 fixture 就是 golden trace，用于将已有能力固定为回归；
3. pair 只用于**提议**规则，人审后才能纳入规范；
4. 新规则必须跑全量黄金集，回归失败就修改或删除规则 —— 规则可读、可回归、可删除，是这条路线的前提。

不做的是「让模型自行生成不可读的检测器」，也不把 LLM 当作 AI 腔判定器放进 CI。

### 七、工程化留位：website、多语言文档、社区贡献

确认三项未来方向，本阶段只留位置，不实施：文档站点与多语言文档 (对标 AutoCorrect 的站点组织)、接受社区贡献所需的 `CONTRIBUTING` 与治理约定，以及随之而来的分发面。一切逐步推进：设计新内容时为它们留出空间，不提前搭建空壳。

## 后果

- 本 ADR 说明「这个项目是什么」；`README.md` 的「定位与愿景」是其摘要，两者不一致时以本文为准。
- 尚未实施的条目记在 `docs/tracker.md` 的「愿景」与「claudish 调研产出」两节；本 ADR 不新增 backlog，也不替代 tracker 记账。
- 不在此确定实施顺序。上述各项真正实施时另起 ADR：fixable / non-fixable 分级、experimental 规则的规范位置、LLM 润色原型的形式，都要分别定案后再修改 `spec/`。
- 可以照搬参考对象的机制，但不照搬默认值 —— 已有 R1–R11 的默认值不会因「上游怎么做」而变动；如需变更，必须有本仓自己的依据。

补记 (2026-09-12)：[PR #186](https://github.com/Luolc/limae/pull/186) 重写 README 后，上面第一条提到的「定位与愿景」小节已不存在。它被拆开了：项目做什么收进 README 开头的一句定位；「规则先于实现、多实现共用黄金 fixture」收进「规则」节末尾一句，并链接回 ADR-0001 / 0006 / 0007；其余各项 (agent-native、参考对象、两段式边界、heuristic learning) 回到本 ADR 与各自的 ADR，README 不再复述。**「以本文为准」这层关系没有变**，变的只是 README 一侧不再有对应的小节：本 ADR 仍说明「这个项目是什么」，README 中任何关于定位的表述若与本文不一致，以本文为准。

## 状态

accepted (2026-08-31)。

§五 (fixable / non-fixable 与 warning 绑定) 由 ADR-0006 取代 (2026-08-31)。
