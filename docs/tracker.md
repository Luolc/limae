# Tracker

backlog 的正本，由 `mdlint-orchestra` 在合入后记账 (全局守则「多 agent 协作」)。规则语义的正本在 `spec/rules.md`，这里只记还没做的事与一句话去向；证据链在 `docs/research/`。

## 规则与配置

- **链接空格 flag**：已建成 R9、默认关 (调研 §3.10 自标争议)；留此备查，默认值等使用反馈再议。
- **`bracket_style = "contextual"`**：括号策略第三档「有中文全角、纯英文半角」，依据是 yikeke / FEX (调研 §3.4；国标的立场是一律全角，不是此档)。debate verdict 已裁决：默认维持 `half` (一律半角，即今天的 R2 + R3)；最小规范 —— 由一个策略规则处理**同一行内、非链接语法的配对括号** (`](` 照旧豁免)，括号内容剔除行内代码 span 后含 CJK 则用全角括号且外侧无空格、否则用半角括号且外侧一空格，嵌套括号从内向外独立判定，未配对或跨行的括号不报不修，R2 / R3 既有的 `disable` 语义保持兼容。
- **裸日文段落的语言探测 (P2)**：v0.3.1 只豁免了含假名的引用 span (`spec/rules.md`「全局豁免」第 4 条)，没有括号包裹的裸日文段落照旧按中文排版规则处理；要不要按行 / 按段探测日文并整体豁免，待有实际需求再定案。现象：不在「」『』《》内的裸日文专名 (如自造例 `サンプルIT推進部` 这种形态) 仍被 R4 报并插空格；消费方 wealth-management 2026-08-31 反馈，非阻塞，wm 暂以给专名补「」规避。单点逃生口已有：v0.5.0 的行内指令 `<!-- lo-md-lint-disable-next-line R4 -->` (`spec/rules.md`「行内指令」，黄金 case `inline-disable-next-line` 就带这个例子)。仍未定的候选修法 (wm 建议)：探测独立连续假名子串，跳过该子串邻接边界的 R4 (日文正字法本就不在 CJK–拉丁边界空格)；随其它规则改动一并走 spec 先行流程，不单独定案。
- **`quote_style` 实现**：检测与转换，语义已在 `spec/rules.md`「规划中的键」定案，新规则 id 届时分配。
- **`quote_style` majority 档**：仿 pyink majority-quotes，按文档内多数引号风格统一，作 corner / curly 之外的第三档。

## 实现与分发

- **Rust 主实现 (ADR-0002)**：主实现转 Rust，对着同一套 `spec/` 与黄金集跑，Python 版留作参考实现；crate 布局与分发形态 (多语言 SDK、LSP、编辑器与 CI 集成，对标 AutoCorrect) 届时另起 ADR。

## 愿景 (正本 `docs/adr/0005-agent-native-positioning.md`，这里只记条目)

- **LLM 语义润色**：agent 调用的语义层润色特性，与确定性 lint 互补。
- **heuristic learning**：从润色前后 pair 蒸馏 experimental 规则 (术语选词、破折号过量等)。
- **website 与多语言文档**：对标 AutoCorrect 的站点与多语言文档。
- **社区贡献机制**：CONTRIBUTING 等贡献流程。

## claudish 调研产出 (`docs/research/claudish-and-ai-slop-survey.md`)

- **文档级密度规则**：破折号 / 粗体 / 列表化行文的密度判定是文档级的，`.findings` 的「行号 + 规则 id」形制装不下；要先给规范加文档级 finding 的形制，再收这批 (ADR-0007 §三)。
- **英文 tells 词表**：English-to-English 的实验规则 (`load-bearing` 过量、否定平行的英文形态等)，沿用 A 家族与 `spec/wordlists/` 形制；规则不分语言，出现在哪管到哪 (ADR-0006 §五)。
- **LLM 润色原型**：Deng spec 当 prompt、旁路文件、不进 CI。
- **A8 补「零 + 拉丁 / 混合名词」**：现判定只取「零」右侧的连续汉字串，漏掉「零 SA 需要读它」「零 service account 需要读它」这类「零 + 拉丁或中英混合名词 + 谓语」的形态 (`machine-setup` 2026-08-31 用 v0.9.0 跑改写前语料时发现，当次靠人工改写)。匹配单位要扩到拉丁词与混合串，边界与白名单语义随之定案。
- **实验规则的 per-file 豁免写进规则文档**：各仓的 tracker / journal 这类历史账按约定不改写，A8 / T2 将来若转 stable，这些文件需要整份跳过。`.lo-md-lint-ignore` (v0.5.0) 已经能做，缺的是在规则文档里写明这条建议做法。
