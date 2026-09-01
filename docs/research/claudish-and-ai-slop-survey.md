# 调研报告：Claudish 语言现象与去 AI 腔工具

> 来源：Grok research agent 调研，2026-08-31；distill 决策另行进 spec / ADR，本文只是证据。

- **任务**：`research-claudish` (Grok，只读)
- **获取日期**：2026-08-31
- **范围**：公开网页、GitHub、X；不改仓库
- **本报告约定**：带 URL 的段落是可核验事实；「判断」小节才是调研方观点。专有名词首次中英对照。

---

## 0. 先说结论 (判断)

用户记忆里「X 上很火的 Claudish to English」不是一个项目，是**两条相邻产品线**：

1. 先出现、工程上更完整的，是 Mike Gvozdev (`gvzdv`) 的 Claude Code 插件 `claudish-to-english`：用本地 / 云端 LLM **显示层改写**助手回复，不改 transcript。
2. 后来引爆 X 的，是 Yuntian Deng (`@yuntiandeng`) 的双向翻译器 ProgramAsWeights `claudish`：把「Claudish」当成一种可正反翻译的方言，并公开了**目前最完整的 philosophy 规范**。

对 `lo-md-lint` 更有用的不是插件本身 (它是 LLM 润色，不是 lint)，而是 Deng 的 spec + 词典、维基百科的 AI 文特征、以及 Vale / VS Code 这类 **确定性探测器**。AI 腔能进 experimental warning 的，几乎都是「词表 + 句式模板 + 密度」；隐喻堆叠、语义压缩这类 Claudish 哲学，只能当 LLM 润色的 prompt，不能当黄金集规则。

---

## 1. 「Claudish to English」：确切名字、作者、热度、原理

### 1.1 两条产品线 (事实)

| 项 | A. Claude Code 插件 | B. 双向翻译器 (X 爆款) |
| --- | --- | --- |
| 确切名字 | `claudish-to-english` | `Claudish` / English ↔ Claudish |
| 作者 | Mike Gvozdev (`gvzdv`) | Yuntian Deng (`@yuntiandeng`)，滑铁卢大学助理教授，ProgramAsWeights |
| 仓库 | https://github.com/gvzdv/claudish-to-english | https://github.com/programasweights/claudish |
| 网站 / demo | Claude Code 插件；Marketplace 自 2026-08-10 起 `Submitted and pending review` | https://programasweights.com/claudish |
| 热度 (2026-08-31 页面) | GitHub **2.4k** stars / 111 forks | GitHub **248** stars / 20 forks；引爆帖 2026-08-22，**16,796 likes / 1,183 reposts / 1,127,683 views** |
| 许可 | MIT | 未在 README 强调官方 affiliation；自称 unofficial parody，inspired by A |
| 初版 | CHANGELOG `0.1.0` = 2026-08-10 | README 写明 Inspired by `gvzdv/claudish-to-english` |

引爆帖原文：

> Claude has become a language, so I built a translator. English <-> Claudish

来源：https://x.com/yuntiandeng/status/2091201867737145472 (2026-08-22)

二次传播例：Vaibhav Sisinty 2026-08-13 介绍插件 (473 likes)；AI Edge 2026-08-27 把 Deng 的翻译器写成 “Redditor built a Claudish translator”。说明社区把「插件」和「双向翻译器」混称同一件事。

### 1.2 不要混淆的同名 (事实)

- **MadAppGang/claudish**：Claude Code 的多模型代理 (proxy)，让 Claude Code 跑 GPT / Gemini / Grok。名字碰巧叫 Claudish (Claude-ish)，**与文风翻译无关**。https://github.com/MadAppGang/claudish
- **leoui/Claudish**：claude.ai 的 Chrome 主题扩展，视觉向。
- **will-ness-ai/claudish-to-english**：Gvozdev 插件的 fork，给 rewrite prompt 加会话上下文与项目词表。

### 1.3 原理 (事实)

**A. 插件 (`gvzdv`)** 是 hook + LLM，不是规则引擎。

- 事件：`MessageDisplay` (显示层) + 可选 `PostToolUse` (Markdown 文件，默认关)。
- 默认 rewriter：本机 ollama (默认模型 `gemma4:26b-mlx`)；也可 `codex` CLI、Anthropic API、任意 OpenAI-compatible。
- 合同：**display-only、fail-open**。推理与 transcript 保留原文；provider 挂了就显示原文。
- 内置 system prompt 很短，核心一句：把助手消息改成更简单的 plain language，保留事实 / 数字 / 路径，代码围栏不动，只输出改写。另有 style preset：`tldr` / `5y` / `caveman`。
- Markdown hook 才碰磁盘：按目录 opt-in，默认写 `NAME.plain.md` 旁路文件。

