# 中文技术文档排版规范盘点

> 来源：Grok research agent 调研，2026-08-31；distill 决策另行进 spec / ADR，本文只是证据。

调研日期：2026-08-31。只读，不改仓库。对照对象是本仓现有规则 R1 (CJK 旁半角标点转全角) / R2 (全角括号转半角) / R3 (半角括号外侧空格)，见 `spec/rules.md`。

**观点与事实分开**：带「建议」「宜」的句子是调研结论；规范原文与仓库现状是事实。

---

## 1. 用户线索核实：掘金译文排版规范

**事实。** 用户记得的那套规范存在，名字与位置如下。

| 项 | 值 |
| --- | --- |
| 社区名 | 掘金翻译计划 |
| 仓库 | [xitu/gold-miner](https://github.com/xitu/gold-miner) (约 34.3k star / 5k fork) |
| 规范名 | **译文排版规则指北** |
| 位置 | [Wiki 页](https://github.com/xitu/gold-miner/wiki/%E8%AF%91%E6%96%87%E6%8E%92%E7%89%88%E8%A7%84%E5%88%99%E6%8C%87%E5%8C%97) |
| 译者入门另写 | [如何参与翻译](https://github.com/xitu/gold-miner/wiki/%E5%A6%82%E4%BD%95%E5%8F%82%E4%B8%8E%E7%BF%BB%E8%AF%91) 第 5 条：「翻译为中文时排版请参考 **中文文案排版指北**」 |

**上游。** 指北正文几乎整段搬自 sparanoid 的《中文文案排版指北》，Wiki 文末参考文献却链到 fork [mzlogin/chinese-copywriting-guidelines](https://github.com/mzlogin/chinese-copywriting-guidelines)，不是正本。相对正本，掘金加了几条翻译场景特有的规则：

- 「以国标 GB/T 15834-2011 为基础」这一句
- **破折号前后需要增加一个空格** (`你好，我是破折号 —— 一个不苟言笑的符号。`)——sparanoid 正本**没有**这条
- 省略号用「一格三点、连续两格 `……`」，后接正文时再加一个空格；并链到 sparanoid issue [#58](https://github.com/sparanoid/chinese-copywriting-guidelines/issues/58)
- 斜体改加粗 (中文阅读体验)
- GitHub 脚注折中方案

**现状 (截至 2026-08-31)。** 基本停更。

- Wiki 该页最后编辑：2021-04-04，Hoarfroster，共 19 次修订
- 仓 `master` 最近一次提交：2024-01-21 (`db4f91a`)
- 2025 年仍有「申请成为译者」issue 打开，但没有对应的译文合入
- 仓库仍 public、仍挂在 README 教程列表里，规范文本可当历史正本读，不宜当「还在执行的社区法」

**观点。** 用户记忆准确：这就是那套。它不是独立发明，是 sparanoid 指北的翻译社区衍生版；破折号两侧空格是掘金自己加上的，不是上游共识。

---

## 2. 规范盘点

按「对 Markdown 技术文档 linter 的可执行性」排序。活跃度是 2026-08-31 抓到的公开数据。

### 2.1 技术社区文案层 (可直接 distill 成规则)

| 规范 | 维护方 | 链接 | 活跃度 | 定位 |
| --- | --- | --- | --- | --- |
| 《中文文案排版指北》 | [sparanoid](https://github.com/sparanoid/chinese-copywriting-guidelines) | [简体 README](https://github.com/sparanoid/chinese-copywriting-guidelines/blob/master/README.zh-Hans.md) | ~15.6k star / 1.8k fork；正文最近合入约 2023-08-09；dependabot 分支 2026-07 仍在动 | **事实：** 中英混排文案的事实标准。Apple / Microsoft 中港台站、V2EX、Ruby China、少数派被它列为实践者。 |
| 《中文技术文档的写作规范》 | 阮一峰 [ruanyf/document-style-guide](https://github.com/ruanyf/document-style-guide) | [文本](https://github.com/ruanyf/document-style-guide/blob/master/docs/text.md) / [标点](https://github.com/ruanyf/document-style-guide/blob/master/docs/marks.md) / [数值](https://github.com/ruanyf/document-style-guide/blob/master/docs/number.md) | ~12.7k star / 2.3k fork；95 commits；公共领域 | 写作规范 (标题层级、句长、语态) + 排版。标点更靠近国标。 |
| 译文排版规则指北 | 掘金翻译计划 | 见 §1 | 停更 | sparanoid 衍生 + 破折号空格 |
| 《中文技术文档写作风格指南》 | [yikeke/zh-style-guide](https://github.com/yikeke/zh-style-guide) | [在线](https://zh-style-guide.readthedocs.io/zh-cn/latest/) | 开源、章节完整 | 从 PingCAP / TiDB 中文文档经验长出来，标点专章写得比 sparanoid 细 (破折号明确「前后不空格」；括号按内容选全角/半角) |
| 百度 FEX Markdown 规范 | [fex-team/styleguide](https://github.com/fex-team/styleguide/blob/master/markdown.md) | 同上 | 标注「还未定稿」 | 中英数字加空格；中文用直角引号「」；括号内有中文用全角、全英文用半角 |

### 2.2 国家标准与排版需求 (权威，但不等于 Markdown 空格规则)

| 规范 | 维护方 | 链接 | 活跃度 | 定位 |
| --- | --- | --- | --- | --- |
| GB/T 15834-2011《标点符号用法》 | 国家标准 | 教育部 PDF 镜像 (ruanyf [参考链接](https://github.com/ruanyf/document-style-guide/blob/master/docs/reference.md) 列出)；W3C 有 [第 5 章英译](https://www.w3.org/zh-hans/news/2013/translation-of-gb-t-15834-2011-part-5/) | 现行 (2012-06-01 实施) | **标点字形、占位、行首行尾禁则**。破折号占两字、居中、不断开；引号形式是弯引号 “”；**不讨论中英文之间要不要 U+0020**。 |
| GB/T 15835《出版物上数字用法》 | 国家标准 | ruanyf 参考链接 | 现行 | 半角数字、千分位、百分号写法 |
| W3C《中文排版需求》(clreq) | [w3c/clreq](https://github.com/w3c/clreq/) 中文布局任务团 | [TR](https://www.w3.org/TR/clreq/)；本稿 2026-08-04 Group Note Draft | **仍在维护** | 给 UA / 字体 / CSS 的排版需求，不是文案手册。中西间距原则是 **不多于 1/4 em 的字距或空白**，行首行尾不加；点号旁、开括号后、闭括号前的西文不加空白。也承认可用 U+0020，宽度随字体。 |

**观点。** clreq 的「四分空」是印刷/CSS 层，linter 往源码里插 U+0020 是社区对它的工程近似，不是 clreq 原文要求。R1–R3 这种「改 Markdown 源码」的工具，应对齐 sparanoid / 阮一峰 / 掘金这一层，把 clreq 当「为什么看起来要留白」的背景，不要把 1/4 em 写成可执行规则。

### 2.3 厂牌与其它公开规范

公开、能核对到正文的：

| 来源 | 链接 | 与本仓相关的立场 |
| --- | --- | --- |
| Apple / Microsoft 中港台官网实践 | sparanoid「谁在这样做？」表 | 中英文之间加空格 (文案层，非开源规范文本) |
| LeanCloud 文档风格指南 | ruanyf 参考链 [open.leancloud.cn/copywriting-style-guide.html](https://open.leancloud.cn/copywriting-style-guide.html) (2026-08-31 未重新抓到正文，历史引用多) | 被 sparanoid / Teambition / 多份指南当作实践来源 |
| 豌豆荚文案风格指南 | Google Doc，ruanyf 列出 | 历史引用；2026-08-31 未核正文 |
| 华为《产品手册中文写作规范》 | 商业文档，ruanyf 经 taodocs 引用 | 权威但非开放 |
| Teambition copywriting | [alibaba-archive/standard 衍生](https://github.com/teambition/standard/blob/master/copywriting-style-guide.md) | 品牌大小写 + 引用 LeanCloud / sparanoid |
| TapTap TDS | [blog.taptap.dev/pages/chinese-copywriting-guide](https://blog.taptap.dev/pages/chinese-copywriting-guide) | 中英/数字空格；直角引号「」『』 |
| 腾讯广告用户体验规范 | 广告语用字，不是技术文档 | 不建议当 linter 来源 |
| [chen3feng/cn-doc-style-guide](https://github.com/chen3feng/cn-doc-style-guide) | 译 Google 文档规范 + 自带检查工具 | 小仓，指向阮一峰 / sparanoid / GB |

**观点。** 厂牌「规范」能公开到可执行粒度的，几乎都是 sparanoid 的再发布。没有找到阿里 / 腾讯 / 字节对外的、与 sparanoid 平级的中文 Markdown 排版规范。华为那份被广泛引用，但不是开放文本，不宜作为本仓规则的规范性来源。

### 2.4 本仓已选的家规 (事实，不是业界共识)

全局守则与本仓 `AGENTS.md` 已经写死：

- 中文行文：全角逗号句号顿号分号冒号
- **括号用半角 `()`，外侧空格、内侧不留**

这与 R2 + R3 一致，与 GB / 阮一峰 / sparanoid 示例里的全角括号 `（）` **相反**，与 zhlint 默认、AutoCorrect `space-bracket`、yikeke「括号内全英文用半角」**同向**。后面 distill 时把 R2/R3 当已拍板的家规，不建议用国标去推翻。

---

## 3. 规则矩阵

立场编码：**要求** / **建议** / **不提** / **反对** (明确写了相反做法)。工具列的是默认行为，不是规范立场。

「链接」列给该条最直接的依据；获取日期均为 2026-08-31。

### 3.1 中英文之间空格

| 来源 | 立场 | 依据 |
| --- | --- | --- |
| sparanoid | 要求 | [中英文之间需要增加空格](https://github.com/sparanoid/chinese-copywriting-guidelines/blob/master/README.zh-Hans.md) |
| 掘金指北 | 要求 | 同上结构 |
| 阮一峰 | 要求 | [文本 § 字间距 (1)](https://github.com/ruanyf/document-style-guide/blob/master/docs/text.md) |
| clreq | 建议 (≤1/4 em 字距，也可用 U+0020) | [横排中西文混排](https://www.w3.org/TR/clreq/) |
| GB/T 15834 | 不提 | — |
| yikeke | 要求 (半角英文用半角空格包围，右侧遇标点省略) | [空白符号](https://zh-style-guide.readthedocs.io/zh-cn/latest/%E6%96%87%E6%A1%A3%E5%86%85%E5%AE%B9%E5%85%83%E7%B4%A0/%E7%A9%BA%E7%99%BD%E7%AC%A6%E5%8F%B7.html) |
| AutoCorrect | 默认开 `space-word: 1` | [.autocorrectrc.default](https://github.com/huacnlee/autocorrect/blob/main/autocorrect/.autocorrectrc.default) |
| zhlint | 默认开 `spaceBetweenMixedwidthContent: true` | [README.zh-CN 规则](https://github.com/zhlint-project/zhlint/blob/main/README.zh-CN.md) |
| pangu.js | 只做这件事 | [README](https://github.com/vinta/pangu.js) |
| 本仓 R1–R3 | 不提 | — |

**共识：广泛。** 技术文档社区几乎无异议。产品专名按官方写法例外 (「豆瓣FM」)。

### 3.2 数字与中文之间空格

| 来源 | 立场 | 依据 |
| --- | --- | --- |
| sparanoid / 掘金 | 要求 (`5000 元`) | 指北「中文与数字」 |
| 阮一峰 | **建议统一，两种都可** | 文本 § 字间距 (2) |
| clreq | 同中西间距 (数字视同西文) | clreq 混排节 |
| yikeke | 要求 | 空白符号 |
| zhlint | 开，但 `skipZhUnits: 年月日天号时分秒` 例外 (`` `2011年` `` 不加) | README `skipZhUnits` |
| AutoCorrect | `space-word` 覆盖 (`于 3 月 10 日`) | README 示例 |
| 本仓 | 不提 | — |

**有分歧。** sparanoid 一刀切加空格；阮一峰允许 `` `2011年5月15日` ``；zhlint / AutoCorrect 对中文计量单位 (年月日天) 倾向不加。

### 3.3 数字与单位、百分号的空格

| 来源 | 立场 | 依据 |
| --- | --- | --- |
| sparanoid / 掘金 | 要求：`10 Gbps`、`20 TB`；**例外** `90°`、`15%` 不加 | 指北「数字与单位」 |
| 阮一峰 | 要求：英文单位前留空隙 (`16 GB`)；百分号「视同阿拉伯数字」，与中文之间可加可不加 | 文本 § 字间距 (3) (2) |
| GB 3100 / 15835 | 数值与单位之间有空隙；% 通常紧贴数字 | ruanyf 参考链接 |
| textlint-rule-zh-no-space-between-num-and-unit-symbol | 要求去掉数字与单位符号之间的空格 | [preset README](https://github.com/darkyzhou/textlint-rule-preset-zh-technical-writing) (注意：它说的是 **单位符号** 如 `%` `°`，不是 `GB`) |
| 本仓 | 不提 | — |

**共识：广泛，带两条硬例外。** ASCII 单位 (`GB` `ms` `px`) 要空格；`%` 和 `°` 不要。`km/h` 这类带斜杠的单位，yikeke 写斜杠两旁不加空格。

### 3.4 半角括号外侧空格

| 来源 | 立场 | 依据 |
| --- | --- | --- |
| 本仓 R3 | **要求** (家规) | `spec/rules.md` |
| yikeke | 括号内**全英文**：半角括号 + 外侧空格 (`数据定义语言 (DDL)`)；括号内有中文：全角、外侧不加 | [中英文混用时标点](https://zh-style-guide.readthedocs.io/zh-cn/latest/%E6%A0%87%E7%82%B9%E7%AC%A6%E5%8F%B7/%E4%B8%AD%E8%8B%B1%E6%96%87%E6%B7%B7%E7%94%A8%E6%97%B6%E6%A0%87%E7%82%B9%E7%94%A8%E6%B3%95.html) |
| zhlint | 默认 `spaceOutsideHalfwidthBracket: true` | README |
| AutoCorrect | 默认 `space-bracket: 1` | `.autocorrectrc.default` |
| 阮一峰 | **反对** (用全角括号、前后不加空格) | 标点 § 括号：`请确认所有的连接（电缆和接插件）均安装牢固。` |
| sparanoid / 掘金 | 示例用全角括号、外侧不加 (`核磁共振成像（NMRI）`) | 指北「使用全角中文标点」 |
| GB/T 15834 | 全角括号、各占一字、外侧无 U+0020 | 标准 5.1.3 |
| fex-team | 有中文用中文括号；全英文用半角 | markdown.md |

**有分歧，且本仓已经站队。** 国标/阮/sparanoid = 全角无空格；技术工具链与 yikeke 的「全英文括注」= 半角 + 外侧空格。R2+R3 是后者。

### 3.5 全角 vs 半角标点 (逗号句号顿号冒号分号问叹)

| 来源 | 立场 | 依据 |
| --- | --- | --- |
| 全体中文规范 | 中文句子用全角 `，。、：；？！` | GB / 阮 / sparanoid / 掘金 / yikeke |
| 阮一峰 / sparanoid | 整句英文、英文专名内部用半角 | 阮 标点原则 (2)；sparanoid「完整英文整句」 |
| 阮一峰 | 并列的英文词在中文句子里仍用顿号 `Google、Facebook、腾讯` | 标点 § 顿号 |
| 本仓 R1 | 只转 `, ; : ? !`，**不转 `.` → `。`**，也不管顿号 | `spec/rules.md` |
| AutoCorrect `fullwidth` | 默认开，CJK 旁标点转全角 | README Features |
| zhlint `fullwidthPunctuation` | 默认 `，。：；？！“”‘’` | README |

**共识：广泛。** 本仓缺口是句号 `.` 和顿号 (英文逗号并列)。R1 现在放过 `$1,000` 和 `https://example.com:8080`，这个例外所有工具都需要，应保留。

### 3.6 引号选型 (「」 vs “”)

| 来源 | 立场 | 依据 |
| --- | --- | --- |
| GB/T 15834-2011 | 横排用弯引号 “” ‘’ (直排才改「」) | 标准 4.8；yikeke 也如此引用 |
| 阮一峰 | 要求弯引号 “” | 标点 § 引号 |
| sparanoid | **争议**：简体可用直角「」 | 指北「争议」节，明确「从语法角度都正确」 |
| 掘金 | 争议；文末写「目前建议使用」弯引号那组 | Wiki 争议节 (文本自相矛盾：示例全是「」，建议句指向 “”) |
| TapTap TDS / fex-team | 要求直角「」『』 | TDS 文案指南；FEX markdown.md |
| zhlint | 默认 `unifiedPunctuation: 'simplified'`，`「」` → `“”` | README |
| clreq | 地区差：大陆横排弯引号，台港直角 | clreq 1.2 / 标点附录 |
| 本仓 | 不提 | — |

**有分歧。** 国标与阮一峰 = “”；港台与一部分大陆技术团队 = 「」。sparanoid 自己把它放进「争议」。不适合做默认强制转换。

### 3.7 破折号 (——) 两侧空格 —— brief 点名的问题

| 来源 | 立场 | 依据 |
| --- | --- | --- |
| GB/T 15834-2011 | **反对空格**：占两字、居中、不断开；示例 `……日本中年人——内山老板……` | 4.10 / 5.1.4 |
| clreq | 不提空格；用 U+2E3A 或两个 U+2014，占两字、不断开 | [附录 A.2](https://www.w3.org/TR/clreq/) |
| 阮一峰 | **条件**：占两字则无空格；若自身只占一字，则前后半角空格 | 标点 § 破折号，两个例句并列 |
| yikeke | **反对空格** (`新的数据库概念——NewSQL`) | [常用中文标点 · 破折号](https://zh-style-guide.readthedocs.io/zh-cn/latest/%E6%A0%87%E7%82%B9%E7%AC%A6%E5%8F%B7/%E5%B8%B8%E7%94%A8%E4%B8%AD%E6%96%87%E6%A0%87%E7%82%B9%E7%AC%A6%E5%8F%B7.html) |
| sparanoid | 不提 | — |
| **掘金指北** | **要求两侧空格** (`破折号 —— 一个`) | Wiki「破折号前后需要增加一个空格」 |
| AutoCorrect `space-dash` | 针对 ASCII `-`，不是 `——`；默认文件为 `1`，README 示例写 `0`，社区配置常关 | [.autocorrectrc.default](https://github.com/huacnlee/autocorrect/blob/main/autocorrect/.autocorrectrc.default)；[zotero-chinese 关了它](https://github.com/zotero-chinese/configs) |
| 英文 AP vs Chicago | AP 两侧空格；Chicago 无空格 | 英文排版，不是中文规范 |

**有分歧，而且掘金是少数派。** 中文排版传统与国标都是「两字宽、紧贴、中间不断」。掘金那条更像把英文 AP 的 em dash 习惯带进译文。阮一峰的「占一字才加空格」是对字体里 U+2014 往往不够两字宽的工程补丁，不是「源码里插 U+0020」的一般要求。

**观点。** 若做规则：默认关；若开，只处理 `——` (两个 U+2014) / `⸺` (U+2E3A)，不要和 ASCII `-`、en dash `–`、范围号 `～` 混在一起。与 R1 无冲突。与「全角标点旁不加空格」在哲学上冲突 (破折号是全角标号)。

### 3.8 省略号 (……)

| 来源 | 立场 | 依据 |
| --- | --- | --- |
| GB / clreq / 阮 / yikeke | 要求两个 U+2026 (或 U+22EF)，六点、占两字；禁止 `...` `。。。` | 阮 标点 § 省略号；yikeke 同 |
| 掘金 | 同上，并：后接内容时加一个空格；链 sparanoid #58 | Wiki |
| sparanoid | 不提 (issue #58 讨论过，未进正文) | — |
| zhlint | 统一 `…` / `⋯` | `unifiedPunctuation` |
| 本仓 | 不提 | — |

**共识：字形广泛；后侧空格仅掘金。** 「不要与『等』连用」是写作规则，linter 很难做对。

### 3.9 专有名词大小写

| 来源 | 立场 | 依据 |
| --- | --- | --- |
| sparanoid / 掘金 | 要求正确大小写 (GitHub 不是 Github) | 指北「名词」 |
| 阮一峰 | 专有名词每个词首字母大写 | 文本 § 英文处理 (6) |
| AutoCorrect | 实验性 `spellcheck`，默认 **关** (`0`)；可配词表 | `.autocorrectrc.default` |
| textlint-rule-terminology / terminology-zh | 词表驱动 | [npm](https://www.npmjs.com/package/textlint-rule-terminology-zh) |
| 本仓 | 不提 | — |

**共识：应该做对；工具层必须靠词表，不能靠规则。** 不适合作为无词表的默认规则。

### 3.10 链接 / 行内代码与中文之间的空格

| 来源 | 立场 | 依据 |
| --- | --- | --- |
| sparanoid / 掘金 | **争议**：链接两侧加空格 | 指北「争议」；掘金加「同一篇文章风格要一致」 |
| zhlint | 默认 `spaceOutsideCode: true` (行内代码两侧空格) | README |
| AutoCorrect | 默认 `space-backticks: 1` | `.autocorrectrc.default` |
| textlint-rule-zh-space-around-inline-code | 要求行内代码两侧空格 | preset 表 |
| pangu.js | **明确不要拿它处理 Markdown** | README：「You SHOULD NOT use pangu.js to spacing Markdown documents」issue #127 |
| 本仓 | 行内代码 span **内部**豁免；定界反引号算正文，所以 `` `code`(x) `` 仍报 R3 | `spec/rules.md` 全局豁免 |

**有分歧。** 行内代码两侧空格在工具里是主流默认；链接两侧空格被规范自己标成争议 (Markdown `[文字](url)` 渲染后链接文字与中文是否留白，和源码里 `](` 语法空格是两件事)。本仓 R3 已经处理「反引号与括号」的边界，但不管「中文与 `` `code` `` 之间」。

---

## 4. 同类工具先例

### 4.1 AutoCorrect ([huacnlee/autocorrect](https://github.com/huacnlee/autocorrect))

Rust CLI + 多语言 SDK + LSP + VS Code + GitHub Action。约 1.6k star。最新 release v2.16.3 (2026-01-03)。MDN 中文、Rust Book CN、Ruby China 在用。

**规则 (`.autocorrectrc.default`，0=关 / 1=error / 2=warning)：**

| 规则 id | 默认 | 含义 |
| --- | --- | --- |
| `space-word` | 1 | CJK 与英文/数字之间加空格 |
| `space-punctuation` | 1 | 部分半角标点后加空格 |
| `space-bracket` | 1 | `()` `[]` 靠近 CJK 时外侧加空格 |
| `space-backticks` | 1 | 行内代码靠近 CJK 时加空格 |
| `space-dash` | 1 (README 示例却写 0) | ASCII `-` 两侧空格；**不是**中文破折号 |
| `space-dollar` | 0 | `$` 靠近 CJK |
| `fullwidth` | 1 | CJK 旁标点转全角 |
| `no-space-fullwidth` | 1 | 全角标点旁去空格 |
| `no-space-fullwidth-quote` | 1 | “” 旁去空格 |
| `halfwidth-word` | 1 | 全角字母数字转半角 |
| `halfwidth-punctuation` | 1 | 英文语境全角标点转半角 |
| `spellcheck` | 0 | 词表纠错 (实验) |
| `context.codeblock` | 1 | 是否处理 Markdown 代码块 |

**值得抄：**

1. **每条规则独立 id + 三级开关 (off / error / warning)**，和本仓 ADR-0002「flag 化」同构。
2. 配置发现：项目根 `.autocorrectrc` (YAML/JSON) + JSON Schema。
3. `textRules`：对具体字符串覆盖严重级别 (产品名「豆瓣FM」这种)。
4. 行内 `autocorrect-disable` / `autocorrect-disable space-word` / `autocorrect-enable`。
5. `.autocorrectignore` 跟 `.gitignore` 同语法；默认也尊重 `.gitignore`。
6. 按文件类型 AST 只扫字符串和注释，不碰代码 token。
7. `--lint` 出 diff / JSON / rdjson (reviewdog)。

**不要抄：** 把 `space-dash` 默认开——连官方 README 示例都写成 0，社区配置也常关。`spellcheck` 默认关是对的。

### 4.2 zhlint ([zhlint-project/zhlint](https://github.com/zhlint-project/zhlint))

TypeScript，约 1k star，2026-06 仍有提交。规则提炼自 W3C clreq、HTML 中文兴趣组、Vue.js 中文文档翻译。作者自己写「这些规则也许存在争议」。

**设计：** 不是「规则 id + on/off」，而是 **一份 `RuleOptions` 对象**，每个开关是 `true` / `false` / `undefined` (undefined = 不动)。另有 `.zhlintrc`、`.zhlintignore`、`.zhlintcaseignore`。

默认与本仓高度同向：`halfwidthPunctuation: '()'` (全角括号变半角，即 R2)、`spaceOutsideHalfwidthBracket: true` (即 R3)、`fullwidthPunctuation` 含 `，。：；？！` (R1 的超集，还含句号和弯引号)。

**值得抄：**

1. `skipZhUnits: '年月日天号时分秒'`——数字规则的逃生口。
2. `skipAbbrs: ['Mr.','Dr.','e.g.',...]`——别把缩写的点转成 `。`。
3. `skipPureWestern: true`——整行英文不处理 (R1 已有「两侧都不是 CJK 则不动」)。
4. HTML 注释 `<!-- zhlint ignore: ( , ) -->` 与 `<!-- zhlint disabled -->`。
5. Markdown / Hexo tag 预解析，只格式化可见文本。

Rust 移植 `zhlint-rs` 基本停在 2024-02，不要当活上游。

### 4.3 pangu.js ([vinta/pangu.js](https://github.com/vinta/pangu.js))

「盘古之白」。约 4.8k star，2026-06 仍在发版。只做 CJK 与半角字母数字符号之间插空格，**不做标点全半角转换**。Chrome 插件起家。作者明确：**不要用来 spacing Markdown**。

对 linter 的价值是文化源头 (sparanoid 开篇就引用它) 和「空格规则的最小核」。多语言移植一大串。配置面几乎没有——它不是 linter。

### 4.4 textlint 中文插件

[textlint/textlint](https://github.com/textlint/textlint) 是日文社区的文本 lint 框架。中文侧没有官方大一统 preset，比较完整的是 [darkyzhou/textlint-rule-preset-zh-technical-writing](https://github.com/darkyzhou/textlint-rule-preset-zh-technical-writing)：

| 包 | 做什么 |
| --- | --- |
| zh-space-between-zh-and-en-or-num | 汉字与英文/数字之间空格 |
| zh-space-around-inline-code | 行内代码两侧空格 |
| zh-no-space-around-zh-punct | 中文标点旁去空格 |
| zh-no-space-between-num-and-unit-symbol | 数字与 `%` `°` 等单位符号之间去空格 |
| zh-double-zh-ellipsis | 省略号形态 |
| zh-no-redundant-punctuation | 叠用标点 |
| zh-correctly-ordered-pairs | 引号书名号成对 |
| terminology | 英文术语词表 |

另有 `textlint-rule-zh-half-and-full-width-bracket` (三种模式：一律全角 / 一律半角 / 有中文则全角否则半角)——正好覆盖 §3.4 的分歧，可当 R2 的配置模型。

**值得抄：** 一条规则一个包、`--fix` 可标、preset 只是打包。本仓不必做成 textlint 插件，但「括号策略三选一」值得做成 flag，而不是把 R2 写死。

### 4.5 其它

- [jxlwqq/chinese-typesetting](https://github.com/jxlwqq/chinese-typesetting) PHP
- [ricoa/copywriting-correct](https://github.com/ricoa/copywriting-correct) / satouriko fork：按 sparanoid 纠空格和标点
- [hotoo/pangu.vim](https://github.com/hotoo/pangu.vim)
- prettier-plugin-autocorrect：把 AutoCorrect 接到 Prettier

---

## 5. 对照 R1–R3 的 distill 建议

先重复家规事实：本仓已经用 R2+R3 选择了「半角括号 + 外侧空格」，与 AGENTS.md 一致。下面不建议回退这条，除非用户改家规。

### 5.1 建议 distill (默认开)

| 候选 | 内容 | 共识 | 与 R1–R3 |
| --- | --- | --- | --- |
| CJK–拉丁空格 | CJK 与 `A-Za-z` 之间补一个半角空格；产品官方写法豁免 (textRules / 词表) | 广泛 | 无冲突。R3 的「word 字符」定义已含 CJK 与 ASCII 字母数字，实现时可复用 |
| 全角标点旁去空格 | `，。、；：？！` 两侧不要 U+0020 | 广泛 | 无冲突。修复顺序建议放在 R3 之后，免得刚补的括号空格被误删 (全角标点 ≠ 半角括号) |
| 半角数字 | `０-９` → `0-9` (设计稿海报例外，linter 可不管) | 广泛 | 无冲突 |
| 英文单位前空格 | `16GB` → `16 GB`；`%` `°` 紧贴数字 | 广泛 (带例外) | 无冲突。`$1,000` 现有 R1 豁免应继续 |
| 行内代码两侧空格 | 中文与 `` `code` `` 之间补空格；围栏块仍整块豁免 | 工具主流默认；规范层 sparanoid 把「链接」标争议、没单独写代码 | 与全局豁免兼容：豁免的是 span **内部**，定界符外侧正是该管的。和 R3 对 `` `code`(x) `` 的处理同方向 |
| R1 补句号 | CJK 旁的 `.` → `。`，继续豁免 `e.g.` `Dr.` `1.2.3` URL | 广泛 (R1 现在漏了句号) | R1 扩展，不是新哲学。必须带 `skipAbbrs` |

### 5.2 建议默认关 (实现可做，flag 默认 off)

| 候选 | 内容 | 为何关 |
| --- | --- | --- |
| CJK–数字空格 | `` `12 个月` `` vs `` `12个月` `` vs `` `2011年` `` | 阮一峰明确「两种都行、要统一」；zhlint 对年月日例外。一刀切开会跟大量「2016年」文档打架 |
| 链接两侧空格 | `[文字](url)` 渲染文字与中文之间 | sparanoid / 掘金自己标「争议」 |
| 直角/弯引号统一 | 「」 ↔ “” | 国标 vs 港台 vs 技术团队，三套都有拥趸 |
| 破折号两侧空格 | `——` 左右 U+0020 | **见下节** |
| 专有名词大小写 | GitHub / iOS / npm | 必须词表；AutoCorrect 也默认关 |
| 禁止叠用 `！！` `………` | 写作风格 | 规范有、误报少，但属于风格不是排版对错；可做 warning |
| ASCII `-` 两侧空格 (`space-dash`) | `中文 - 英文` | AutoCorrect 自己都动摇；会打到 YAML、命令行、范围 |

### 5.3 待用户拍板

**1. 破折号两侧空格该不该做。**

- 业界怎么做：**国标、clreq、yikeke、传统印刷 = 不加**；**掘金译文指北 = 加** (这是用户记得的那条)；阮一峰 = 看它占几格；sparanoid = 不提。
- 工具：没有主流 linter 默认给 `——` 加空格。AutoCorrect 的 `space-dash` 管的是 ASCII 连字符。
- 建议：做成独立 flag，**默认关**。若用户认掘金家规，再打开。打开时只认 `——` / `⸺`，并且与「全角标点旁去空格」互斥 (开破折号空格就不要去它旁边的空格)。
- 不要把「两个 hyphen `--`」自动变成破折号——那是输入法/编辑器的事，Markdown 里 `--` 经常是 HTML 注释或 option。

**2. 括号策略要不要做成三档，而不是写死 R2。**

textlint-rule-zh-half-and-full-width-bracket 的三档：一律半角 (当前 R2) / 一律全角 (国标、阮、sparanoid) / 有中文全角、纯英文半角 (yikeke、FEX)。家规目前是第一档。flag 化之后，默认仍可保持 R2 开，但给「要对齐国标」的用户一条退路。R2 关时 R3 也应一起关，否则全角括号没有外侧空格规则、半角又不管了。

**3. 数字与中文空格 + 中文单位例外。**

若做，建议抄 zhlint：`skipZhUnits = 年月日天号时分秒`，默认关或 warning。默认开会和「2011年5月15日」这种阮一峰也认的写法冲突。

**4. 顿号 vs 英文逗号。**

阮一峰要求中文句子里并列英文词用 `、` 不是 `,`。R1 现在会把 `Google, Facebook` 在 CJK 旁边的逗号转成 `，` (全角逗号)，**不是**顿号。要不要再转成顿号是语义问题 (并列 vs 句内停顿)，机器很难稳。建议先不做。

### 5.4 建议不做

| 项 | 理由 |
| --- | --- |
| 用 pangu 当 Markdown fixer | 作者禁止 |
| 抄 clreq 的 1/4 em 字距 | 源码层无法表达，是渲染层 |
| 默认开 spellcheck / 术语表 | 无默认词表就会乱改；可后续当可选插件 |
| 斜体改加粗 | 掘金场景 (GitHub 渲染中文斜体难看)，不是通用排版法；会破坏 Markdown 语义 |
| 整句英文检测后改半角标点 | AutoCorrect `halfwidth-punctuation` 有，但「何为整句」边界糊，误伤中文里的引用 |

### 5.5 若做新规则，配置形态建议抄谁

优先抄 AutoCorrect，贴合 ADR-0002：

- 规则 id 稳定 (延续 R4…)
- 每条 `off | error | warning` (或本仓简化成 off/on，warning 以后再加)
- 项目配置 `[tool.lo-md-lint]` / 独立 toml
- 行内 disable 注释
- 字符串级豁免 (豆瓣FM、`401(k)` 已经在 R3)
- Markdown 感知：围栏块、行内代码——本仓已有，保持

从 zhlint 只抄例外列表 (`skipZhUnits`、`skipAbbrs`)，不抄它那份巨大的 boolean options 对象——和「规则 id 稳定不复用」的本仓模型不合。

修复顺序现状是 R2 → R1 → R3。若加入「CJK 空格」和「全角标点去空格」，建议：

1. 括号全半角 (R2)
2. 标点全半角 (R1，含句号)
3. 括号外侧空格 (R3)
4. CJK–拉丁 / 代码空格
5. 全角标点旁去空格 (最后收，避免 4 插在 `` `你好，world` `` 里变成 `` `你好， world` ``)

---

## 6. 三档结论 (给 orchestra)

### 建议 distill

1. **CJK 与拉丁字母之间空格** (默认开)
2. **全角标点两侧去空格** (默认开)
3. **全角数字转半角** (默认开)
4. **数字与 ASCII 单位之间空格，`%` `°` 除外** (默认开)
5. **行内代码与中文之间空格** (默认开)
6. **R1 补 CJK 旁句号 `.` → `。`，带缩写豁免** (默认开，算 R1 修完而不是新哲学)

### 建议不做 (默认不做，也不先做 flag)

1. 用 pangu 处理 Markdown
2. 专有名词大小写 (无词表)
3. 斜体改加粗
4. 顿号语义替换
5. `--` → `——` 的输入法级替换
6. clreq 四分空

### 待用户拍板

1. **破折号 `——` 两侧空格**：掘金要求、国标反对。建议独立 flag、默认关。这是 brief 里最需要一句话裁决的项。
2. **括号策略三档** vs 维持死 R2 (一律半角)。家规已选半角；flag 化时要不要给国标党退路。
3. **数字与汉字之间空格** (含「年月日」例外)：默认关还是 warning。
4. **引号「」 vs “”**：统一转换还是不管。
5. **链接两侧空格**：规范自己标争议。

---

## 7. 来源 (URL + 获取日期 2026-08-31)

- [xitu/gold-miner](https://github.com/xitu/gold-miner)
- [译文排版规则指北 (Wiki)](https://github.com/xitu/gold-miner/wiki/%E8%AF%91%E6%96%87%E6%8E%92%E7%89%88%E8%A7%84%E5%88%99%E6%8C%87%E5%8C%97) (raw: `https://raw.githubusercontent.com/wiki/xitu/gold-miner/译文排版规则指北.md`)
- [如何参与翻译](https://github.com/xitu/gold-miner/wiki/%E5%A6%82%E4%BD%95%E5%8F%82%E4%B8%8E%E7%BF%BB%E8%AF%91)
- [sparanoid/chinese-copywriting-guidelines README.zh-Hans.md](https://github.com/sparanoid/chinese-copywriting-guidelines/blob/master/README.zh-Hans.md)
- [mzlogin/chinese-copywriting-guidelines](https://github.com/mzlogin/chinese-copywriting-guidelines) (掘金参考文献指向的 fork)
- [ruanyf/document-style-guide](https://github.com/ruanyf/document-style-guide) 及 [text.md](https://github.com/ruanyf/document-style-guide/blob/master/docs/text.md) / [marks.md](https://github.com/ruanyf/document-style-guide/blob/master/docs/marks.md) / [number.md](https://github.com/ruanyf/document-style-guide/blob/master/docs/number.md) / [reference.md](https://github.com/ruanyf/document-style-guide/blob/master/docs/reference.md)
- [W3C clreq](https://www.w3.org/TR/clreq/) (DNOTE 2026-08-04)
- [GB/T 15834-2011 第 5 章英译 (W3C)](https://www.w3.org/zh-hans/news/2013/translation-of-gb-t-15834-2011-part-5/)
- [yikeke/zh-style-guide](https://github.com/yikeke/zh-style-guide) / [空白符号](https://zh-style-guide.readthedocs.io/zh-cn/latest/%E6%96%87%E6%A1%A3%E5%86%85%E5%AE%B9%E5%85%83%E7%B4%A0/%E7%A9%BA%E7%99%BD%E7%AC%A6%E5%8F%B7.html) / [常用中文标点](https://zh-style-guide.readthedocs.io/zh-cn/latest/%E6%A0%87%E7%82%B9%E7%AC%A6%E5%8F%B7/%E5%B8%B8%E7%94%A8%E4%B8%AD%E6%96%87%E6%A0%87%E7%82%B9%E7%AC%A6%E5%8F%B7.html) / [中英文混用标点](https://zh-style-guide.readthedocs.io/zh-cn/latest/%E6%A0%87%E7%82%B9%E7%AC%A6%E5%8F%B7/%E4%B8%AD%E8%8B%B1%E6%96%87%E6%B7%B7%E7%94%A8%E6%97%B6%E6%A0%87%E7%82%B9%E7%94%A8%E6%B3%95.html)
- [fex-team/styleguide markdown.md](https://github.com/fex-team/styleguide/blob/master/markdown.md)
- [chen3feng/cn-doc-style-guide](https://github.com/chen3feng/cn-doc-style-guide)
- [TapTap TDS 中文文案风格指南](https://blog.taptap.dev/pages/chinese-copywriting-guide)
- [huacnlee/autocorrect](https://github.com/huacnlee/autocorrect) / [.autocorrectrc.default](https://github.com/huacnlee/autocorrect/blob/main/autocorrect/.autocorrectrc.default)
- [zhlint-project/zhlint README.zh-CN.md](https://github.com/zhlint-project/zhlint/blob/main/README.zh-CN.md)
- [vinta/pangu.js](https://github.com/vinta/pangu.js)
- [darkyzhou/textlint-rule-preset-zh-technical-writing](https://github.com/darkyzhou/textlint-rule-preset-zh-technical-writing)
- [textlint Collection of textlint rule · Chinese](https://github.com/textlint/textlint/wiki/Collection-of-textlint-rule)
- 本仓 `spec/rules.md`、`docs/adr/0002-rule-flags-and-rust.md`
