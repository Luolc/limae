# 调研报告：LLM 语义润色怎么做

> 来源：Grok research agent 调研，2026-08-31；distill 决策另行进 spec / ADR，本文只是证据。产品身份、热度、Deng spec 的 philosophy 清单已见 `docs/research/claudish-and-ai-slop-survey.md` §1，本文不重复考证，只补源码与机制。

调研日期：2026-08-31。只读，不改两个第三方仓，也不改本仓 `spec/`、`src/`、`tests/`。对照对象是本仓两段式边界 (`docs/adr/0005-agent-native-positioning.md` §四 / §六)、润色独立于 lint (`docs/adr/0006-rule-grading-fixability-severity-experimental.md` §四) 与现行规则 (`spec/rules.md` 的 `zh-typography` 一族、zh-tell-1–zh-tell-5、zh-word-1–zh-word-2)。

**观点与事实分开**：带「建议」「宜」「不要」的句子是调研结论；源码行为、本仓规范原文是事实。

第三方仓与本文引用的 commit (与 brief 一致)：

| 仓 | 本地路径 | commit | 作者日期 | 许可 |
| --- | --- | --- | --- | --- |
| [gvzdv/claudish-to-english](https://github.com/gvzdv/claudish-to-english) | `~/3p/gvzdv/claudish-to-english` | `bf271f9` (0.9.0) | 2026-08-28 | MIT (`.claude-plugin/plugin.json:12`) |
| [programasweights/claudish](https://github.com/programasweights/claudish) | `~/3p/programasweights/claudish` | `700588c` | 2026-08-29 | MIT (`LICENSE:1`) |

下文路径相对各自仓根。Gvozdev 仓没有测试套件 (`CLAUDE.md:7`)；Deng 仓对词典有 `dictionary/validate.py`，翻译器本身没有测试。

---

## 0. 先说结论 (判断)

两个项目都是 **LLM 整段改写**，不是 linter，也不是逐句 / unified diff 补丁器。对 `lo-md-lint` 能抄的是合同，不是产品形态：

- Gvozdev：显示层或旁路文件、fail-open、prompt 很短、**代码围栏几乎只靠 prompt**，YAML frontmatter 是唯一真正机械保护的结构。
- Deng：两份可复制 spec + 本地 PAW function；词典是给人读的 field guide，**运行时不进 prompt**；明确禁止机械换词。

中文不能直接套 Deng 的英文 Claudish spec。中文要处理的是翻译腔、量词、全角标点与术语对照；这些里只有标点 / 空格 / 带锚点的选词已经在 `zh-typography` 一族与 zh-word-1，剩下的才是润色。

**与 `zh-typography` 一族 的顺序：先润色，后 lint。** 先 `--fix` 再润色，模型会把全角标点与 CJK 空格改回去；先润色再 `--fix`，排版家规能把模型的标点病修好，且几乎不碰语感。人审的对象是「润色 + 排版 fix」之后的旁路文件，不是模型的生输出。

原型建议：做成 **可选的 agent skill (主路径) + 独立批跑入口 (次路径)**，产物默认旁路文件；不要做成 `lo-md-lint` 的 CI 子命令，也不要把 LLM 链进 `--fix`。结构不变量用本仓已有的全局豁免解析器做确定性检查，黄金集测保护器，不测模型散文。

---

## 1. Gvozdev：`claudish-to-english` (事实)

Claude Code 插件，版本 `0.9.0` (`.claude-plugin/plugin.json:3`)。纯 bash + `jq` + `curl`，无构建、无测试套件 (`CLAUDE.md:7`)。合同写在 `CLAUDE.md:29`：

> On *any* problem — provider down, timeout, no `jq`, malformed payload, missing file — a hook emits nothing and exits 0, leaving Claude's original text on screen.

配置优先级全仓一致：flag 文件 > 环境变量 > settings 文件 > 内置默认 (`CLAUDE.md:25`)。flag 文件存在，是因为 Claude Code 的 `env` 在会话启动时冻结，中途改不了 (`rewrite.sh:85`)。

### 1.1 触发点

三只 hook，注册在 `hooks/hooks.json`：

| 事件 | 脚本 | timeout | 默认是否做事 |
| --- | --- | --- | --- |
| `SessionStart` | `session-notice.sh` | 10 s | 只公告上次 `/claudish` 留下的 flag，不改写 (`session-notice.sh:3`) |
| `MessageDisplay` | `rewrite.sh` | 60 s | 默认开：显示层改写助手回复 |
| `PostToolUse` (matcher `Write\|Edit`) | `rewrite-md.sh` | 180 s | 默认关：要设 `CLAUDISH_MD_DIR` 才碰磁盘 (`rewrite-md.sh:10`) |

斜杠命令 `/claudish` (`commands/claudish.md`) 不自己调模型。它只跑 `claudish-ctl.sh`，写 `~/.claude/claudish-{off,mode,style,lang,model}` 这组 flag，并在 `last` 时从 transcript 重打原文 (`claudish-ctl.sh:17`、`claudish-ctl.sh:210`)。

**没有独立 CLI 翻译器。** 离开 Claude Code 这组 hook 不运行。

`MessageDisplay` 按流式 chunk 各起一个进程 (`rewrite.sh:5`)。hook 把每个 `.delta` 按 `message_id` 落到 `$TMPDIR/claudish-to-english/<session>/<message>/<index>.part` (`rewrite.sh:189`)，只在 `.final == true` 时拼全文、调一次模型 (`rewrite.sh:218`)。非 final 的 chunk：`append` 模式原样放行，`replace` 模式发空字符串把原文压掉 (`rewrite.sh:205`)。

### 1.2 Prompt / spec 怎么组织

没有 Deng 那种独立 spec 文件。System prompt **内联在 bash 里**，约一段话，可被整份文件替换，不合并。

默认 prompt (`rewrite.sh:287`)：

> You rewrite the assistant's message into much simpler, plain language. Write the rewrite in the same language as the message you are rewriting. Keep every fact, name, number, and file path. Use short sentences and everyday words. Leave fenced code blocks unchanged. Output ONLY the rewritten message with no preamble, labels, or commentary.

分层 (后写的覆盖先写的，除非注释标明例外)：

1. **内置默认**，或 style preset 整段换掉默认：`tldr` / `5y` / `caveman` (`rewrite.sh:291`)。`tldr` 要求明显更短 (目标一半)，并且 **Omit fenced code blocks** —— 与默认的「围栏不动」相反 (`rewrite.sh:292`)。
2. **语言行** (有配置才加) 写在 prompt-file 检查之前：自定义 prompt 被视为完整 prompt，自带语言 (`rewrite.sh:297`)。
3. **`CLAUDISH_PROMPT_FILE` 整份替换**，读不到或空则回退默认 (`rewrite.sh:304`)。自定义 prompt **大于** style。
4. **人称锚定** 故意放在 prompt-file **之后**：输入是助手对用户的一回合，`I/me/my` 是助手、`you/your` 是用户；这是输入事实不是风格，自定义 prompt 不许把它写错 (`rewrite.sh:314`)。
5. **用户原问** 从 `.transcript_path` 取最后一条非 meta 的 user 文本，截到 800 个 codepoint，只当上下文，禁止改写 / 回答 / 复述这个问题 (`rewrite.sh:322`)。

Markdown hook 另有一份内联 prompt (`rewrite-md.sh:198`)，可被 `CLAUDISH_MD_PROMPT_FILE` 整份替换。它比显示 hook 多要了 Markdown 结构：

> Keep every fact, name, number, link, and file path. Keep all Markdown structure — headings, lists, tables, and links. Do NOT change fenced code blocks or any YAML frontmatter; reproduce them exactly.

显示 hook 的默认 prompt **没有**提到行内代码、链接、表格。style preset 里的 `5y` / `caveman` 补了 “Keep technical terms, commands, and identifiers in their original form” (`rewrite.sh:293`)，默认那条没有这句。

语言解析在 `lang.sh`：`CLAUDISH_LANG_FILE` > 显式 (含空) `CLAUDISH_LANG` > 项目 / 用户 `.claude/settings*.json` 的 `language` 键，与 Claude Code 自己拼 `# Language` 的顺序相同 (`lang.sh:10`)。值要清洗：控制字符折成空格、最多 3 个词 / 30 个 codepoint (`lang.sh:24`)，因为这份 settings 会随仓库走，不可信。

### 1.3 词典

**没有词典。** 仓内无 `dictionary/`、无词表文件、prompt 不注入术语对照。去 Claudish 的办法就是「更短、更日常」。Philosophy 清单在 Deng 仓，不在这里 (既有调研 §1.5)。

### 1.4 输入输出粒度

| 路径 | 输入 | 输出 | 写盘？ |
| --- | --- | --- | --- |
| 显示 hook | 拼好的整条助手消息 | `hookSpecificOutput.displayContent` (`rewrite.sh:147`) | 否。transcript 与推理保留原文 (`README.md:12`) |
| Markdown hook | 整份 `.md` 的 body (frontmatter 已剥掉) | `sibling`：`NAME.plain.md`；`overwrite`：原地加 marker (`rewrite-md.sh:244`) | 是，经 shell `mv`，不走 Claude 的 Write，所以不重入 PostToolUse (`rewrite-md.sh:19`) |

都是 **整段一次改写**。没有逐句、没有 search-and-replace、没有 unified diff。短于 `CLAUDISH_MIN_CHARS` (默认 200，剥掉围栏后再数非空白字符) 的消息 / 文件直接跳过 (`rewrite.sh:222`、`rewrite-md.sh:175`)。

显示模式：`append` (默认) 原文流完再附一块带标签的改写；`replace` 只显示改写，失败则把全文原文重新打出来 (`rewrite.sh:392`)。

`tldr` 允许改写比原文短很多，并且丢掉围栏；这与「结构保真」不是同一条产品线。

### 1.5 怎么防改坏代码块 / 链接 / 术语

**机械保护只有 YAML frontmatter。** `rewrite-md.sh:146` 在调模型之前，若首行是 `---` 且后面有闭合 `---`，把整块 (含定界行) 撕下来，只把 body 送给模型，写回时再拼回去。overwrite 的 marker 写在 frontmatter 之后，以免它不再是 frontmatter (`rewrite-md.sh:246`)。

围栏代码块：

- **长度门**用 awk 按行首 ` ``` ` 翻转开关，把围栏行从「散文长度」里扣掉 (`rewrite.sh:223`)。这只影响「要不要改写」，**不**把围栏从送给模型的文本里拿掉。
- **内容保护只靠 prompt** (“Leave fenced code blocks unchanged”)。没有占位符、没有二次 diff、没有「围栏字节级相等」的断言。
- 行首 `~~~` 的围栏：这段 awk **认不出来** (只看 ` ``` `)。本仓全局豁免两种定界 (`spec/rules.md`「全局豁免」第 1 条)，Gvozdev 的长度门与它不对齐。
- `tldr` 明确叫模型 **省略**围栏 (`rewrite.sh:292`)。

行内代码、链接 destination、表格分隔行、标题标记：Markdown prompt 用自然语言要求保留结构与 `link targets` (`rewrite-md.sh:205`)；显示 hook 的默认 prompt 连这些都没写。没有解析器。

术语：prompt 要求保留 fact / name / number / file path；没有词表、没有「密钥 vs 秘密」这类对照。

失败合同：空改写、截断 (ollama `done_reason: length`、Anthropic `stop_reason: max_tokens`、OpenAI `finish_reason: length`) 一律丢弃，不把半成品写上屏或写进文件 (`providers.sh:239`、`providers.sh:271`、`providers.sh:325`)。overwrite 模式尤其依赖这一条 (`README.md:531`)。

### 1.6 人审环节

**没有 accept / reject 门。** 改写自动出现。

人能做的：

- `append` 模式下原文与改写同时在屏上，人可以只看原文。
- `/claudish last` 从 transcript 重打原文；回复以 `<!-- claudish:original -->` 开头，显示 hook 见到就跳过 (`rewrite.sh:197`、`commands/claudish.md:14`)。
- Markdown `sibling` (默认) 不碰原文，人可以去看 `.plain.md`。`overwrite` 无预览、无确认，只靠 marker 防二次咀嚼 (`rewrite-md.sh:166`)。README 自己说弱模型会把真文档改坏 (`README.md:410`)。
- `/claudish off` 或 `touch ~/.claude/claudish-off` 暂停后续改写，已经写出去的 `.plain.md` / overwrite 不会回滚。

人在环路的位置是 **事后对照**，不是写盘前的闸。

### 1.7 模型调用与成本

自己发请求，**不**复用当前 Claude 会话的补全 (oauth 模式是唯一的例外，见下)。`providers.sh:136` 的 `llm_complete SYSTEM USER` 对四种 provider 各打一次非流式补全：

| `CLAUDISH_PROVIDER` | 默认模型 | 键 | 备注 |
| --- | --- | --- | --- |
| `ollama` (默认) | `gemma4:26b-mlx` | 无 | `http://localhost:11434/api/chat`；请求带 `think: false`、`temperature: 0.3` (`providers.sh:314`) |
| `codex` | CLI 自己的默认 | 无 (用 CLI 登录) | `codex exec --sandbox read-only`，在 `$TMPDIR` 下跑，不进仓库 (`providers.sh:285`) |
| `anthropic` | `claude-haiku-4-5` | `CLAUDISH_ANTHROPIC_KEY` 或 `ANTHROPIC_API_KEY` | Messages API；`max_tokens` 默认 4096 (`providers.sh:69`) |
| `openai` | `gpt-5.6-luna` | `CLAUDISH_OPENAI_KEY` 或 `OPENAI_API_KEY` (仅官方 URL 必填) | 任意 OpenAI-compatible；打 `api.openai.com` 时默认 `reasoning_effort: "none"` (`providers.sh:80`) |

超时：显示 hook 默认 45 s，须低于 hook 的 60 s；Markdown hook 默认 150 s，须低于 180 s (`hooks/hooks.json:20`、`hooks/hooks.json:32`，`rewrite.sh:117`，`rewrite-md.sh:75`)。

README 给出的唯一性能数字 (2026-08-31 仍写在 `README.md:416`)：默认 `gemma4:26b-mlx` 大约 60 tokens/s，长文档 30–120 s；该模型约 17 GB，且是 Apple-silicon MLX 构建 (`README.md:44`)。**美元单价未核实** —— 仓内没有报价表。

成本控制是工程手段，不是账单：

- 默认走本机 ollama，正文不离开机器 (`README.md:624`)。
- 短于 200 个散文字符跳过。
- 推理关掉 (`think: false` / `reasoning_effort: none`)，避免为一次改写烧 reasoning token (`README.md:615`)。
- 打到输出上限的半成品丢弃。
- 云端 provider 会把每条助手消息 (以及 Markdown hook 开启时的文件正文) 送到外部 API；README 把选云端 provider 本身当成 consent switch (`README.md:462`)。

`CLAUDISH_ANTHROPIC_AUTH=oauth` 从 macOS Keychain 或 `~/.claude/.credentials.json` 读 Claude Code 的 access token，改写记到用户订阅上。脚本自称 UNOFFICIAL，每次会话第一次成功改写会警告 (`providers.sh:56`、`providers.sh:422`)。oauth 模式拒绝非默认 `CLAUDISH_ANTHROPIC_URL`，以免 token 打到代理 (`providers.sh:148`)。

**未核实：** 一次典型助手消息的 prompt / completion token 数；Haiku / Luna 的实际账单；`gemma4:26b-mlx` 在非 Mac 上的替代模型质量。

---

## 2. Deng：`programasweights/claudish` (事实)

Yuntian Deng 的双向翻译器。README 写明 Inspired by Gvozdev 仓，unofficial parody，与 Anthropic 无官方关系 (`README.md:47`)。`LICENSE` 是 MIT。

### 2.1 触发点

**CLI 翻译器，不是 hook。** `translate.py:17`：

```text
Usage: python translate.py {to-claudish|to-english} TEXT
```

两个 ProgramAsWeights function id 写死在 `translate.py:10`：`to-claudish = ca9d5165b6c8e6615529`，`to-english = e469f61ccab2699fbd51`。`paw.function(...)` 之后 `print(translate(sys.argv[2]))` (`translate.py:21`)。没有文件输入、没有目录扫描、没有 Markdown 感知。长文档得自己把正文塞进 argv。

README 说程序下载一次后本地跑，不要传 `max_tokens`，PAW 在 EOS 自然停 (`README.md:24`)。Live demo 在网站；本仓源码不包含网站。

### 2.2 Prompt / spec 怎么组织

两份 Markdown spec，给人复制进自己的 PAW program 或其它模型 (`README.md:38`)：

| 文件 | 行数 | 角色 |
| --- | --- | --- |
| `specs/claudish-to-english.md` | 205 | 正向：Claudish → 直白英语 |
| `specs/english-to-claudish.md` | 39 | 反向：把普通英语写成 Claudish (生成「腔」样本用) |

**`translate.py` 不读这两份文件，也不读词典。** 运行时只有 function id。规范与权重是否字节一致，从本仓源码 **无法核实**；README 把 spec 标成可复制的正本，不是运行时加载的资源。

正向 spec 的结构 (比 Gvozdev 默认 prompt 厚一个数量级)：

1. 定义 Claudish，目标是 smallest set of ordinary propositions (`specs/claudish-to-english.md:1`)。
2. 改写必须是 paraphrase，不是对输入的回答；不添事实 (`specs/claudish-to-english.md:5`)。
3. Prefer semantic compression：多句可压成一句；不要一对一保留句数 (`specs/claudish-to-english.md:13`)。
4. 降抽象、拆修辞、解码隐喻、保逻辑外延、拆连字符压缩、降研究腔 (`specs/claudish-to-english.md:35` 起)。
5. **不要机械换词典** (`specs/claudish-to-english.md:106`)：

   > Do not mechanically replace words using a fixed dictionary.

6. 合法术语要留 (`specs/claudish-to-english.md:176`)。
7. 保真收尾 (`specs/claudish-to-english.md:203`)：

   > Preserve names, quotations, commands, code, and technical terminology whose wording must remain fixed.
   >
   > Output only the rewritten text.

反向 spec 要求可见的风格变换 (至少改结构 / framing / 抽象层级等两项)，长度与输入大致相当，短输入通常仍是一句 (`specs/english-to-claudish.md:25`)。high-signal 词名单在 `specs/english-to-claudish.md:17`，既有调研 §1.4 已摘。

### 2.3 词典形制

`dictionary/entries.json`，`schema_version` 必须为 2 (`dictionary/validate.py:53`)。这是网站词典的正本 (`dictionary/README.md:3`)，**不是**翻译器的运行时词表。

校验器强制的规模与字段 (`dictionary/validate.py:15`、`dictionary/validate.py:130`)：

- 条目数 25–60；commit `700588c` 上是 **30 条** (word 15 / phrase 6 / reassurance 5 / construction 4)。
- 每条恰好这些键：`slug`、`term`、`plain_english`、`explanation`、`category`、`aliases`、`example` (`example` 内恰好 `claudish` + `english`，且两者不得相同)。
- 四个 category：`word` / `phrase` / `reassurance` / `construction`。
- `phrase_guide` 3 条组合规则：`x-shaped`、`metaphor-stacking`、`contrastive-reveal`。
- `recommended_slugs` 必须与全部 entry slug 是同一集合。
- `specimens` 7 条，必须指向 `sources` 里的公开 URL，且注明是模型输出 (`dictionary/README.md:25`)。commit 上 5 个 source，都是 GitHub 上的公开 issue 评论或 claudeisms 语料。

编辑说明原文 (`dictionary/entries.json:4`)：

> These are ordinary English and technical terms, not words invented by Claude. Claudish emerges from their unusual frequency, combinations, metaphors, and sentence structures.

网站 vendoring 一份 commit-pinned 的 `entries.json`，不在访客浏览器里现拉 GitHub (`dictionary/README.md:34`)。

与本仓 `spec/wordlists/` 的差别：Deng 词典是 **释义 + 例句的 field guide**；本仓 zh-word-1 是 `wrong` / `right` / `anchors` 的可执行替换 (当前 3 条)，`zh-tell` 与 `en-tell` 两族是禁词 / 句式表。Deng 的 30 条 **不能**当 zh-word-1 来跑 —— spec 自己禁止机械替换，且多数词 (如 `canonical`、`drift`) 在技术文档里是合法用语 (`specs/claudish-to-english.md:178`；本仓 en-tell-3 只收了 `load-bearing` 一条，理由写在 `spec/rules.md` 的 en-tell-3)。

### 2.4 输入输出粒度

一次调用、一个字符串、stdout 一行 (或一段) 改写。粒度由调用方决定：README 示例是单句 (`README.md:30`)。没有 chunk 缓冲，没有「按 Markdown 块切」，没有旁路文件。正向 spec 允许输出比输入短很多 (`specs/claudish-to-english.md:201`)，因此 **不保证行数不变** —— 与本仓 `--fix`「不增删行」的契约 (`spec/rules.md`「处理单位」) 直接相反。

### 2.5 怎么防改坏代码块 / 链接 / 术语

**全靠 prompt。** 仓内没有 Markdown 解析器、没有 frontmatter 剥离、没有围栏占位。保护句就是 §2.2 引的 “Preserve names, quotations, commands, code…”。链接、表格、行内代码没有单独条款。

逻辑外延是另一类「别改坏」：`Do X if Y` 不得变成 Y 是唯一触发；`required` 不得变成 `sufficient`；隐喻含糊时取最窄解释 (`specs/claudish-to-english.md:108`)。这是语义保真，不是结构保真。

### 2.6 人审环节

**没有。** CLI 打印结果，人自己决定贴不贴回去。网站 demo 的人审 UI 不在本仓源码里，**未核实**。

### 2.7 模型调用与成本

`import programasweights as paw`，安装走 `--extra-index-url https://pypi.programasweights.com/simple/` (`README.md:11`)。权重下载后本地跑。本仓源码看不到底层模型名、量化、硬件需求、是否走网络推理。

**美元成本、延迟、token 数均未核实** (本次只读源码，未跑 PAW、未装该 index)。与 Gvozdev 的差别是：这里没有「每个 Claude 消息打一次 API」的热路径，是人显式调用的翻译函数。

---

## 3. 两侧对照 (事实)

| 维 | Gvozdev 插件 | Deng 翻译器 |
| --- | --- | --- |
| 触发 | Claude Code hook + `/claudish` 开关 | CLI `translate.py` / `paw.function` |
| Prompt | 内联一段话，可整份替换；style 三档 | 两份独立 spec，运行时不加载 |
| 词典 | 无 | 30 条 field guide，运行时不注入 |
| 粒度 | 整条消息 / 整份 md body | 调用方给的整段 TEXT |
| 结构保护 | frontmatter 机械；围栏 / 链接 / 行内代码靠 prompt | 全靠 prompt |
| 人审 | 无闸；append / sibling 方便对照 | 无 |
| 模型 | 自建请求：ollama / codex / Anthropic / OpenAI-compatible | PAW 本地 function |
| 失败 | fail-open，半成品丢弃 | 源码未见 fail-open；异常即 CLI 失败 |
| 行数 | 默认「改写全文」，`tldr` 可大幅缩短 | 正向 spec 鼓励压缩句数 |
| 中文 | 「跟输入同一语言」；小模型非英文质量 README 已警告 (`README.md:305`) | spec 与词典都是英文 Claudish |

---

## 4. 对中文的适用性

### 4.1 事实

Gvozdev 的默认 prompt 要求 “Write the rewrite in the same language as the message you are rewriting” (`rewrite.sh:287`)。`lang.sh:28` 把 `简体中文` 当作合法语言名的例子。README 的 caveat (`README.md:305`)：

> a rewrite is only as good as the model that writes it, and small local models simplify English noticeably better than they simplify anything else.

Deng 的两份 spec、30 条词典、7 条 specimen 全部是英文。Claudish 的定义本身是 Claude 的英语修辞 (`specs/claudish-to-english.md:3`)。没有中文翻译腔、量词、全角标点条款。

本仓已经落地、且与润色相邻的确定性规则 (`spec/rules.md`「每条规则的三轴取值」)：

- `zh-typography` 一族：fixable · error · stable，管标点宽度与空格。
- `zh-tell` 与 `en-tell` 两族：non-fixable · warning · experimental，管套话 / 句式 / 黑话 / 聊天残留 / 英文 tell / 「零 + 名词」。
- zh-word-1：fixable · warning · experimental，3 条带锚点的 `wrong = right`。
- zh-word-2：non-fixable · warning · experimental，「秘密」误用。

ADR-0006 §四：LLM 润色是独立一条线，不以 claudish 检测为前提，也不从 warning 计数触发。ADR-0006 §五：规则不分语言，英文 tell 出现在中文文档里同样报。

既有调研 §2.2 列出的中文特有层 (翻译腔、直译术语、公文黑话、接住体) 本次不复考；它们仍然构成「Deng spec 覆盖不了的那一截」。

### 4.2 中文润色要多做的事 (判断)

直接拿 Deng 正向 spec 当中文 prompt，会漏掉这些：

1. **术语中英对照。** `secret` → 「密钥」而不是「秘密」，`cache` → 「缓存」而不是「快取」，是 ADR-0005 §四写出的动机。zh-word-1 已经能确定性修 3 组；润色 prompt 应要求「英文标识符保持原词，中文只用本仓 `zh-word` 一族的人话」，不要让模型另造「门控 / 一等公民」。没有唯一替换的词 (zh-word-2 的「秘密」) 只许标、不许模型自行选三个候选之一写死 —— 那是人审。
2. **全角标点与中英空格。** 英文模型 (以及 Gvozdev 默认的 Gemma) 会输出 CJK 旁的半角逗号、括号贴汉字、漏掉 zh-typography-4 空格。Gvozdev / Deng 的 prompt 都没提 GB/T 标点。这不是语义问题，不该靠模型记家规。
3. **量词与语序。** 「正在飞」、翻译腔的 `He gave me a smile` → 「他给了我一个微笑」，是既有调研 §2.2 / §2.3 的中文层。Deng 的「降抽象、拆 not X but Y」对这类几乎无用；要写进中文润色 spec 的是「删翻译腔、补量词、恢复中文语序」，并配合成例句 (ACME / Foo)，不要配真实业务句。
4. **中英两套指纹不要混进一张禁词表。** 既有调研 §2.4 与 ADR-0006 §五已经定了：共享句式 (否定平行、三段式)，词表各管各的。润色 prompt 也应两段：英文 Claudish 用 Deng 正向 spec；中文用一套更短的中文条款 (套话、黑话、翻译腔)，不要把 `load-bearing` 的释义翻译成中文禁词。

Gvozdev 的「跟输入同一语言」对中文 **机制上可用**，质量取决于模型。默认 `gemma4:26b-mlx` 按 README 自己的话不宜当中文润色器。中文原型应点名一个能写中文的模型，或复用已经在写这篇文档的 agent 会话；具体哪一档在本机上够用，**未核实**。

### 4.3 与 `zh-typography` 一族 的先后顺序 (判断)

先把两条会出事的流水线写清楚，再给建议。

**先 `--fix` 再润色会出什么问题**

- zh-typography-1 刚把 `你好,世界` 收成 `你好，世界`，英文中心的润色模型很容易把逗号改回半角。Gvozdev / Deng 的 prompt 都不提全角，没有机制阻止这件事。
- zh-typography-3 / zh-typography-4 / zh-typography-7 / zh-typography-8 / zh-typography-11 补的空格同样会被「更短、更日常」吃掉。
- zh-word-1 若已把「秘钥」换成「密钥」，润色仍可能再写成「秘密」或「秘钥」 —— 模型没有锚点约束。
- `--fix` 还可能改掉润色想保留的原文误译，让人审看不到「模型原来写错了什么」。先 fix 等于毁掉蒸馏 pair 的 before。

**先润色再 `--fix` 会出什么问题**

- `zh-typography` 一族 的 `--fix` 改的是标点宽度与空格，**几乎不改选词和句式**，所以不破坏 Deng 意义上的语感 (哪几个命题、怎么压缩)。这是家规落地，不是「把人话改回腔」。
- zh-word-1 的 `--fix` **会改词**。若润色故意在无锚点行里写了「代币」(钱的意思)，zh-word-1 因无锚点不动；若同行出现了 `token`，zh-word-1 会改成「令牌」。这是 zh-word-1 的既有契约，不是润色引入的新风险；experimental 默认关，原型可以先只跑 `zh-typography` 一族。
- `zh-tell` 与 `en-tell` 两族全是 non-fixable，`--fix` 碰不到套话。套话要么润色删掉，要么留下当 warning。
- Deng 允许减句、Gvozdev 的 `tldr` 允许减半并丢掉围栏。`--fix` 不增删行 (`spec/rules.md`「处理单位」)，所以 **减句是润色自己的事，lint 既修不回来，也不该修**。结构不变量检查要拦的是围栏 / 链接被改坏，不是句数。
- 若人审的是模型生输出、写回后再 `--fix`，人看到的和入库的不一致。所以人审应发生在 `--fix` 之后。

**建议 (明确)**

流水线固定为：

1. (可选) `lo-md-lint` **check** 原文，tell / word 两类家族的 findings 当作润色 prompt 的 hint。这是 ADR-0006 §四允许的「使用方式」，不是「warning 累积才润色」。没有 findings 也可以润色。
2. LLM 润色 → 旁路文件，**不覆盖正本**。
3. **确定性**结构不变量检查 (围栏、行内代码、destination / URL、表格骨架、frontmatter)。失败则丢弃这次润色，fail-open 回原文。
4. 对旁路文件跑 `lo-md-lint --fix` (默认 `zh-typography` 一族；zh-word-1 仅当 `enable_experimental` 已开)。再 `check` 一次当验收。
5. 人对照「原文 vs (润色 + 排版 fix) 后的旁路」，接受后才写回。
6. 被接受的 pair 才进入 heuristic learning；被 `--fix` 改过标点的 after 仍算同一 pair 的 after，before 必须是润色前的原文。

**不要**先 `--fix` 再润色，也 **不要**指望润色 prompt 自己遵守 `zh-typography` 一族。排版是确定性段的工作；语义压缩才是模型的工作。BitsAI-Fix 的「补丁再 lint」既有调研 §3.3 已有工业先例，方向与这条一致。

---

## 5. 给 lo-md-lint 的润色原型建议 (判断)

本节落到「可以据此写 ADR」的粒度：合同、顺序、测什么、和 HL 怎么接。按 brief **不写 ADR、不写代码、不设计目录结构**。

### 5.1 形态

三条路，各自利弊：

| 形态 | 利 | 弊 |
| --- | --- | --- |
| `lo-md-lint` 的 CLI 子命令 (如 `polish`) | 用户只记一个二进制；能直接复用本仓的豁免解析与 `--fix` | 把非确定性、要密钥、要网络的路径塞进以 CI 为质量门的工具；容易被误接进 required check；和「lint 快、公式化、确定性」(`docs/adr/0005-agent-native-positioning.md:32`) 抢同一入口 |
| agent skill | 人已经在会话里，审稿是同一回合；不给 linter 加 API 依赖；ADR-0005 的 agent-native 定位天然匹配；三家 harness 都能读项目 skill | 没有 Claude Code 就没有 Gvozdev 那种 hook 热路径；批跑 `docs/` 要另开会话；skill 文本本身会过期，要当 prompt 正本养护 |
| 独立工具 (Gvozdev `rewrite-md.sh` 那种) | 与 linter 解耦，fail-open 不影响 `check` 退出码；可被 skill、CLI、将来的 CI 可选 job 共用 | 又一个发行面；两套配置；若它自己再实现一遍 Markdown 豁免，会和 `spec/rules.md` 漂移 |

**建议：** 第一期主路径是 **项目 skill** (人在环路、旁路文件、不进 CI)。批跑需要时再做一个 **独立入口**，内部调用同一份润色 prompt 与同一份结构检查，但 **不要**挂成 `lo-md-lint check/fix` 的默认子命令。Gvozdev 的 `MessageDisplay` hook 是「读助手回复」的产品，不是文档 linter，不要移植到本仓。

tracker 已有的「LLM 润色原型：Deng spec 当 prompt、旁路文件、不进 CI」(`docs/tracker.md:28`) 与这条同向，可在 ADR 里收成合同，不必另起产品名。

### 5.2 模型调用

**建议默认走「当前 agent 会话」**，不要在 linter 进程里再打一条 API。理由：

- 润色本来就要人审 (ADR-0005 §四)；人已经为这次会话付了模型。
- 本仓是 public 仓，工具代码里再嵌一套 provider / 密钥解析，会重复 Gvozdev `providers.sh` 的整个攻击面 (oauth、Keychain、ambient `ANTHROPIC_API_KEY`)。
- Gvozdev 把「选云端 provider」当成 consent switch (`README.md:462`)；文档润色同样会把正文送出去。第一期用当前会话，egress 边界与用户已经接受的 agent 相同。

若做独立批跑入口，provider 合同宜抄 Gvozdev 的 **fail-open + 默认本机**：

- 默认不打云。本机 ollama 或用户显式配置的兼容端点。
- 半成品 (打到 `max_tokens` / `length`) 丢弃，不写旁路文件。
- 关掉 reasoning (`think: false` / `reasoning_effort: none`)；润色不是解题。
- **不要**抄 `CLAUDISH_ANTHROPIC_AUTH=oauth`。

Prompt 组织建议分层，抄两侧各自擅长的，不要混成一份 200 行英文 spec 直接喂中文：

1. **合同段** (短、稳定，相当于 Gvozdev 默认 prompt)：只输出改写；保留事实 / 数字 / 路径；围栏、行内代码、链接 destination、表格骨架不动；不是对文档的「回答」。
2. **中文语义段** (本仓要新写的，短)：删翻译腔与套话、补量词、术语跟 `zh-word` 一族走、不要造 Deng 词典里那些英文隐喻的中文直译。
3. **英文 Claudish 段**：Deng 正向 spec 可整份当附录，只在输入含英文行文时启用；不要把 30 条词典贴进 prompt (spec 禁止机械换词，而且会教模型「看到 `canonical` 就删」)。
4. **人称 / 文类锚定** (Gvozdev `rewrite.sh:320` 那一层)：文档不是助手对用户说话，「我」不是模型。
5. (可选) 本次 `check` 的 tell / word 两类家族的 findings，当 hint 不当必改清单。

**成本量级：** Gvozdev 仓内能引用的只有 60 tokens/s、长文档 30–120 s、Anthropic 默认 4096 completion cap、显示 45 s / 文件 150 s 超时。美元、本仓 `docs/` 一篇典型 ADR 的 token 数、中文模型相对 Gemma 的质量，**均未核实**。第一期用当前会话的话，增量成本就是「多一轮改写」，不要另报一套云账单。

### 5.3 产物

**默认旁路文件**，文件名合同可以与 Gvozdev 的 sibling 相同：`NAME.plain.md` 写在原文旁边，或写到用户指定的对照目录。不要默认 overwrite。

另外两份只作人审辅助，不替代旁路文件：

- **unified diff** (原文 vs 旁路)：人审时比读两份全文快。
- **建议列表**不适合做主产物。语义压缩跨句、可减行 (Deng `specs/claudish-to-english.md:29`)，落不进本仓 `.findings` 的「一行一条」。硬做成 findings 会逼模型一对一改词，正好违反 Deng 的「不要机械换词典」。

写回合同 (与 ADR-0005 §四一致)：旁路文件 **不是**正本；人审接受后才覆盖。独立批跑入口即使提供 overwrite，也必须是显式 flag，并且失败时文件字节与原文相同 (Gvozdev `rewrite-md.sh:24`)。

Gvozdev overwrite 的 HTML marker (`rewrite-md.sh:79`) 能防二次咀嚼，但会污染文档。本仓若做幂等，宜用「旁路已存在且原文未变则跳过」，不要往 `docs/` 里插 `<!-- claudish-to-english:rewritten -->`。

### 5.4 怎么用黄金 fixture 防回归结构不变量

润色的散文 **不能**放进 `spec/fixtures/*.fixed`：模型非确定、允许减句，与「`.in` 与 `.fixed` 逐行对齐」(`spec/README.md`「约定」) 冲突。黄金集要锁的是 **保护器**，不是改写风格。

不变量清单 (应与 `spec/rules.md`「全局豁免」对齐，再加表格骨架)：

| 不变量 | 测什么 | 本仓已有可复用的 case |
| --- | --- | --- |
| 围栏代码块 | 定界行 + 内部字节级相同 (含 ` ``` ` 与 `~~~`) | `fenced-code.in`、`fenced-code-clean.in` |
| 行内代码 | span 内部字节级相同，定界反引号数量不变 | `inline-code-spans.in`、`inline-code-doc.in` |
| 链接 destination / 裸 URL | `](…)` 内与 `http(s)://` 串字节级相同；锚文字允许改 | `url-protection.in` |
| 含假名引用 span | 「」『』《》内含假名则整段不动 | `kana-quote-span.in` |
| 表格骨架 | 行数、每行列数、对齐分隔行的 `|` / `-` 结构不变；单元格散文允许改 | `span-not-across-table-rows.in` (现测的是 span 截断，不是润色；可当骨架样例) |
| YAML frontmatter | 整块字节级相同 | 本仓黄金集目前没有 frontmatter case，要补合成一份 |
| 标题标记 | 行首 `#` 的个数不变；标题散文允许改 | 可新写合成 case |

**怎么测，而不把 LLM 放进 CI：**

抄 Gvozdev 的 `CLAUDISH_STUB=1` (`rewrite.sh:279`)：保护器先把不变量撕成占位，再跑一个 **确定性的破坏桩** (例如把所有可见散文换成 `X`，或整段大写)，再嵌回。黄金断言是「桩跑完，不变量区域仍与 `.in` 字节相同」。这样测的是解析 / 占位 / 嵌回，不测 Gemma 会不会听话。

第二道闸发生在真模型路径上，但仍是确定性的：润色产出旁路之后，用同一套豁免解析器对 `.in` 与旁路抽不变量，不相等就 fail-open。这道闸可以留在 skill / 独立入口里，**不必**进 `lo-md-lint` 的 required check。

**不要**让模型「自己保证围栏不动」而不做机械剥离。Gvozdev 只机械保护了 frontmatter，围栏仍送给模型；`tldr` 还会叫它删围栏。本仓已经有豁免解析，保护器应 **先撕后嵌**，prompt 里的 “Leave fenced code blocks unchanged” 只当皮带，不当唯一安全带。

现有 runner 的三条断言 (`spec/README.md`「runner 的判定」) 仍然只服务 lint。润色保护器是第四类测试，断言形状不同 (区域字节相等，而不是行号 + 规则 id)。可以仍放 `spec/fixtures/` 用新后缀，也可以放实现自己的测试；ADR 要定的是「不变量清单与 fail-open」，不是目录。

### 5.5 与 heuristic learning 的接口

ADR-0005 §六要的是：pair 只 **提议**规则，人审后进 `spec/`，新规则必须跑全量黄金集。既有调研 §4.4 已说这不是现成产品，是运维模型。润色原型要预留的不是训练循环，是一份 **可蒸馏的 pair 记录**：

每条被接受的润色留下：

- before：润色前的原文 (未经 `--fix` 预处理)
- after：人审接受的文本 (已经过结构检查与 `zh-typography` 一族的 `--fix`)
- 可选：模型生输出 (人改之前)，用来看人改了什么
- 元数据：日期、模型名、用了哪一版 prompt、是否开过 experimental。**不要**写路径里的真实业务名；fixture / 语料只用 ACME / Foo (`AGENTS.md`「隐私边界」)

蒸馏方向 (从 pair 里能稳定长出的，才接 HL)：

| pair 里反复出现的编辑 | 可以提议成 | 不能提议成 |
| --- | --- | --- |
| 同一 `wrong` 在同类锚点旁被换成同一 `right` | zh-word-1 词表加一行 | 无锚点的全局替换 |
| 同一中文套话 / 黑话被删 | zh-tell-1 / zh-tell-3 词表加一行 | 「读着别扭」这种不可判句 |
| 同一英文 tell 被删 | en-tell-1 / en-tell-3 词表加一行 | Deng 词典里的合法技术词 |
| 多句压成一句、隐喻被拆开 | 无 (留在润色 spec) | 任何 findings 规则 —— 黄金集写不稳 (既有调研 §6) |

接口形状：润色侧 **只产出 pair**；提议规则是另一条 agent 任务，读 pair、写词表补丁 + 新 fixture 草案，跑全量黄金集，回归失败就丢这条提议。不要在润色进程里改 `spec/wordlists/`。这与翁家翌 HL 的「策略可读、可回归、可删」同构，也与 ADR-0007「词表是规范、加一条词就是改规范」同构。

未接受的旁路、fail-open 回退的原文、stub 测试的 `X` 填充，都不是 pair。

### 5.6 第一期范围 (给后续 ADR 的边界)

做：skill 驱动、旁路 `.plain.md`、机械保护不变量、润色后跑 `zh-typography` 一族的 `--fix`、人审后写回、留下 pair。

不做：CI required check、warning 计数触发、overwrite 默认开、把 Deng 词典当 zh-word-1、在 linter 里内置 oauth / 云厂商 SDK、用 `.fixed` 锁散文。

---

## 6. 不建议做的事 (判断)

既有调研 §7.3 的七条仍然成立。下面只补 **润色这条线** 的增量；不重复「不要嵌 Gvozdev / Deng 当依赖」「不要 LLM 当 CI 判定器」原文，只把它们在润色场景下写具体。

1. **不要把润色接进 `lo-md-lint --fix`。** `--fix` 的契约是逐行、唯一、不动点 (`spec/rules.md`「处理单位」)。语义压缩减句、非确定，接进去会拆掉黄金集。ADR-0006 已经把润色从 warning 触发上解开，不要从 fix 路径再耦回去。
2. **不要先 `--fix` 再润色。** 理由见 §4.3：模型会把 zh-typography-1 的全角标点改回去，也毁掉 HL 需要的 before。
3. **不要默认 overwrite 正本。** Gvozdev 自己把 overwrite 标成弱模型会毁文档 (`README.md:410`)，且没有人审闸。本仓 ADR-0005 §四已经要求旁路 + 人审后写回。
4. **不要只靠 prompt 保护围栏 / 行内代码 / 链接。** Gvozdev 对围栏就是这样，还被 `tldr` 反过来要求删除。本仓有豁免解析器，应机械剥离。
5. **不要把 Deng 的 `entries.json` 当可执行词表灌进 zh-word-1 或 en-tell-3。** 正向 spec 禁止机械换词；30 条里多数是合法技术用语；本仓 en-tell-3 已经用「造得出无修辞技术句就不收」筛过一轮 (`spec/rules.md` 的 en-tell-3)。词典的用法是给人读、给润色 spec 当例子，不是 `wrong = right`。
6. **不要用 Gvozdev 的 `tldr` / `5y` / `caveman` 当文档润色默认档。** `tldr` 省略围栏；`caveman` 删冠词、改时态，会改事实表面。文档润色要的是 Deng 那种 paraphrase，不是风格戏仿。
7. **不要抄 `CLAUDISH_ANTHROPIC_AUTH=oauth`。** 非官方、订阅凭证、本仓 public。Gvozdev 自己每会话警告一次 (`providers.sh:422`)。
8. **不要把润色做成 MessageDisplay 式的显示层插件。** 本仓的对象是仓库里的 Markdown 正本，不是 Claude Code 的屏幕。显示层改写解决不了 `docs/` 入库。
9. **不要用 `.in` / `.fixed` 去锁模型散文，也不要为了迁就润色放宽「不增删行」。** 那是 `zh-typography` 一族的契约；润色另测不变量。
10. **不要从未经人审的模型输出蒸馏规则。** HL 的反馈是人接受的 after；把生输出当 after，等于把一次幻觉写进 `spec/wordlists/`。
11. **不要为中文另做语言探测再切 prompt 包。** ADR-0006 §五：规则不分语言。润色 prompt 可以「中英两段都写上，模型按看到的文字用」，不要按文件路径猜语言。
12. **不要在原型里接 PAW / 私有 extra-index。** Deng 的价值是 spec 文本，不是 `paw.function("e469f61ccab2699fbd51")` 这条运行时依赖。function id 与 spec 是否一致无法从源码核实。

---

## 附录：关键引用 (均为 2026-08-31 对照本地 commit)

**Gvozdev `bf271f9`**

- 三只 hook：`hooks/hooks.json:3`、`:14`、`:25`
- fail-open 合同：`CLAUDE.md:29`，`rewrite.sh:27`，`rewrite-md.sh:22`
- 默认显示 prompt：`rewrite.sh:287`
- Markdown prompt：`rewrite-md.sh:198`
- frontmatter 机械保护：`rewrite-md.sh:146`
- 围栏只进长度门、仍送给模型：`rewrite.sh:222`
- provider 与默认模型：`providers.sh:88`
- oauth 警告：`providers.sh:422`
- 性能数字：`README.md:416`

**Deng `700588c`**

- CLI 与 function id：`translate.py:10`、`:17`
- spec 不进运行时：`translate.py` 全文不引用 `specs/` 或 `dictionary/`
- 禁止机械换词典：`specs/claudish-to-english.md:106`
- 保代码 / 专名：`specs/claudish-to-english.md:203`
- 允许减句：`specs/claudish-to-english.md:29`
- 词典字段与 25–60 条上限：`dictionary/validate.py:15`、`:130`

**本仓**

- 两段式：`docs/adr/0005-agent-native-positioning.md:30`
- HL 四步：`docs/adr/0005-agent-native-positioning.md:51`
- 润色不挂 warning：`docs/adr/0006-rule-grading-fixability-severity-experimental.md:68`
- 全局豁免：`spec/rules.md`「通用模型 / 全局豁免」
- 黄金集三条断言：`spec/README.md`「runner 的判定」
