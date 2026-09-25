# AutoCorrect 与 zhlint 源码级工程调研

> 来源：Grok research agent 调研，2026-08-31；distill 决策另行进 spec / ADR，本文只记录证据。规则名单、默认开关、配置文件发现等 README / 配置层信息已见 `docs/research/zh-typography-guidelines-survey.md` §4，本文不重复。

调研日期：2026-08-31。全程只读，不改两个第三方仓，也不改本仓 `spec/`、`src/`、`tests/`。对照对象是本仓规则模型 (`spec/rules.md`、`spec/README.md`)、Python 参考实现 (`src/lo_md_lint/zh_format.py`) 与 Rust 终态方向 (ADR-0002)。

**观点与事实分开**：带「建议」「宜」「不要」的句子是调研结论；源码行为和本仓规范原文是事实。

第三方仓与本文引用的 commit (与 brief 一致，均为该仓当时 tip)：

| 仓 | 本地路径 | commit | 日期 (作者日期) |
| --- | --- | --- | --- |
| [huacnlee/autocorrect](https://github.com/huacnlee/autocorrect) | `~/3p/huacnlee/autocorrect` | `e1a75da` | 2026-06-23 |
| [zhlint-project/zhlint](https://github.com/zhlint-project/zhlint) | `~/3p/zhlint-project/zhlint` | `c8678fe` | 2025-07-12 |

下文路径均相对各自仓根。AutoCorrect 核心位于 `autocorrect/src/`，zhlint 核心位于 `src/`。

---

## 1. AutoCorrect

### 1.1 Markdown 怎么切出可见文本

**事实。** 它不用 CommonMark parser，而是 pest PEG 文法加一次树遍历。Markdown 入口：

```rust
#[derive(GrammarParser, Parser)]
#[grammar = "../grammar/markdown.pest"]
struct MarkdownParser;
```

(`autocorrect/src/code/markdown.rs:8`)

`GrammarParser` 过程宏为各语言生成 `format_*` / `lint_*`：pest 从 `Rule::item` 解析全文，再交给共用的 `format_pairs` (`autocorrect-derive/src/lib.rs:10`)。pest 调用深度上限为 `100_000_000` (`autocorrect/src/code/code.rs:24`)，用于防止复杂文法卡死。

文法顶层 (`autocorrect/grammar/markdown.pest:2`)：

```
item = _{ SOI ~ line* ~ EOI }
```

解析成功后得到一棵有序嵌套的 pair 树：`block` 与其 `text` / `inline` 等子 pair 的 span 会重叠，`format_pair` 通过 `into_inner()` 递归 (`code.rs:56`)。解析失败时，`format_pairs` 走 `Err` 分支，`FormatResult::error` 将输出还原为原文 (`autocorrect/src/code/code.rs:34`，`autocorrect/src/result/mod.rs:134`)。

`format_pair` 按规则名分流 (`autocorrect/src/code/code.rs:48`)：

| pest 规则名 | 行为 |
| --- | --- |
| `string` / `link_string` / `mark_string` / `text` / `inner_text` / `comment` | 交给 `format_or_lint`，作为可见文本运行规则 |
| `codeblock` | 提取语言和正文，**按语言递归** `format_for` / `lint_for` |
| 其它有子节点 | 继续向下遍历；无子节点则 `ignore`，拷贝原文 |

具体跳过 / 处理方式：

- **围栏代码块。** 文法只识别三重反引号，不识别 `~~~` (`markdown.pest:53`，仓内文法没有 `~~~` 字面量)：

  ```
  PUSH("```") ~ codeblock_lang ~ codeblock_code ~ POP
  ```

  围栏内容并非「整块豁免」：带语言标记时，会按该语言再运行一次规则 (Rust 字符串、JSON 的 key 等会被改，见 `markdown.rs` 测试期望)。语言无法识别时，`format_for` 落到默认分支，原样返回 (`autocorrect/src/code/mod.rs:179`)。四空格缩进代码走 `indent_code`，会拼进 `codeblock`，但提取不到 `codeblock_lang` / `codeblock_code`，实测为「不处理」。开关是配置 `context.codeblock`，默认 `1` (开)；关闭后，`format_or_lint_for_inline_scripts` 直接 return (`code.rs:183`)。
- **行内代码。** `code = ${ PUSH(open_code) ~ inner_code ~ close_code }`，定界符只识别一个 `` ` `` (`markdown.pest:113`)，**不是** CommonMark 的等长反引号串。`inner_code` 是原子规则，内容原样拷贝。反引号被切成独立 pair，周围的 `text` pair 看不到它们，因此规则 `space-backticks` 在 Markdown 中基本打不中 (配置默认仍开启；纯文本路径可以打中，见 `format.rs` 的 `行内\`code\`代码` 用例)。
- **链接 / 图片。** `link_string` 按可见文本处理 (alt / 锚文字会改)；`href = @{ paren }` 整段为原子规则，URL 不改 (`markdown.pest:102`)。wikilink `[[...]]` 没有子节点，整段 `ignore`。
- **HTML。** `html` 拆成起止标签 (原子，不改) 与 `inner_text` (会改)。属性值不在 `inner_text` 中，因此 `title="HTML标签里面都不处理"` 保持原样，标签体内文本会改 (`markdown.rs` 测试)。HTML 注释走 `comment`，**注释正文也会被规则改写**；注释同时还是 toggle 的载体 (见 §1.3)。
- **front matter。** `front_matter` 匹配 `---` 包裹的块 (`markdown.pest:86`)。普通 `meta_pair` 的值是 `string`，会被改 (`title: 示例标题Title` → `title: 示例标题 Title`)；`tags:` 走 `meta_tags`，逗号分隔的 CJK 标签会刻意不插空格。

**写回。** format 模式没有「按偏移打补丁」。父 pair 不自行输出；只有走到 `format_or_lint`，或走到无子节点并 `ignore` 的节点，才会调用 `FormatResult::push`，按实际遍历顺序拼接 `new` (`code.rs:48`，`result/mod.rs:125`)。这是实现事实，**不是** pair 互不重叠的保证。解析失败则整份返回原文，见上。lint 模式用 pest 的 `pair.line_col()` 计算行 / 列，多行 pair 再按 `\n` 增加 `sub_line` (`code.rs:104`)。`format_or_lint` 会把一段可见文本按空格或换行切开，分别运行规则，再 `join("\n")` (`code.rs:150`)，因此**单段内部的换行数不变**；规则函数本身只做正则替换，不插入 `\n`。仓内**没有**「输出行数 == 输入行数」的断言，这是拼接带来的副作用，不是契约。

含 CJK 的 `block` 会临时执行 `toggle_merge_for_codeblock`，关闭其中的 `halfwidth-punctuation` (`code.rs:64`，`result/mod.rs:65`)；只有英文段落才会把全角标点收成半角。

### 1.2 规则怎么组织

**事实。** 规则单元是 `Rule { name, format_fn }`，其中 `format_fn: fn(&str) -> Cow<str>` (`autocorrect/src/rule/rule.rs:6`)。规则通过两张硬编码表和 `lazy_static` 注册，没有插件式 registry (`rule/mod.rs:19`)：

1. `RULES` (先运行)：`space-word`、`space-punctuation`、`space-bracket`、`space-dash`、`space-backticks`、`space-dollar`、`fullwidth`
2. `AFTER_RULES` (后运行)：`halfwidth-word`、`halfwidth-punctuation`、`no-space-fullwidth`、`no-space-fullwidth-quote`、`spellcheck`

空格类规则几乎都是 `Strategery`：两套字符类正则，分别 `Add` 或 `Remove` 空格，可选 `with_reverse` 后再反向运行 (`rule/strategery.rs:9`)。`fullwidth` / `halfwidth-*` / `spellcheck` 各自有替换函数。

配置开关是 `HashMap<String, SeverityMode>`，0 = off、1 = error、2 = warning (`config/severity.rs:4`)。`Rule::severity()` 读取当前全局 `Config`；名称不在 map 中时视为 Off (`rule/rule.rs:63`)。**没有**本仓这种「默认集 ∪ enable − disable」的求值方式，而是每条规则自行查表。

同一条 `format_fn` 同时服务 check 和 fix，区别在 `Rule::format` 与 `Rule::lint` (`rule/rule.rs:35`)：

- `format` (fix)：只运行 `SeverityMode::Error`。Warning 规则在 **fix 时完全跳过**。
- `lint` (check)：Off 跳过，Error / Warning 都运行；首次改写时将 `RuleResult.severity` 写为对应级别。

因此 Warning 的含义是「只报不修」。测试配置把 `spellcheck` 设为 `2`，所以 `format_after_rules(..., lint=false)` 不改 `ios`，只有 `lint=true` 才会改成 `iOS` (`rule/mod.rs:241`，`autocorrect/tests/.autocorrectrc.test:3`)。

另有 `textRules`：原文 `contains` 某字符串时，会将整段结果覆盖为 Off (还原) 或 Warning (`rule/mod.rs:152`)。这是子串级的严重级别覆盖，不是规则 id。

**顺序与幂等。** 顺序就是上述两张表的字面顺序，只是实现细节，**没有**写成对外契约。冲突由后段收尾：`space-word` 可能在全角标点旁留下空格，再由 `no-space-fullwidth` 删除。运行两遍时，**没有** fixpoint 循环，也没有「再次 format 不变」的测试。Markdown 大黄金样例 `tests/fixtures/markdown.raw.md` 只断言一次 `format_markdown` 的输出；对期望文本再运行 `lint_for` 并要求 `lines` 为空 (`markdown.rs:333`)，这说明「修好的文本 check 干净」，不是「fix 再 fix 不变」。

`format_or_lint_with_disable_rules` 会先按空白切段；不含 CJK 的段落跳过 `RULES`，最后整段再运行 `AFTER_RULES` (`rule/mod.rs:86`)。URL / 路径启发式 (`PATH_RE`、`PATH_HASH_RE`) 会让整段 `format_part` 直接 return，避免改动地址。

### 1.3 disable / ignore 怎么实现

**事实。行内指令**由 pest 文法扫描注释 (`config/toggle.pest`)：

```
enable  = ${ "autocorrect" ~ (":" ~ " "* | "-") ~ ("enable" | "true")  ~ pair* }
disable = ${ "autocorrect" ~ (":" ~ " "* | "-") ~ ("disable" | "false") ~ pair* }
```

可带规则名列表 (`autocorrect-disable space-word,fullwidth`)。`format_or_lint` 遇到 `comment` / `COMMENT` 时，先通过 `toggle::parse` 写入 `Results` 上的 `Toggle` 状态机 (`code.rs:93`)。作用域是**文档序上的后续 pair**，直到下一条 enable / disable；不是「只作用于本行」，也不是源码区间标记。

`Toggle::Disable(空表)` 表示全部关闭，`is_enabled()` 为 false，后续整段跳过；`Disable(非空)` 时 `is_enabled()` 仍为 true，只会把这些名称放入 `disable_rules` HashMap 过滤 (`config/toggle.rs:50`，`code.rs:97`)。同向的 `merge` 会合并规则名；Enable 与 Disable 相遇时，后者覆盖 (`toggle.rs:78`)。

**忽略文件。** `autocorrect/src/ignorer.rs` 使用 Rust `ignore` crate (与 ripgrep 相同) 的 `GitignoreBuilder`，会在工作目录同时 `add` `.autocorrectignore` 与 `.gitignore`，并通过 `matched_path_or_any_parents` 匹配。CLI 还有第二层：`ignore::WalkBuilder` 开启 `git_ignore(true).parents(true)` (`autocorrect-cli/src/lib.rs:147`)，再对每个条目调用 `ignorer.is_ignored` (`:166`)。绝对路径会先 `strip_prefix(cwd)` 再匹配 (`:88`)。**显式传入的文件同样受约束** —— walker 的根就是参数中的路径，命中 ignore 后会 `continue`。stdin 模式不走 walker / Ignorer。LSP / Node / Python / Ruby / Java / Wasm 都封装了同一份 `Ignorer`，但**没有**跨 SDK 共用的 ignore 测试 fixture，各自只有一层冒烟测试。

### 1.4 测试怎么组织

**事实。**

- **核心：** 各模块使用 `#[cfg(test)]`，大量采用 `HashMap<&str, &str>` 的输入 / 期望对 (`format.rs` 的 `assert_cases`)。Markdown 另有一份很长的 inline `indoc` 对，加上 `tests/fixtures/markdown.raw.md` / `markdown.fixed.md`。其它语言使用 `tests/fixtures/<lang>.raw.*` + `.fixed.*` + `.expect.json` (lint JSON)。
- **跨语言 SDK：** 不共享这套 fixture。`autocorrect-node/__test__/index.spec.mjs`、`autocorrect-py/test_autocorrect_py.py`、`autocorrect-rb/test/` 各写几条 `format("Hello你好.")` / `Ignorer` 冒烟测试，FFI 都接到同一份 Rust 核心。`Makefile` 的 `test:node` / `test:python` / `test:ruby` / `test:java` 分别运行各自一层。
- **property / 幂等：** 仓内搜不到 `proptest` / `quickcheck` /「跑两遍」断言。`cargo criterion` 只用于 bench。

lint 报告粒度是**整行** `old` / `new` (`LineResult`)，不带规则 id；一行上多个规则的改写会揉进同一条 diff。

---

## 2. zhlint

### 2.1 Markdown 怎么切出可见文本

**事实。** 设计文档将流程写成四步 (`docs/design.md:3`)：parse → apply rules → join → report。Markdown 不是由规则自行切分，而是先由 **hyper parser** 将源码收成带槽位的块。

默认 hyper parser 链 (`src/options.ts:31`)：`ignore` → `hexo` → `vuepress` → `markdown`。

**Markdown** (`src/hypers/md.ts:234`) 使用 `unified().use(remark-parse).use(remark-gfm).use(remark-frontmatter)`，得到真正的 mdast，不是用正则模拟 CommonMark。随后：

- 只将 `paragraph` / `heading` / `table-cell` 收成 block (`md.ts:34`)。围栏代码、HTML 块、thematic break、yaml front matter **不会成为 block**。
- yaml 节点直接 `return` (`md.ts:93`)；front matter 整块留在后续 non-block 的缝隙中，**值也不改**。
- 块内 inline：`emphasis` / `strong` / `delete` / `link` / `linkReference` 记为 `HYPER` 标记 (左右定界符，中间继续解析)；`inlineCode` / `break` / `image` / `imageReference` / `footnoteDefinition` / `html` 记为 `RAW` (`md.ts:57`)，整段 (包括图片 alt) 不再进入字符解析。
- 链接的 `startValue` 是 `[`，`endValue` 是 `](url)` (`md.ts:179`)；锚文字属于可见文本，destination 位于 HYPER 右标记中，规则无法改到。

Hexo `{% ... %}...{% endx %}` 与 VuePress `:::` 容器会用正则在 `modifiedValue` 中替换为**等长** `@` 占位，确保偏移仍与原文对应 (`src/hypers/hexo.ts:10`，`vuepress.ts:19`)。markdown parser 运行在 `modifiedValue` 上，但提取 block 文本时使用原始 `data.value` (`md.ts:253`)。

每个 block 再交给自研字符 parser (`src/parser/parse.ts:78`)：逐字分类为西文 / CJK / 标点 / 括号 / 引号组；空格会写入相邻 token 的 `spaceAfter` / `innerSpaceBefore`，不再单独占一种 token (`docs/design.md:57`)。hyperMarks 会在对应 index 插入 RAW / HYPER，RAW 一次覆盖 `[startIndex, endIndex)`。

**写回。** `join` 读取各 token 的 `modified*`，拼回块字符串 (`src/join.ts:104`)；`replaceBlocks` 按 block 的 `start` / `end` (unist `offset`，半开区间) 将改过的块嵌回原文，并原样拷贝缝隙中的 non-block (`src/replace-block.ts:30`)。位置是**源码偏移**，report 再将 offset 换算成行 / 列 (`src/report.ts:37`)。行数是否不变，取决于规则会不会改 `spaceAfter` 中的 `\n`：`case-linebreak` 会强制还原带换行的 `spaceAfter` (`src/rules/case-linebreak.ts:19`)，这是保行的关键，但**没有**「输入输出行数相等」测试。`trimSpace` 理论上可删去块首尾空格；对应单测在「latest remark parser」下被 `test.skip` (`test/rules.test.ts:20`)。

remark 同时识别 CommonMark / GFM 的 \`\`\` 与 `~~~`。围栏代码不是 paragraph，落在 block 缝隙中，整块不改 —— 与 AutoCorrect「按语言再 format 一次」相反。

### 2.2 规则怎么组织

**事实。** 没有规则 id 表。规则单元是 `Handler = (token, index, group) => void` (`src/parser/travel.ts:3`)。`generateHandlers(options)` 返回按固定顺序排列的闭包数组 (`src/rules/index.ts:24`)：

1. `space-trim`
2. `punctuation-width` / `punctuation-unification`
3. `case-abbrs` (将缩写中的点改写**还原**)
4. 空格：`space-hyper-mark`、`space-code`、`space-letter`、`space-punctuation`、`space-quotation`、`space-bracket`
5. `case-linebreak` (还原换行)
6. `case-zh-units` / `case-html-entity` (还原)
7. `case-pure-western` (整段西文时清掉 validations、还原 modified)

`travel` 会先序遍历 group 中每个 token，再递归子 group (`travel.ts:9`)。每条规则自行读取 `Options` 中对应键：`true` 表示执行，`false` 通常表示相反策略 (例如不要空格)，`undefined` 表示**既不报也不改** (`docs/design.md:192`，`src/rules/space-letter.ts:8`)。这不是 severity，而是「是否改动」的三态。

**顺序与幂等。** 顺序属于实现约定；设计文档使用 L/P/Qo/Qi/Bo/Bi/D 的组合表对齐各空格规则 (`docs/design.md:262`)，并注明「This part might vary frequently」。特殊情况采用「先改再还原」：`space-letter` 会为 `2011年` 插入空格，`case-zh-units` 若原文两侧本来没有空格，就会清掉 `modifiedSpaceAfter` 并执行 `removeValidationOnTarget` (`src/rules/case-zh-units.ts:42`)。原文已有空格时不会还原，所以 `2020 年1月1日…` 会保留「2020 后面的一个空格」(`test/example-units-fixed.md:25`)。没有第二遍 pass，也没有幂等测试；`test/examples.test.ts` 中 Vue 文档用例的断言是 `result === input` (已合规文章保持不变)，不是 `run(run(x)) === run(x)`。

`run()` 总会同时计算 `result` 字符串和 `validations`。CLI 不带 `--fix` 时只打印报告、不写盘 (`bin/index.js:125`)；规则层没有「只报不修」形态。没有 error / warning 分级，validation 只有 `name` / `message` / `index` / `length` / `target` (`src/report.ts:63`)。

### 2.3 disable / ignore 怎么实现

**事实。三种完全不同的机制。**

1. **整文件 disable。** `lint` 开头用 `/<!--\s*zhlint\s*disabled\s*-->/g` 搜索原文，命中后返回 `origin === result`、`validations: []`、`disabled: true` (`src/run.ts:53`)。**不是区间**；注释出现在文件任意位置都会跳过整份文件 (`test/example-disabled.md`)。
2. **按片段 ignore。** HTML 注释 `<!-- zhlint ignore: prefix-,start,end,-suffix -->` (`src/hypers/ignore.ts:23`)，语法仿 Scroll To Text Fragment (`src/ignore.ts:3`)。hyper parser 将解析结果放进 `ignoredByRules`；每块通过 `findIgnoredMarks` 在块字符串中找到所有出现位置，得到 `[start, end)` (`src/ignore.ts:21`)。`join` 时，若 token 与 mark 重叠，就将对应 `modified*` 还原为原文；validation 放入 `ignoredRuleErrors`，不会对外报告 (`src/join.ts:14`)。`.zhlintcaseignore` 与 `Config.caseIgnores` 使用同一套 `parseIngoredCase` (`src/rc/index.ts:145`，`src/options.ts:160`)。**不能按规则名关闭**，只能圈定一段文字。
3. **忽略文件。** `.zhlintignore` 按行读入 `fileIgnores` (`src/rc/index.ts:119`)。CLI 使用 npm `ignore` (gitignore 语法) 的 `gitignore().add(config.fileIgnores).createFilter()`，再过滤 `glob.sync` 得到并转为相对路径的文件 (`bin/index.js:81`)。`createFilter()` 返回 true 表示**保留** (未被 ignore)。**显式传入的文件同样会被过滤**。不会自动读取 `.gitignore` (与 AutoCorrect 不同)。只发现 `--dir` (默认 cwd) 下的那一份，不向上查找。

### 2.4 测试怎么组织

**事实。** 使用 Vitest (`package.json` 的 `test:vitest`)。

- `test/rules.test.ts`：单规则 `getOutput` / `lint`，输入输出写在测试中，warning 断言 `index` + `target` + `message`。
- `test/examples.test.ts`：整文件黄金对 (`example-units.md` / `example-units-fixed.md` 等)。
- `test/md.test.ts`：断言 hyper parser 提取的 mark (链接 `[` / `](xxx)`、行内代码 RAW 等)。
- `test/lint.test.ts`：`ignoredCases` API。
- hexo / vuepress / report 各有单独文件。

只有单语言实现，没有 SDK 矩阵，也没有 property 测试。消息字符串集中在 `src/rules/messages.ts` (中文)。

---

## 3. 对照 (事实)

| 问题 | AutoCorrect (`e1a75da`) | zhlint (`c8678fe`) | 本仓现状 |
| --- | --- | --- | --- |
| MD 切分 | pest PEG，不是 CommonMark | remark-parse + GFM + frontmatter (mdast) | 正则：围栏行、CommonMark 等长反引号、最简 `](` / 裸 URL (`zh_format.py` 的 `_is_fence` / `_code_spans` / `URL_DESTINATION`) |
| 代码块 | \`\`\` 按语言**递归 format**；无 `~~~` | 不成 block，缝里原样，\`\`\` / `~~~` 都跳 | 围栏行与内容整行豁免，\`\`\` 与 `~~~` 都认 |
| 行内代码 | 单 \` 定界，内容不改；`space-backticks` 在 MD 里几乎无效 | RAW token，`spaceOutsideCode` 管外侧 | 等长反引号串，内部豁免，zh-typography-7 管外侧 |
| 链接 | 锚文字改、href 原子 | HYPER：`[` + 锚文字 + `](url)`，destination 不改 | destination 与裸 URL 全局豁免 |
| 图片 | alt 当 link_string 会改 | 整段 RAW，alt 也不改 | 未单独建模；alt 落在行文里会走规则 |
| HTML | 标签不改、inner_text 改；注释当正文改 | 块级 HTML 在缝里；inline html 为 RAW | 无 HTML 豁免 |
| front matter | `---` 块内普通值会改，`tags:` 特例 | yaml 节点整块跳过 | 规范未提，按行文处理 |
| 写回 | pair 拼接；parse 失败回原文 | 按 offset 嵌块 (`replaceBlocks`) | 逐行 `split("\n")` / `join`，**不增删行**是规范 (`spec/rules.md`「处理单位」) |
| 位置 | pest `line_col`；lint 报整行 old/new | 源码 offset → 行 / 列；按 token 部件 | 行号 + 规则 id，无列号 |
| 规则单元 | 具名 `fn(&str) -> Cow<str>`，两张静态表 | `Handler` 闭包 + `Options` 三态布尔 | 稳定 id zh-typography-1 到 zh-typography-11，规范以 `spec/rules.md` 为准 |
| 顺序 | `RULES` 然后 `AFTER_RULES`，实现细节 | `generateHandlers` 数组，先改再还原特例 | 「修复顺序」是契约，各实现必须一致 |
| 幂等 | 单遍，无 fixpoint，无测试 | 单遍 + 还原，无 `run(run(x))` 测试 | `--fix` 循环到不动点；runner 断言 `fix(fixed) == fixed` |
| 只报不修 | Warning：lint 跑、format 不跑 | 无；`undefined` = 不报不改。CLI 不写盘只是不保存 | check 与 fix 同一启用集；tracker 有 non-fixable / warning backlog |
| 行内 disable | 注释状态机，可带规则名，后续 pair 生效 | `zhlint disabled` 整文件；`zhlint ignore:` 按片段，不按规则名 | 无 (tracker 要做) |
| 忽略文件 | `.autocorrectignore` + `.gitignore`，`ignore` crate；显式文件也过滤 | `.zhlintignore`，npm `ignore`；不读 `.gitignore`；显式文件也过滤 | 无 |
| 黄金样例 | 模块内 map + `tests/fixtures/*.{raw,fixed,expect}` | `test/example-*.md` 对 + 单测内字符串 | `spec/fixtures/<case>.{in,fixed,findings}`，可选 `.conf` |
| 跨语言共享测试 | 不共享 fixture，SDK 各自冒烟 | 无 SDK | 语言无关黄金集，实现只写薄 runner (`spec/README.md`) |

---

## 4. 对本仓 Rust 终态的 distill 判断

以下是观点。只讨论机制和理由，不涉及 crate 布局。本仓已经确定的模型 (逐行、不增删行、check 看原文、fix 到不动点、规则 id 稳定、启用集 = (默认集 ∪ enable) − disable) 是约束，不建议用任何一侧的默认行为推翻。

### 4.1 值得抄

1. **「抽出可见文本 → 改 → 按原偏移嵌回」** (zhlint 的 block + `replaceBlocks`)，不要采用 AutoCorrect 那种「遍历 pair 树、依次 `push` 终端节点」。理由：本仓契约要求语法结构不动、行数不变；嵌回只替换 mdast 认定的 paragraph / heading / table-cell，围栏、列表标记、yaml 会自然留在缝隙中。AutoCorrect 的拼接只是成功 parse 后的树遍历副作用，parse 失败则整份回原文；它的 Markdown 文法也不是 CommonMark (无 `~~~`、单反引号、注释当正文、代码块递归 format)。Rust 若更换 parser，应对齐 zhlint 的写回策略，而不是 pest 的文法。
2. **Parse 失败则整份原文返回** (AutoCorrect `FormatResult::error`)。理由：排版 linter 改坏源码的代价高于漏报；本仓现有正则路径很少会「解析失败」，但一旦换成真正的 parser，这条就是必要的安全网。
3. **行内 disable 使用文档序状态机，且可带规则 id** (AutoCorrect `Toggle`)。理由：tracker 所需的逃生口是「单点误报不必动配置」。zhlint 的整文件 `disabled` 太粗；它的片段 ignore 圈定的是文字而不是规则，无法关闭「这一段只跳过 zh-typography-4」。状态机比「只作用于注释所在行」更能覆盖围栏前的一段，也比源码区间标记少掉配对闭合。建议注释语法与本仓规则 id (`zh-typography-4`) 对齐，不要引入 `space-word` 这类外部分名。
4. **忽略文件使用 gitignore 语法，显式传入的路径也要过滤** (两侧 CLI 都如此)。理由：与「我写在命令行上就一定要检」相反，两侧都选择让 ignore 生效，避免 CI 脚本 `git ls-files '*.md'` 再把生成物送回来。Rust 侧直接使用 `ignore` crate 即可 (AutoCorrect 已验证)。是否像 AutoCorrect 一样**默认叠加 `.gitignore`**，宜单独拍板：叠加可省一份名单，但会让「源码在 gitignore 中、仍想 lint」的生成文档消失；zhlint 只读自己的 ignore 文件，更可预测。无论选哪种，显式路径与忽略文件的关系都必须写入规范；两侧源码都没有「强制检」开关。
5. **用等长占位保住偏移** (zhlint 对 Hexo / VuePress 使用 `@`.repeat(length))。理由：本仓若始终只处理 CommonMark，目前不必做；一旦要豁免某种「看似正文、实际是指令」的结构，先占位再 parse 比把方言写进文法更省事。
6. **lint 报告带列号 / 偏移** (AutoCorrect `c`，zhlint `index`)。理由：当前规范只要求行号 + 规则 id，黄金集也按此比较。Rust 终态若要对接 LSP (ADR-0002 / tracker)，内部保留列或 byte offset 成本很低，对外 fixture 不必改。不要像 AutoCorrect 那样把同一行上的多规则揉成整行 `old`/`new` —— 本仓 `.findings` 已经允许「同一行同一规则可出现多次」。
7. **特例使用「先改再还原」只适合选项很多、规则共用同一 token 流的场景** (zhlint 的 `skipZhUnits` / `skipAbbrs` / linebreak)。本仓已经将 `skip_zh_units` 写入 zh-typography-5 的判定，将缩写表写入 zh-typography-1，**不必再抄还原 pass**。值得借鉴的是这一点：空格规则和「不要改换行」冲突时，zhlint 通过强制还原换行保住行数 (`case-linebreak`)；本仓根本不在 token 的 `spaceAfter` 中修改 `\n`，没有这个问题，但 Rust 若改用 token 流，就要一并处理。

### 4.2 不要抄

1. **不要抄 AutoCorrect 的 Markdown pest 文法。** 它不是 CommonMark：不识别 `~~~`、行内代码不是等长反引号串、会 format HTML 注释、会递归 format 围栏内容、会改 front matter 的普通值、会改图片 alt。这些都直接违反本仓「全局豁免」第 1–3 款 (围栏整块、行内代码内部、destination / URL)。`context.codeblock` 默认开启，与本仓默认相反。既有调研 §4.1 第 6 条「按文件类型 AST 只扫字符串和注释」对代码语言成立，对 Markdown **不成立**，源码级应以本文为准。
2. **不要抄 `space-backticks` 这种「规则假定反引号仍在同一段文本中」的实现。** Markdown 一旦切成 token，规则就不会命中；配置默认开启会让人误以为它在工作。本仓 zh-typography-7 明确按 span 判断定界串外侧，这个模型应保留。
3. **不要抄 zhlint 那份巨大的 boolean `Options`。** 既有调研已说明它不符合「规则 id 稳定不复用」。源码进一步确认：`undefined` / `true` / `false` 三态、先改再还原、消息字符串与 handler 一一对应，迁移成本高，也没有本仓需要的「关闭 zh-typography-3、其余保持」这类启用集运算。
4. **不要把 AutoCorrect 的 Warning = 只报不修当作默认严重级别模型。** 机制本身 (lint 运行、fix 跳过) 很清晰，但本仓当前契约是启用集内 check 与 fix 行为一致。tracker 的 non-fixable 若要落地，宜做成规则属性 (ruff 的 fixable)，不要做成配置中的 0/1/2 并与规则表缠在一起。也不要抄 `textRules` 的 `contains` 覆盖 —— 子串命中就整段还原，调试时无法看出是哪条规则让步，且与本仓「未知 id 必须报错」的配置理念相反。
5. **不要抄「单遍 fix、靠作者保证幂等」。** 本仓已经因为「zh-typography-2 换出的半角括号会改变链接 destination 豁免范围」而将不动点写入规范 (`spec/rules.md`「处理单位」；`zh_format.py` `fix_text` 的 `while True`)。AutoCorrect / zhlint 都没有这条，也没有测试。Rust 实现必须继续针对 `spec/fixtures` 运行三条断言，不能降格为它们的单遍模型。
6. **不要把 SDK 冒烟测试当黄金集。** AutoCorrect 的多语言绑定各自只写 `Hello你好.`，真正的 Markdown 行为只在 Rust 测试中。本仓 ADR-0001 的 `spec/fixtures/` 才是应借鉴的共享方式；Rust 终态若增加绑定，绑定测试保持冒烟即可，不要再复制一份 fixture。
7. **不要将 zhlint 的整文件 `disabled` 作为主逃生口，也不要用 case ignore 替代 disable 规则。** 片段 ignore 适合「这一串 `( , )` 是 API 签名」这类原文本就如此的内容；它会按文本搜索所有出现位置，容易误伤。本仓若实现，应作为 disable 注释的补充，而不是主机制。
8. **不要抄「英文段落把全角标点收成半角」** (AutoCorrect `halfwidth-punctuation` + CJK block 临时关闭)。本仓 zh-typography-1 是 CJK 旁半角 → 全角，反向转换不在默认集；家规中的括号方向已与 zhlint 默认一致 (既有调研 §2.4)，不要为了对齐 AutoCorrect 再增加一条未进入 spec 的反向规则。

### 4.3 对既有调研的校准 (源码改了 README 印象的地方)

既有调研 §4 在配置层是对的。源码要求改口的只有这些：

- AutoCorrect 在 Markdown 中**会改代码块** (默认)，不是「只扫可见文本」。
- AutoCorrect `space-backticks` 默认开启，但在 Markdown 路径中几乎不生效。
- zhlint `skipZhUnits` 不是「判定时跳过」，而是「插完再还原，且只还原原文没有空格的边界」。本仓 zh-typography-5 的 `skip_zh_units` 是判定豁免，语义更清晰，应保持。
- 两侧 CLI 都会对**显式文件**应用 ignore，README 不一定写清。编写本仓规范时要明确写死。

**未核实：** AutoCorrect `WalkBuilder` 在「路径既是 walker 根、同时又被 gitignore」时，是根节点仍会发出、再被 `Ignorer` 丢掉，还是 walker 根本不会发出 —— 源码中两层过滤都存在，本文只按「最终不处理」描述，未逐步运行 CLI 确认哪一层先起作用。zhlint `replaceBlocks` 的 heading 节点 offset 是否包含 `#` 标记：mdast 通常包含，字符 parser 会看到 `#`；`example-units.md` 几乎没有 ATX 标题，heading 标记会不会被空格规则改写，本文未用 vitest 实测。
