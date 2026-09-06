# Tracker

backlog 的正本，由 `limae-orchestra` 在合入后记账 (全局守则「多 agent 协作」)。规则语义的正本在 `spec/rules.md`，这里只记还没做的事与一句话去向；证据链在 `docs/research/`。

## 规则与配置

- **链接空格 flag**：已建成 zh-typography-9、默认关 (调研 §3.10 自标争议)；留此备查，默认值等使用反馈再议。
- **`bracket_style = "contextual"`**：括号策略第三档「有中文全角、纯英文半角」，依据是 yikeke / FEX (调研 §3.4；国标的立场是一律全角，不是此档)。debate verdict 已裁决：默认维持 `half` (一律半角，即今天的 zh-typography-2 + zh-typography-3)；最小规范 —— 由一个策略规则处理**同一行内、非链接语法的配对括号** (`](` 照旧豁免)，括号内容剔除行内代码 span 后含 CJK 则用全角括号且外侧无空格、否则用半角括号且外侧一空格，嵌套括号从内向外独立判定，未配对或跨行的括号不报不修，zh-typography-2 / zh-typography-3 既有的 `disable` 语义保持兼容。
- **裸日文段落的语言探测 (P2)**：v0.3.1 只豁免了含假名的引用 span (`spec/rules.md`「全局豁免」第 4 条)，没有括号包裹的裸日文段落照旧按中文排版规则处理；要不要按行 / 按段探测日文并整体豁免，待有实际需求再定案。现象：不在「」『』《》内的裸日文专名 (如自造例 `サンプルIT推進部` 这种形态) 仍被 zh-typography-4 报并插空格；消费方 wealth-management 2026-08-31 反馈，非阻塞，wm 暂以给专名补「」规避。单点逃生口已有：v0.5.0 的行内指令 `<!-- limae-disable-next-line zh-typography-4 -->` (`spec/rules.md`「行内指令」，黄金 case `inline-disable-next-line` 就带这个例子)。仍未定的候选修法 (wm 建议)：探测独立连续假名子串，跳过该子串邻接边界的 zh-typography-4 (日文正字法本就不在 CJK–拉丁边界空格)；随其它规则改动一并走 spec 先行流程，不单独定案。
- **`quote_style` 实现**：检测与转换，语义已在 `spec/rules.md`「规划中的键」定案，新规则 id 届时分配。
- **`quote_style` majority 档**：仿 pyink majority-quotes，按文档内多数引号风格统一，作 corner / curly 之外的第三档。
- **仓内自用的词撞上词典自己的收词**：词典把「正本」收作要避免的 AI 中文 (`spec/lexicon/zh.toml`)，而本仓自己的 Markdown 里有 38 处在用它 (2026-09-03 数，散在 35 行上；`README.md`、`spec/README.md`、`spec/rules.md`、`docs/tracker.md`、多份 ADR 与调研)，包括 tracker 开头那句「backlog 的正本」。计数只算 Markdown —— 词典条目自身、`site/index.html` 这份生成产物、`spec/wordlists/` 的注释与 fixture 不在内。这是 dogfooding 上的自相矛盾：本仓用自己的 linter 检查自己的 Markdown，词典却没有对应的可执行规则来暴露它。两条路 —— 把词典条目做成 `zh-word` 家族的规则让它真能报，或者认定这个词在技术语境下可用、从词典里撤掉。**先定哪一条，再动那 38 处**，不要反过来。

## 实现与分发

