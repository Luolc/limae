# Tracker

backlog 的正本，由 `mdlint-orchestra` 在合入后记账 (全局守则「多 agent 协作」)。规则语义的正本在 `spec/rules.md`，这里只记还没做的事与一句话去向；证据链在 `docs/research/`。

## 规则与配置

- **R5 年月日豁免 (P1)**：数字–中文空格加 `skipZhUnits` 类豁免 (年月日天号时分秒)，做成配置键、默认维持不豁免 (`zh-typography-guidelines-survey.md` §3.2)。
- **链接空格 flag**：已建成 R9、默认关 (调研 §3.10 自标争议)；留此备查，默认值等使用反馈再议。
- **`bracket_style = "contextual"`**：括号策略第三档「有中文全角、纯英文半角」；debate verdict 维持一律半角为默认，此档是对齐国标者的退路，落地时附最小规范 (调研 §3.4)。
- **`quote_style` 实现**：检测与转换，语义已在 `spec/rules.md`「规划中的键」定案，新规则 id 届时分配。
- **`quote_style` majority 档**：仿 pyink majority-quotes，按文档内多数引号风格统一，作 corner / curly 之外的第三档。
- **规则 fixable / non-fixable 分级**：non-fixable 规则当 warning 报告，学 ruff 的 fixable 标注。

## 愿景 (用户 2026-08-31 指示，只记条目)

- **LLM 语义润色**：agent 调用的语义层润色特性，与确定性 lint 互补。
- **heuristic learning**：从润色前后 pair 蒸馏 experimental 规则 (术语选词、破折号过量等)。
- **website 与多语言文档**：对标 AutoCorrect 的站点与多语言文档。
- **社区贡献机制**：CONTRIBUTING 等贡献流程。

## claudish 调研产出 (`docs/research/claudish-and-ai-slop-survey.md`)

- **experimental AI 腔 warning 蒸馏实验**：中英两套指纹分开，单点 tell 只做密度 warning。
- **术语选词小表试点**：AutoCorrect `wrong = right` 形制、分语境 (如 secret → 密钥)。
- **LLM 润色原型**：Deng spec 当 prompt、旁路文件、不进 CI。