源码默认 prompt (https://github.com/gvzdv/claudish-to-english/blob/main/rewrite.sh，2026-08-31)：

> You rewrite the assistant's message into much simpler, plain language. Write the rewrite in the same language as the message you are rewriting. Keep every fact, name, number, and file path. Use short sentences and everyday words. Leave fenced code blocks unchanged. Output ONLY the rewritten message with no preamble, labels, or commentary.

**B. 翻译器 (`programasweights`)** 是两份可复制 spec + 两个 ProgramAsWeights function id。

- Python：`to_claudish = paw.function("ca9d5165b6c8e6615529")`，`to_english = paw.function("e469f61ccab2699fbd51")`
- 程序下载后本地跑；live demo 在网站。
- 规范正本：
  - https://github.com/programasweights/claudish/blob/main/specs/claudish-to-english.md
  - https://github.com/programasweights/claudish/blob/main/specs/english-to-claudish.md
- 词典：https://github.com/programasweights/claudish/tree/main/dictionary (`entries.json` + https://programasweights.com/claudish/dictionary)

另有开权适配器尝试：Hugging Face `adamrotmil/claudish-style-adapter` (作者 X 自称 2026-08-25 WIP，把 Claudish 压成可在 CPU 上跑的权重)。未核其质量。

### 1.4 Philosophy：什么算 Claudish (事实，摘自 Deng spec)

Deng 给的定义 (claudish-to-english.md)：

> “Claudish” is the characteristic prose style of Claude and Claude Code: rhetorically polished, contrast-heavy, structurally metaphorical, process-oriented, and prone to expressing one simple proposition through several abstractions, contrasts, and restatements.

改写目标不是「换几个词」，而是恢复 **smallest set of ordinary propositions**。下面是 spec 里明确点名、可当判据清单的条目。

**要删掉的修辞 (Remove rather than paraphrase)**：

- 对比框架：`not X but Y` / `X, not Y` / `less X than Y`，或先否定一种 framing 再给「正确」framing
- 舞台强调：`the key distinction` / `the deeper point` / `the honest take` / `the cleanest way to see this` / `the load-bearing constraint` / `the verdict here` / `the smoking gun`
- 冗余定向：`in one sentence` / `put differently` / `in other words` 以及反复总结
- 格言收尾：`that distinction matters` / `that is the boundary` / `that is the actual constraint`
- 坦诚表演：`you’re absolutely right` / `fair hit` / `one honest caveat` / `the honest answer` (人际含义本身重要时除外)
- 同义反复：同一命题换词再说一遍

**要解码的结构 / 过程隐喻**：

| Claudish | 普通英语 (spec 给的方向) |
| --- | --- |
| `X-gated` / `gated on X` | X is required / must happen first |
| `owner-gated` | only owners may do it |
| `approval-gated` | approval is required |
| `hard gate` / `hard boundary` / `hard stop` | strict requirement / blocker |
| `load-bearing` | essential / necessary / central |
| `surface` | the actual object / interface / issue |
| `path` | the action / option / process |
| `layer` | the component |
| `handoff` | transfer / transition |
| `spine` | main structure |
| `landed` | merged / completed / deployed (看上下文) |
| `surfaced` | appeared / was found / was reported |
| `stale` | outdated |
| `verified` / `audited` | tested / checked / confirmed |
| `canonical` | official / preferred |
| `blocker` | something preventing progress |
| `drift` | change or divergence over time |

**要拆开的连字符压缩**：`X-gated` / `X-backed` / `X-side` / `X-level` / `X-first` / `X-safe` / `X-matched` / `X-layer` / `X-surface` / `X-path` / `X-boundary`。例：`approval-gated release path` → `release requires approval`。

**要降调的研究腔词** (修辞用法，不是真技术词时)：`frontier` / `horizon` / `floor` / `surface` / `exchange rate` / `regime` / `trajectory` / `slice` / `cell` / `matched` / `frozen` / `headline` / `confirmatory` / `protocol` / `claim gate` / `lower bound` / `clears` / `survives` / `implicates`。

**明确不是禁词**：`provenance` / `lineage` / `calibration` / `routing` / `boundary` / `gate` / `surface` / `protocol` / `verified` / `canonical` / `drift` 在它们确实是最清楚的技术描述时要保留。spec 反复强调：**不要机械换词典**。

**保真硬约束** (对 lint / 润色都关键)：

- 改写必须是 paraphrase，不是对输入的「回答」
- 不添加新事实、解释、建议、因果、排他规则
- 逻辑外延不能变宽：`Do X if Y` 不等于 Y 是唯一触发；`required` 不等于 `sufficient`；`not tested` 不等于 `incorrect`
- 隐喻含糊时取**最窄**、上下文直接支持的解释

反向 spec (`english-to-claudish.md`) 把 Claudish 写成可注入的倾向，并点名 high-signal 词：`load-bearing, spine, shape, grain, verdict, audited, gating, quality-gated, owner-gated, approval-gated, cleanly, hard gate, hard constraint, hard boundary, hard stop, routing, routing layer, context router`。

词典 `entries.json` 把词分成 word / phrase / reassurance / construction 四类，并给了组合规则：`X-shaped`、metaphor stacking、`not X but Y`。推荐 slug 包括 `gated-on`、`honest-shape`、`belt-and-suspenders`、`blast-radius`、`fail-closed`、`youre-right-to-push-back` 等。定量来源之一是 archiewood/claudeisms (单用户 175 份 Claude Code transcript vs 前 LLM 时代 Stack Overflow 评论)：`load-bearing` 相对频率 >7,500×，作者注明该词出现在 Claude Code system prompt (`give brief updates when you find something load-bearing`)。https://github.com/archiewood/claudeisms

独立语料：louisabraham/load-bearing 对 GitHub PR 做词簇分析 (页面称 595 天、约 46 万 PR)。其中一个 2026 年冒出的簇，到 2026 年中约占「看起来像人写的」PR 的 39–40%，代表词就是 `load-bearing`。https://louisabraham.github.io/load-bearing/ ；中文报道 https://gigazine.net/news/20260831-load-bearing-ai-vocabulary-github/ (2026-08-30)。

### 1.5 这一节的判断

- 用户说的「最近很火」对得上 **Deng 的 X 帖 + Gvozdev 插件**，名字记忆准确；「de-Claude / AI-slop remover」是同一生态里的兄弟项目，不是这个爆款的本名。
- **Philosophy 清单在 Deng spec，不在插件默认 prompt。** 插件默认只要求「更短、更日常」；要蒸馏 experimental lint，应引用 spec / dictionary，而不是 hook 脚本。
- 插件的 Markdown hook 是「LLM 润色 CLI」的最近邻，但改的是显示或旁路文件，不是 findings 流。

---

## 2. Claudish / AI 腔特征清单

社区其实有两套重叠但不相同的清单：一套是 **Claude 编程助手方言 (Claudish)**，一套是 **通用 LLM 网文 / 百科腔 (AI slop)**。前者对 coding agent 文档更尖；后者对中文公众号 / README 更尖。

### 2.1 英文 (事实，按出处)

**A. Wikipedia: Signs of AI writing** (WikiProject AI Cleanup，描述性 field guide，不是方针)

https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing (2026-08-31 仍标注 August 2026 需更新)

内容层：

- 重要性通胀：`stands/serves as`、`is a testament`、`pivotal/vital/key role`、`underscores/highlights`、`reflects broader`、`indelible mark`、`evolving landscape`
- 罐头关注度：`independent coverage`、`profiled in`、`active social media presence`
- 表层分析：句尾现在分词 `highlighting/underscoring/emphasizing/ensuring/reflecting/symbolizing/contributing to`
- 广告腔：`boasts a`、`vibrant`、`nestled`、`in the heart of`、`groundbreaking`、`diverse array`
- 模糊归属：`experts say` / `observers note` 一类 weasel

语言 / 结构层 (该页后半与大量二次整理一致)：

- 否定平行：`it's not X, it's Y` / `not just X, but Y`
- 三段式 (rule of three)
- em dash 过密、粗体过密、emoji 列表、碎片标题
- AI 词汇：`delve`、`tapestry`、`leverage`、`realm`、`robust`、`seamless`、`underscore`、`foster`、`harness`、`unpack`

维基自己的 caveat (事实，不是 pedantry)：单点 tell **不是检测器**；人类识别 LLM 文本接近随机；em dash / delve 都有人类先例。Kobak 等对 PubMed 的测量：`delves` 在 2024 摘要里相对 2022 暴涨约 28 倍 (社区常引)。Washington Post 2025-04-09 专门写过「em dash = ChatGPT hyphen」是神话。The Economist 2026 年一组比较甚至发现：多数模型的 em dash 已不再高于人类，**缺标点**反而更像模型 —— 例外是 Claude 仍偏爱 em dash。

**B. Claudish 专用 (Deng spec + dictionary + claudeisms)**：见 §1.4。与通用 slop 的差别是：少 `delve/tapestry`，多 `load-bearing/gated/spine/shape/landed/drift/verdict`。这是 coding-agent 文档的指纹，不是 ChatGPT 博客的指纹。

**C. 可执行规则包** (把维基观察编成 YAML / skill)：

| 项目 | 形式 | 规模 | 备注 |
| --- | --- | --- | --- |
| krishnasunkam/vale-ai-tells | Vale style-package，**真 lint** | Dash / EpigramContrast / NegParallel / CopulaInflation 等 | 「rule floor, not a detector」；不改写，只标 |
| mandakan/llm-slop-detector | VS Code 扩展 | 核心 ~40 条 + 可选 pack (含 `claudeisms`) 共 500+ regex | 字符类有 quick fix；短语**故意无自动替换** |
| hardikpandya/stop-slop | Agent skill | ~30 禁词 + 8 结构 | 2026-07 调查称 14k+ stars 但已停更 134 天 (upkeep F) |
| blader/humanizer | Agent skill | 33 模式，源自维基 | 调查时约 32k stars，生态默认选择 |
| woerndl/unsloppify | Skill + 常驻 baseline | 六种 failure-mode + 词表扫描 | 明确拿 Claudish 当反面教材 (README 副标题就是 load-bearing 玩笑) |
| adamdunkels/deslop-text | Skill | 32 checks；W2 = `Not X, it is Y` | 浏览器 demo https://adamdunkels.github.io/is-it-slop/ |
| stephenturner/skill-deslop | Skill | ~30 tropes + 50 分量表 | 科学写作加权 |
| isatimur/de-slop | Skill | fidelity-first：空心跨度只标不编 | 维护分在调查里高于明星项目 |

2026-07-30 的生态普查：https://github.com/Daguilar0123/ai-writing-skill-field-guide (68 个 skill，star 与维护无相关)。判断：star 不能当选型依据。

**D. 量化研究 (事实)**

- Samuel Bestvater 等对近 50 万英语网页：ChatGPT 后 em dash 约翻倍，`delve` / `testament` / `interplay` 约翻倍，negative parallelism 近三倍 (SFGate 2026-08-27 转述)。
- 广告文案 playbook (The Adpharm, Claude Opus 4.7)：若只修三件事，修 negative-parallelism + tricolon + em dash，「大约 70% 的一眼可见 tell」。https://www.theadpharm.com/insights/claude-opus-anti-slop-playbook

### 2.2 中文 (事实，按出处)

**A. 中文维基 `《AI生成文的特征》`** (知识源，持续更新)

https://zh.wikipedia.org/zh-cn/Wikipedia:AI生成文的特征 (快捷 WP:AISIGNS)

与英维平行，并点名中文词：

> 稳 / 稳稳地、接住、作为 / 服务于、见证 / 提醒、至关重要 / 意义重大 / 关键时刻 / 转折点、强调 / 突出其重要性、反映出更广泛的……、标志着其持续 / 持久地……、做出贡献、奠定基础、不断变化的格局、聚焦点、不可磨灭、根植于……

结构：否定平行「这不是……而是……」(引朱宥勋)；排比三段式；粗体轰炸；列表式行文 (marker + 粗体 + 引号)；emoji；破折号过密；表格误用；编号切段；聊天残留 (「希望这对你有帮助」「当然可以」)；知识截止免责；Markdown 当 wikitext 粘进来；`turn0search0`、`utm_source=chatgpt.com`。

朱宥勋视频 (清单常引)：https://www.youtube.com/watch?v=9uuX6cb81C8 `「对『AI腔』厌烦了吗？分析AI生成文字的经典句型」`。论点：该句型本身是合法的「定义 + 区分」，问题是滥用造成审美疲劳。

**B. 中文 skill / 词表 (可抄组织方式)**

| 项目 | 覆盖 | URL |
| --- | --- | --- |
| op7418/Humanizer-zh | 中文移植 blader/humanizer；页面约 **16.4k** stars，但 2026-07 普查标为停更 fork | https://github.com/op7418/Humanizer-zh |
| Raymondhou0917/speak-human-tw | 35+ 繁中痕迹，知识源=中文维基 + 朱宥勋 | https://github.com/Raymondhou0917/speak-human-tw |
| acchuang/zh-tw-humanizer | 50 模式 + 48 组两岸用词 | https://github.com/acchuang/zh-tw-humanizer |
| 0xtresser/cn-humanizer | 18 模式 + 100+ 高频词 + 12 条翻译腔 | https://github.com/0xtresser/cn-humanizer |
| ninehills/public-skills `deslop-zh` | 黑话表：赋能 / 抓手 / 闭环 / 沉淀 / 对齐 / 拉通… | https://github.com/ninehills/public-skills |
| VincentOld/stop-slop-zh `phrases.md` | 开场套话、八股连接词、强调虚壳、互联网黑话 | https://github.com/VincentOld/stop-slop-zh |
| yanyintingyou/stop-slop-cn-en | 中英双语；「赋能、助力、生态、闭环、高质量发展、未来可期」 | https://github.com/yanyintingyou/stop-slop-cn-en |
| AAzzAAzzAAzzAA/remove-chinese-ai-tics | 审计 / 保守清理 / 标准改写分模式；保事实 | https://github.com/AAzzAAzzAAzzAA/remove-chinese-ai-tics |
| LifelongLazyLearner/qu-ai-wei | 51 条，含翻译腔专节 | 普查里简体中文维护分较高 |

**C. 中文特有、英文清单没有的层**

1. **翻译腔 (translationese)**：知识份子 2026 文把 AI 中文的「像翻译」解释为默认语言是英语分析性写作 (Baker 的 translationese)。例：`He gave me a smile` → 「他给了我一个微笑」，人话是「他朝我笑了笑」。https://www.zhishifenzi.com/depth/depth/10608.html
2. **直译术语**：掘金 2026-07-07「那些看不懂的 AI 味词语」 —— `gate` → 「门控」(人话：把关)；`cross-cutting` → 「横切」；`first-class citizen` → 「一等公民」；`contract` → 「契约」(人话：约定 / 接口约定)。https://juejin.cn/post/7659706081527169075
3. **公文 / 互联网黑话**：赋能、抓手、闭环、沉淀、对齐、拉通、赛道、心智、高质量发展、未来可期、拭目以待。
4. **接住体**：爱范儿 2026-05-08 记录 ChatGPT 中文口癖「稳稳地接住你」，推测是 `I got you` 的戏剧化直译。DeepSeek / 豆包有各自口癖 (豆包「最直接、最真相、最不绕弯…」排比)。https://www.ifanr.com/1665148
5. **工程师腔**：说人话 skill 把「抽象底层接口，实现高内聚低耦合」列为技术文档 AI 味。https://www.xmsumi.com/detail/3011

### 2.3 「正在飞」类怪话 (事实 + 缺口)

**公开清单里没有以「正在飞」为条目的稳定出处。** 2026-08-31 检索 (网页 + 中文维基特征页 + 主流中文 skill) 未命中该短语作为 AI 腔标签。

最近似、有出处的同类：

- 「稳稳接住」：翻译腔把松弛英语口语戏剧化 (爱范儿；中文维基词表含「稳 / 接住」)
- 进行时直译：英语 present continuous / `in flight` / `it's flying` (进展顺利) 译成「正在飞」 —— 这是翻译腔机制上说得通的假说，**不是已发表清单**
- 直译术语族：门控 / 横切 / 一等公民 (掘金)

### 2.4 这一节的判断

对 `lo-md-lint` 要把英文 Claudish 和中文 AI 腔当成 **两套规则包**：共享「否定平行、三段式、破折号密度、粗体密度」，其余词表几乎不重叠。`delve` 在中文文档里不是问题；`load-bearing` 的中文直译「承重 / 负载」和「赋能」也不是同一类错。

单点词 (尤其 em dash、delve、破折号) 只适合 **warning + 密度阈值**，不适合 error。维基、Economist、华盛顿邮报都在说：单独一条 tell 的假阳性很高。

---

## 3. 同类工具与先例

### 3.1 去 AI 腔 / 人化 (开源优先，事实)

三类，不要混：

1. **Prompt skill (改写)**：humanizer / stop-slop / deslop / Humanizer-zh / 说人话。装进 agent，生成时或事后改写。不是 linter，没有稳定 findings 格式，也没有黄金集。
2. **确定性探测器 (lint)**：
   - Vale `vale-ai-tells`：prose linter 风格包，标 tell 不改写。https://github.com/krishnasunkam/vale-ai-tells
   - `llm-slop-detector`：编辑器诊断 + 字符级 quick fix，短语无 fix。https://github.com/mandakan/llm-slop-detector
   - AutoCorrect (`huacnlee/autocorrect`)：CJK 空格 / 标点 / 可选 spellcheck 词表，**与 lo-md-lint 的 zh-typography-1 到 zh-typography-3 最近邻**。https://github.com/huacnlee/autocorrect (~1.6k stars)
3. **LLM 显示层改写**：`claudish-to-english` 插件 (见 §1)。

商业「humanizer / undetectable.ai」一类不列入：目标是骗检测器，和文档质量相反。

### 3.2 LLM 文档校对 / 润色 CLI，以及和 lint 的组合 (事实)

- **Vale + markdownlint**：Earthly 等把 prose lint 和 Markdown 结构 lint 串在 CI。Vale 吃 YAML 风格包，是「文档 lint」的行业默认。https://earthly.dev/blog/markdown-lint/
- **Vale 用于 agent 上下文文件**：Amanda Martin, DEV 2026-02-23，用 Vale `existence` 规则抓 `follow best practices` 这类空指令。https://dev.to/amandamartindev/practical-linting-for-agent-context-files-322h
- **`@promptier/lint`**：prompt 的启发式 lint，可再开 Ollama 做 semantic lint (矛盾、含糊、啰嗦)。https://www.npmjs.com/package/@promptier/lint
- **agent-md**：为「给 LLM 读的 Markdown」做结构 lint (缩进、ASCII 图)。方向相反：优化机器可读，不是去 AI 腔。https://github.com/loclv/agent-md
- **cclint** (`felixgeelhaar/cclint`)：lint `CLAUDE.md`；`--fix` 修格式；`cclint why --ai` 把违规行 + 规则上下文送给 Claude Haiku 出修复建议。https://github.com/felixgeelhaar/cclint
- **Gvozdev Markdown hook**：PostToolUse 后对指定目录 `*.md` 做 plain-language rewrite，默认旁路文件。这是「润色 CLI」但走 hook 而不是 `lo-md-lint` 这种 findings CLI。

### 3.3 「lint warning 触发 LLM fix」有没有人做过 (事实)

有，而且已经是工业模式，只是对象几乎都是**代码** lint，不是散文：

- **BitsAI-Fix** (ByteDance, arXiv:2508.03487)：tree-sitter 扩上下文 → LLM 出 search-and-replace patch → **再跑一遍 lint 验收** → 规则奖励惩罚多余改动。冷启动用可验证 lint 失败当 RL 数据。https://arxiv.org/html/2508.03487v1
- **ast-grep rewriters**：YAML 规则匹配 + `fix` 字段，多 rewriter 一次应用。确定性，不是 LLM。https://ast-grep.github.io/guide/rewrite/rewriter
- **Rhys Sullivan / Peter Steinberger 2025-11**：公开论点是「能写成 agent 规则的，应改写成 lint 规则」 —— 给模型清晰报错、真强制、省 context。https://x.com/RhysSullivan (quoted by @steipete/status/1993377986452898220)
- 散文侧最接近：`llm-slop-detector` 的字符 quick fix；`cclint why --ai`；Vale 标完由人 / agent 改。**没有看到成熟的「Markdown AI 腔 warning → LLM fix → 用同一套黄金集回归」开源产品。**

### 3.4 这一节的判断

`lo-md-lint` 若做 LLM 语义润色，业界默认拆成两段，不要揉成一条规则：

1. **确定性 floor** (Vale / AutoCorrect 模式)：词表、句式、标点密度 → warning，多数 non-fixable。
2. **可选 LLM pass**：只在用户要润色时跑，输出是改写或建议，不是 CI required check。BitsAI-Fix 的「再 lint 一次」值得抄：LLM patch 必须仍通过 zh-typography-1 到 zh-typography-3 和将来的 experimental 规则。

不要做第三种：用 LLM 当 linter 的判定器 (每次 CI 调模型)。没有黄金集稳定性，也没有 `spec/fixtures/` 可回归。

---

## 4. Heuristic learning 与从改写 pair 归纳规则

### 4.1 「汪嘉毅」是谁 (事实)

语音「汪嘉毅 / Wang Jiayi / Jiayi Wang + heuristic learning」对得上的公开人物是：

**翁家翌 (Weng Jiayi)**，不是汪嘉毅。

| 项 | 值 |
| --- | --- |
| 英文名 | Jiayi Weng |
| 中文名 | 翁家翌 |
| X | `@Trinkle23897` (显示名 Jiayi Weng；bio: MTS @openai, author of the entire post-training RL infra) |
| GitHub | https://github.com/Trinkle23897 |
| 文章 | *Learning Beyond Gradients*，2026-05-08，https://trinkle23897.github.io/learning-beyond-gradients/ (英 / 中切换) |
| Artifact | https://github.com/Trinkle23897/learning-beyond-gradients (页面约 603 stars) |
| 概念 | Heuristic Learning (HL) / Heuristic System (HS) |

检索时排除的近邻 (不是这个人)：

- 汪佳依 Jiayi Wang，统计助理教授 (jiayiwang1017.github.io)
- 机器人 Jiayi Wang，BIGAI / Edinburgh
- Oak Ridge 的 Jiayi Wang (联邦学习)
- 经典 AI 教材里的 heuristic search (南京大学课滑等) —— 不是「heuristic learning」这个 2026 用法

未找到「汪嘉毅」与 heuristic learning 的稳定公开对应。**最接近候选就是翁家翌。**

### 4.2 HL 实际在说什么 (事实)

博客定义 (中译综述与原文一致，`36氪` 2026-05-15 等)：

- HL 的主体是**程序代码**，不是网络权重
- 闭环仍是 state → action → feedback → update；update 由 coding agent **直接改代码** (策略、检测器、测试、配置、记忆)
- 反馈可以是环境奖励、测试、日志、视频回放、人类反馈
- HS 不只是 `policy.py`，还包括回归测试、golden trace、失败记录、版本 diff

实验结果 (博客 / artifact，作者自称)：Atari Breakout 打到理论满分 864；MuJoCo Ant / HalfCheetah 进入常见 Deep RL 量级；Atari 57 在同等环境步数下中位数高于 PPO。全程不训练神经网络。

与 lint 规则蒸馏的结构同构：**规则是显式软件，用测试和 golden case 记住旧能力，新规则不能弄坏旧 case。** 翁家翌 2026-05-25 自己也随口提过用 flake8 `C901` 这类规则给启发式代码做复杂度正则。https://x.com/Trinkle23897/status/2058754170241585639

这 **不是**「从文档改写 pair 自动挖 lint 规则」的现成产品。它是「coding agent 养护规则系统」的工程叙事。社区跟进例：流体控制里用 Codex 养可读启发式，作者 @pg_dons 明确 Inspired by @Trinkle23897 (2026-05-18)。

### 4.3 从编辑 pair / 改写 pair 归纳规则的公开做法 (事实)

| 做法 | 做什么 | 和 lo-md-lint 的距离 |
| --- | --- | --- |
| AIR (Automated Instruction Revision), arXiv:2604.09418 | 有标数据 → 聚类 → LLM 从近邻对比归纳 `if-then` 规则 → 再编译去重进 prompt | 近：可把 AI 稿 vs 人改稿当 labeled pair |
| RewriteLM (Wiki 修订), arXiv:2305.15685 | 从 Wikipedia revision 抽 `<source, target, edit summary>`，再生成指令 | 近：改写 pair 的经典数据源 |
| CoEdIT, arXiv:2305.09857 | 公开编辑语料 + 规则化指令模板，微调编辑模型 | 远：产物是模型，不是 lint 规则 |
| Rule distillation into LLM, arXiv:2311.08883 | 把**已有文本规则**蒸进权重 | 方向反了 |
| RuleEdit (ACL 2025 Findings) | 规则级知识编辑，避免一条实例改坏全局 | 远 |
| ILP / FOLD / LIME-FOLD | 从正负例归纳逻辑子句 | 近但重；散文特征噪声大 |
| LintSeq, arXiv:2410.02749 | 名字像 lint，其实用 linter 把代码切成无错误的 edit sequence 做训练数据 | 不要误用 |
| BitsAI-Fix | lint 失败当可验证监督，RL 出补丁 | 近：规则已存在，学的是 fix |

**没有找到**「对 Markdown 人改 pair 跑 edit-distance，自动产出 Vale/lo-md-lint 规则并合入 spec」的成熟开源流水线。能引用的是部件：diff 对齐、LLM 提议规则、人审、黄金集锁住。

### 4.4 这一节的判断

用户说「汪嘉毅的 heuristic learning」应标成猜测：**翁家翌 / Jiayi Weng / @Trinkle23897**。若用户其实指别人，停下来改。

可迁移的不是 Atari 分数，是运维模型：

1. 规则写在 `spec/`，fixture 是 golden trace
2. 从改写 pair 只**提议**规则 (AIR 风格 if-then)
3. 人审之后才进规范；agent 可以养实验分支，但不能绕过黄金集
4. 新规则必须跑全量 fixture，回归失败就删规则或改规则 —— 对应 HL 的「旧能力固化成测试」

不要把 HL 理解成「让模型自己从 pair 长出不可读的检测器」。翁家翌强调的价值是策略**可读、可回归、可删**。

---

## 5. 术语选词规则的可行性

### 5.1 现成中英术语表 (事实)

权威 / 半权威：

- **全国科学技术名词审定委员会**，查询台「术语在线」https://www.termonline.cn ；国务院授权，规范名词号称 60 余万条。计算机科学技术名词有第三版 (2018)。原则是单义性：一个概念一个规范中文名。https://www.cnterm.cn/
- 台湾经济部智慧财产局「国家专利技术术语中英对照」开放资料。https://data.gov.tw/en/datasets/32500
- IBM / 各厂产品 glossary (例：IBM 文档中文术语表把 cache 写作「高速缓存」)。组织方式是「首选术语 + 非首选 see also」，适合抄结构不适合当唯一词表。

开源 / 社区：

- **机器之心 AI 术语库**：约 2442 条，字段为英文术语 / 中文翻译 / 缩写 / 来源扩展。https://github.com/jiqizhixin/Artificial-Intelligence-Terminology-Database
- **菜鸟教程计算机中英对照**：含港台 vs 大陆两列。例：Cache = 快取 / 高速缓存、缓存；Key = 金钥、密钥 / 密钥。https://www.runoob.com/w3cnote/programming-en-cn.html
- **astroicers/security-glossary-tw**：470+ 繁中资安术语，YAML，可编程 `find_terms(text)`。https://github.com/astroicers/security-glossary-tw
- 领域小表：密码术语 (secret/key → 密钥，不是秘密)、计算机网络课对照 (Shared Secret Key = 共享密钥)。

`secret` → 「秘密」vs 「密钥」正是多义项，不是单行替换能解决的：密码学 / 凭证语境才是密钥；叙事「保守秘密」仍是秘密。全国名词委的单义性在这里要靠 **上下文 / 词表分区**，不能靠全局 regex。

### 5.2 AutoCorrect 词表怎么组织 (事实)

https://github.com/huacnlee/autocorrect

- 主业是 CJK-英文空格和标点 (与 lo-md-lint zh-typography-1 到 zh-typography-3 同族)
- `spellcheck` 默认关；用户在 `.autocorrectrc` 的 `spellcheck.words` 里加**项目专用**词
- 文档明确：不要把 apple、python 这种普通词放进去
- 映射语法：`nodejs = Node.js`、`AppStore = App Store` (错误形 = 正确形)
- 另有 `textRules` 对整句例外定 severity

这是「术语选词规则」最能直接抄的工程形状：**小、可覆盖、按项目、可 fix。**

### 5.3 这一节的判断

术语选词 **可以** 做 experimental、而且是少数 **可 fix** 的 AI 腔规则 —— 但必须：

- 词表分区 (crypto / 系统 / 通用)，避免 `secret` 全局替换
- 只收录高置信、单义或可被周围英文锚点消歧的项 (文中同时出现 `secret` 与 key/token/API 才报「密钥」)
- 组织抄 AutoCorrect：`wrong = right`，默认 warning，`--fix` 可选
- 权威集 (术语在线) 当参考，不要把 60 万条灌进 linter；从本仓文档真实误译 (secret/cache/token/prompt…) 长一小表，第三次重复再收

两岸用词 (视频 / 影片) 是另一条产品决策：本仓是简体情况 A，不要把繁中本地化规则混进默认包 (zh-tw-humanizer 那 48 组是台湾产品，不是 lo-md-lint 默认)。

---

## 6. 对 lo-md-lint 的映射 (判断)

当前规范 (`spec/rules.md`) 是逐行、可 fix、CJK 标点三规则。AI 腔多数 **跨句、非逐行、不可确定性 fix**。硬塞进同一模型会冲掉「输入输出逐行对齐」的黄金集假设。

较干净的切分：

| 层 | 例子 | 进 CI？ | fix？ |
| --- | --- | --- | --- |
| 已有 zh-typography-1 到 zh-typography-3 | 全角标点、括号空格 | 是 | 是 |
| Experimental warning，确定性 | 「不是 A 而是 B」密度；`综上所述` / `值得注意的是`；`load-bearing` 在中文文档里的无端出现；术语小表 | 默认关，flag 开 | 词表可 fix，句式多半否 |
| LLM 润色 | Deng spec 当 prompt；Gvozdev 式旁路 `.plain.md` | 否 | 人审后才回写 |

Claudish 哲学里「语义压缩、降抽象」几乎不能变 findings：同一命题的反复需要语义等价，黄金集写不稳。

---

## 7. 三档结论

### 7.1 建议现在记进 tracker

1. **记清爆款身份**：`gvzdv/claudish-to-english` (插件，2.4k★) vs `programasweights/claudish` (Deng 翻译器 + spec/词典，X 爆款)。Philosophy 以 Deng spec 为正本。
2. **记一条产品边界**：去 AI 腔分「确定性 warning」和「LLM 润色」两段；后者不进 required check。
3. **记中英两套指纹**：英文 Claudish (`load-bearing` / gated / 否定平行) ≠ 中文 AI 腔 (赋能 / 接住体 / 翻译腔)。不要共用一张禁词表。
4. **记 heuristic learning 的人名**：翁家翌 (Jiayi Weng, `@Trinkle23897`)，文章 *Learning Beyond Gradients*；「汪嘉毅」无稳定命中。
5. **记术语表入口**：术语在线 + AutoCorrect `wrong = right` 组织方式；`secret` 必须分语境。
6. **记可抄的 lint 近邻**：`vale-ai-tells`、`llm-slop-detector` (短语无 autofix)、BitsAI-Fix 的「LLM 补丁再 lint」。

### 7.2 建议下一步做实验

1. **手工蒸馏 10–20 条 experimental warning**：从本仓文档 + 合成 fixture 里捡「不是 X 而是 Y」、`综上所述`、无端 `load-bearing`、两三条高置信术语。全部 non-fixable (术语除外)，默认 disable，用新规则 id，不破坏 zh-typography-1 到 zh-typography-3 逐行模型 —— 若做不到逐行，先单开 `experimental/` 规范，不要假装是 zh-typography-4。
2. **收集改写 pair (合成数据)**：用 Deng 的 english-to-claudish 生成「腔」样本，人改或 to-english 当目标，跑一轮 AIR 式规则提议，人审后再决定进不进 spec。测试 fixture 只用 ACME / Foo 这类合成串。
3. **对照 Vale**：同一批中文 Markdown 跑 `vale-ai-tells` (英文规则会误报) 和中文词表 regex，量假阳性。这能回答「密度阈值要不要」。
4. **LLM 润色原型 (仓外 scratch)**：复用 Deng claudish-to-english spec 当 prompt，只对 `docs/` 出旁路文件，验证会不会改坏术语、代码围栏、zh-typography-1 到 zh-typography-3。不接 CI。
5. **术语小表试点**：5–15 个本仓真实会错的词 (密钥 / 缓存 / 令牌…)，带上下文锚点，测 `--fix` 会不会误伤「秘密」。

### 7.3 不建议碰

1. **不要**把 Gvozdev 插件或 Deng 翻译器当依赖嵌进 `lo-md-lint`。一个是 Claude Code hook，一个是 PAW function；都不是 findings CLI。
2. **不要**用 LLM 当 CI 判定器去「检测 AI 腔」。无回归、费钱、假阳性无上限；维基明确反对把检测器分数当删除标准。
3. **不要**做全局禁 em dash / 禁破折号 / 禁 `delve` 的 error 级规则。证据显示单点 tell 不可靠，而且会误伤人类作者 (华盛顿邮报、Economist、维基 caveat)。
4. **不要**灌入术语在线 60 万条或 Humanizer-zh 全量模式。后者 star 高、维护差，且大量模式不可判定。
5. **不要**做「骗过 GPTZero」的 humanizer。和文档 linter 目标冲突，也有检测对抗的伦理问题。
6. **不要**把繁中本地化 (视频→影片) 或小红书腔清洗当默认规则。超出本仓中文技术文档范围。
7. **不要**把 MadAppGang/claudish (模型代理) 写进同一条 tracker。同名干扰。

---

## 附录：主要 URL (均为 2026-08-31 访问)

**Claudish**

- https://github.com/gvzdv/claudish-to-english
- https://github.com/programasweights/claudish
- https://programasweights.com/claudish
- https://raw.githubusercontent.com/programasweights/claudish/main/specs/claudish-to-english.md
- https://raw.githubusercontent.com/programasweights/claudish/main/specs/english-to-claudish.md
- https://x.com/yuntiandeng/status/2091201867737145472
- https://github.com/archiewood/claudeisms
- https://louisabraham.github.io/load-bearing/

**特征清单**

- https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing
- https://zh.wikipedia.org/zh-cn/Wikipedia:AI生成文的特征
- https://github.com/krishnasunkam/vale-ai-tells
- https://github.com/mandakan/llm-slop-detector
- https://github.com/op7418/Humanizer-zh
- https://github.com/Raymondhou0917/speak-human-tw
- https://www.ifanr.com/1665148
- https://juejin.cn/post/7659706081527169075

**HL / 规则蒸馏**

- https://trinkle23897.github.io/learning-beyond-gradients/
- https://github.com/Trinkle23897/learning-beyond-gradients
- https://arxiv.org/html/2604.09418v1 (AIR)
- https://arxiv.org/html/2508.03487v1 (BitsAI-Fix)

**术语**

- https://www.termonline.cn/
- https://github.com/huacnlee/autocorrect
- https://github.com/jiqizhixin/Artificial-Intelligence-Terminology-Database