- **Rust 全量迁移**：[ADR-0015](adr/0015-rust-migration.md) 经 PR #66 合入，仓级约定 (G) 经 PR #67 合入；A1 文本基础与 Rust CI (#69)、A2 Markdown 保护范围 (#73)、A5 配置解析 (#75) 已完成，A2 前的引用配对基线修正见 #71。A3a 宽度规则 (zh-typography-1 / 2 / 10，#77) 与 A3b 正文间距 (3 / 4 / 5 / 6，#79) 已完成行检查与正文片段修复；规则 11 的非重叠计数、同起点报告顺序与合成样例已在 #81 补齐；A3c 结构间距原语 (#83) 与纯排版文档编排 (#85) 已完成；生产 API 的适用排版黄金样例完整通过，实验规则与行内指令分别归 A4 / A6。A4 首批五条词表规则与构建嵌入 (#87) 已完成，英文大小写与边界合同先由 #88 独立澄清；句式与造词原语 (#90) 已完成；术语检查与片段修复 (#92) 已完成；文档实验规则编排与直接相关测试适配清理 (#94) 已完成；下一项 A6 行内指令，承接边界见 #94 PR 描述。按 ADR 的 A–E 自动推进、agent 互审，直到正式 `limae` 为 Rust、Python deprecated 且参考实现与测试保留；实际 tag / release / publish 前停止。进度仅在此记，任务依赖与验收合同见 ADR。
- **A4 / A6 黄金集交叉依赖**：`experimental-en-ai-tells`、`experimental-zh-ai-tells`、`experimental-zh-secret`、`experimental-zh-zero-noun` 各有一条生效的行内禁用指令，A4 文档集成时须保留这四组的 A6 依赖，不能过滤被指令影响的 finding 后声称完整通过；`experimental-english-unicode` 没有这项依赖；A6 后与全部黄金集一起验收 (见 [PR #91 事实复核](https://github.com/Luolc/limae/pull/91#issuecomment-5559435631))。
- **单轮术语阶段顺序判据**：按 [PR #94 合并决定](https://github.com/Luolc/limae/pull/94#issuecomment-5559756587)，A6 触及 pipeline 时将已验证的合成词条单轮 `fix_line` 测试落入仓内，验证术语置后变体会红；最终不动点输出不能代替单轮阶段顺序的判据。原测试内重复编排已随 #94 清理。
- **A8b 配置错误差分**：按 [PR #75 的协调决定](https://github.com/Luolc/limae/pull/75#issuecomment-5558344043)，复用既有手写消息子串，比较退出码、错误类别、来源路径与配置键；自有校验错误不要求全文逐字相同，因为 Rust 不回显未校验的配置值。须在成对错误用例中体现这项差异，不能删除整条 stderr；正常 finding、warning 与成功输出仍按 ADR 的完整差分合同。
- **Rust 迁移收尾补项**：E 的默认 pre-commit id 切换必须包含 `.pre-commit-hooks.yaml` 的 `language` / `entry` 与全新消费仓安装验收 (PR #66 复审 P2)。orchestra 按用户「当前 Python 全部功能迁移」的范围对账 (2026-09-06)，补入现有 `tools/render_lexicon.py` 的 HTML 生成功能：迁为 Rust 开发工具，以同源生成产物字节一致验收；Python 脚本保留作参考，不增加 rubrics 等新输出。
- **后续分发形态**：多语言 SDK、LSP、编辑器与 CI 集成，对标 AutoCorrect；Rust 迁移完成后另起 ADR，不属于本轮 A–E。
- **Rust 发版准备** (2026-09-06)：许可证尚未选定；[维护者的开发机发布手册 (私有仓)](https://github.com/Luolc/machine-setup/blob/main/docs/knowledge/crates-io-publishing.md) 已经 machine-setup #194 合入，认证仍待用户创建 scoped token，首发后配置 GitHub Actions Trusted Publishing。首发前只能验引用可解析且值非空，不能据此声称 token 有效；操作与权限配置以手册为准，不在此重复。该待办不阻塞代码迁移与无需认证的发布预演，实际发布仍在本轮授权停止点之外。

## 愿景 (正本 `docs/adr/0005-agent-native-positioning.md`，这里只记条目)

- **LLM 语义润色**：agent 调用的语义层润色特性，与确定性 lint 互补。
- **heuristic learning**：从润色前后 pair 蒸馏 experimental 规则 (术语选词、破折号过量等)。
- **website 与多语言文档**：对标 AutoCorrect 的站点与多语言文档。
- **社区贡献机制**：CONTRIBUTING 等贡献流程。

## claudish 调研产出 (`docs/research/claudish-and-ai-slop-survey.md`)

- **文档级密度规则**：破折号 / 粗体 / 列表化行文的密度判定是文档级的，`.findings` 的「行号 + 规则 id」形制装不下；要先给规范加文档级 finding 的形制，再收这批 (ADR-0007 §三)。
- **英文 tells 词表**：English-to-English 的实验规则 (`load-bearing` 过量、否定平行的英文形态等)，沿用 tell 家族 (`zh-tell` / `en-tell`) 与 `spec/wordlists/` 形制；规则不分语言，出现在哪管到哪 (ADR-0006 §五)。
- **A/B 单侧失败时降级**：两个候选只回来一个时，现在整轮不显示；应改成降级成单路并把失败记进诊断，而不是整轮消失。根在 `ab.run` 要改成逐候选返回。A/B 关着时不发作。
- **型号准入清单**：`ab.py` 里一份可执行的合格型号表，默认空；证据在 `docs/research/polish-engine-cli-behavior.md`。判据必须是该文档 §六 第 2 步的实测，**不能是裸 PONG** —— haiku 与 sonnet 裸 PONG 都通过，而它们正是 #46 里把正文当对话的那两个 (ADR-0008 §四)。
- **润色的方差**：同一段输入连跑四次，输出从「逐字完全相同、一字未改」跨到「删光全部行内强调并引入一个 `zh-typography-1`」(2026-09-01 实测)。两端都不是润色。`spec/polish/general.md` 的「If the text needs no change, output it unchanged」给了「什么都不做」一条合法出路。先按 ADR-0012 的单路记录攒真实分布，再决定改法 —— 单次采样证明不了任何事。
- **ADR-0008 §七 的澄清缺指向**：§七「不做旁路目录」被 ADR-0009 §五 澄清过，但 `0008-limae-polish-cli.md` 状态节没有指向；§十 已由 ADR-0012 补上同类的一句。将来有任务碰这份 ADR 时顺手补齐，不单独开 PR。
- **`polish` prompt 的 evolve**：两份 spec 已落地 (PR #37)，但还是起步版；按 ADR-0008 §九 走 alpha-evolve (演化式迭代)，靠 P0 那个 hook 的反馈驱动，并带上 §五 记的那条社区证据对 prompt 措辞的影响。在方差那条解决之前谈不上调「够不够狠 / 像不像作者 / 有没有改错地方」 —— 那三种病要在输出稳定之后才分得出来。
- **引擎实测随版本复核**：`docs/research/polish-engine-cli-behavior.md` 记的三家 CLI (command-line interface) 实测行为带实测日期，CLI 升级或换型号时按该文档的人工验收步骤重跑，通过才进预设表 (ADR-0008 §四)。
- **结构不变量保护器**：改写前后逐字比对围栏、行内代码、链接目标与锚点、标题行、表格结构，变了就判不合格；另配语义层的模型裁判 smoke test，永不进 CI (ADR-0008 §八)。§八 写的是「**至少**要包括」，所以还要补上**行内强调标记** —— 实测模型会把整段 `**` 吃掉，而清单里今天没有这一项，也就是说它并未违规。同时把这一级从 §十 排的 P1 提前到 hook，理由是 hook 才是每条消息都在走的活路径。另有旁证说明清单本身不够：#46 里 haiku 删掉一整段，违反的是**已经写在清单里**的块结构那条 —— 请模型别删要做，我们自己数才是兜底。
- **`polish` 默认模型由 A/B 决定**：ADR-0008 §五只记候选与判据，暂定 terra / sonnet / grok-4.6；用自家语料做 10–20 条盲对照后再定，降到便宜档必须有自家证据。
- **`limae` 三个子命令的实现**：`check` / `format` / `polish` 的命令行分层 (ADR-0008 §二)，今天的 `limae [--fix]` 在过渡期继续可用。
- **P1 `polish` 的文件形态**：单文件改写，以及按 git 变更集 (dirty 或最近一个 commit 碰过的 Markdown) 批量；P2 再做跨文件协调改写 (ADR-0008 §十)。
- **zh-tell-5 补「零 + 拉丁 / 混合名词」**：现判定只取「零」右侧的连续汉字串，漏掉「零 SA 需要读它」「零 service account 需要读它」这类「零 + 拉丁或中英混合名词 + 谓语」的形态 (`machine-setup` 2026-08-31 用 v0.9.0 跑改写前语料时发现，当次靠人工改写)。匹配单位要扩到拉丁词与混合串，边界与白名单语义随之定案。
- **实验规则的 per-file 豁免写进规则文档**：各仓的 tracker / journal 这类历史账按约定不改写，zh-tell-5 / zh-word-2 将来若转 stable，这些文件需要整份跳过。`.limae-ignore` (v0.5.0) 已经能做，缺的是在规则文档里写明这条建议做法。
