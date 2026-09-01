# AutoCorrect 与 zhlint 源码级工程调研

> 来源：Grok research agent 调研，2026-08-31；distill 决策另行进 spec / ADR，本文只是证据。规则名单、默认开关、配置文件发现等 README / 配置层已见 `docs/research/zh-typography-guidelines-survey.md` §4，本文不重复。

调研日期：2026-08-31。只读，不改两个第三方仓，也不改本仓 `spec/`、`src/`、`tests/`。对照对象是本仓规则模型 (`spec/rules.md`、`spec/README.md`)、Python 参考实现 (`src/lo_md_lint/zh_format.py`) 与 Rust 终态方向 (ADR-0002)。

**观点与事实分开**：带「建议」「宜」「不要」的句子是调研结论；源码行为、本仓规范原文是事实。

第三方仓与本文引用的 commit (与 brief 一致，均为该仓当时 tip)：

| 仓 | 本地路径 | commit | 日期 (作者日期) |
| --- | --- | --- | --- |
| [huacnlee/autocorrect](https://github.com/huacnlee/autocorrect) | `~/3p/huacnlee/autocorrect` | `e1a75da` | 2026-06-23 |
| [zhlint-project/zhlint](https://github.com/zhlint-project/zhlint) | `~/3p/zhlint-project/zhlint` | `c8678fe` | 2025-07-12 |

下文路径相对各自仓根。AutoCorrect 核心在 `autocorrect/src/`，zhlint 核心在 `src/`。

---

## 1. AutoCorrect

### 1.1 Markdown 怎么切出可见文本

**事实。** 不是 CommonMark parser，是一份 pest PEG 文法加一次树遍历。Markdown 入口：

```rust
#[derive(GrammarParser, Parser)]
#[grammar = "../grammar/markdown.pest"]
struct MarkdownParser;
```

(`autocorrect/src/code/markdown.rs:8`)

`GrammarParser` 过程宏给每个语言生成 `format_*` / `lint_*`：pest 从 `Rule::item` 解析全文，再交给共用的 `format_pairs` (`autocorrect-derive/src/lib.rs:10`)。pest 调用深度上限 `100_000_000` (`autocorrect/src/code/code.rs:24`)，用来防复杂文法卡死。

文法顶层 (`autocorrect/grammar/markdown.pest:2`)：

```
item = _{ SOI ~ line* ~ EOI }
```

成功解析时整份输入被 PEG 切成互不重叠的 pair；失败则 `format_pairs` 走 `Err` 分支，`FormatResult::error` 把输出打回原文 (`autocorrect/src/code/code.rs:34`，`autocorrect/src/result/mod.rs:134`)。

`format_pair` 按规则名分流 (`autocorrect/src/code/code.rs:48`)：

| pest 规则名 | 行为 |
| --- | --- |
| `string` / `link_string` / `mark_string` / `text` / `inner_text` / `comment` | 交给 `format_or_lint`，当可见文本跑规则 |
| `codeblock` | 抽出语言与正文，**按语言递归** `format_for` / `lint_for` |
| 其它有子节点 | 继续往下走；无子节点则 `ignore`，原文拷贝 |

具体跳过 / 处理：

- **围栏代码块。** 文法只推三重反引号，不认 `~~~` (`markdown.pest:53`，仓内文法无 `~~~` 字面量)：

  ```
  PUSH("```") ~ codeblock_lang ~ codeblock_code ~ POP
  ```

  围栏里不是「整块豁免」：有语言标记就按该语言再跑一遍 (Rust 字符串、JSON 的 key 等会被改，见 `markdown.rs` 测试期望)。语言无法识别时 `format_for` 落到默认分支，原样返回 (`autocorrect/src/code/mod.rs:179`)。四空格缩进代码走 `indent_code`，拼进 `codeblock` 但抽不出 `codeblock_lang` / `codeblock_code`，实测当「不处理」。开关是配置 `context.codeblock`，默认 `1` (开)；关掉则 `format_or_lint_for_inline_scripts` 直接 return (`code.rs:183`)。
- **行内代码。** `code = ${ PUSH(open_code) ~ inner_code ~ close_code }`，定界符只推一个 `` ` `` (`markdown.pest:113`)，**不是** CommonMark 的等长反引号串。`inner_code` 是原子规则，内容原样拷贝。因为反引号被切成独立 pair，周围的 `text` pair 看不到它们，规则 `space-backticks` 在 Markdown 里基本打不中 (配置默认却是开的；纯文本路径可以打中，见 `format.rs` 的 `行内\`code\`代码` 用例)。
- **链接 / 图片。** `link_string` 当可见文本处理 (alt / 锚文字会改)；`href = @{ paren }` 整段原子，URL 不改 (`markdown.pest:102`)。wikilink `[[...]]` 无子节点，整段 `ignore`。
- **HTML。** `html` 拆成起止标签 (原子，不改) 与 `inner_text` (改)。属性值不在 `inner_text` 里，所以 `title="HTML标签里面都不处理"` 保持原样，标签体内文本会改 (`markdown.rs` 测试)。HTML 注释走 `comment`，**注释正文也会被规则改写**，同时注释还是 toggle 的载体 (见 §1.3)。
- **front matter。** `front_matter` 吃 `---` 包裹的块 (`markdown.pest:86`)。普通 `meta_pair` 的值是 `string`，会被改 (`title: 示例标题Title` → `title: 示例标题 Title`)；`tags:` 走 `meta_tags`，逗号分隔的 CJK 标签刻意不插空格。

**写回。** format 模式没有「按偏移打补丁」：`FormatResult::push` 只是把每个 pair 的 `new` 按遍历顺序拼起来 (`result/mod.rs:125`)。lint 模式用 pest 的 `pair.line_col()` 作行 / 列，多行 pair 再按 `\n` 加 `sub_line` (`code.rs:104`)。`format_or_lint` 把一段可见文本按空格或换行切开分别跑规则，再 `join("\n")` (`code.rs:150`)，所以**单段内部的换行数不变**；规则函数本身是正则替换，不插 `\n`。仓内**没有**「输出行数 == 输入行数」的断言，这是拼接的副作用，不是契约。

含 CJK 的 `block` 会临时 `toggle_merge_for_codeblock`，并入关掉 `halfwidth-punctuation` (`code.rs:64`，`result/mod.rs:65`)，英文段落才把全角标点收成半角。

### 1.2 规则怎么组织

**事实。** 规则单元是 `Rule { name, format_fn }`，`format_fn: fn(&str) -> Cow<str>` (`autocorrect/src/rule/rule.rs:6`)。两张硬编码表，用 `lazy_static` 注册，没有插件式 registry (`rule/mod.rs:19`)：

1. `RULES` (先跑)：`space-word`、`space-punctuation`、`space-bracket`、`space-dash`、`space-backticks`、`space-dollar`、`fullwidth`
2. `AFTER_RULES` (后跑)：`halfwidth-word`、`halfwidth-punctuation`、`no-space-fullwidth`、`no-space-fullwidth-quote`、`spellcheck`

空格类规则几乎都是 `Strategery`：两套字符类正则，`Add` 或 `Remove` 空格，可选 `with_reverse` 再跑反方向 (`rule/strategery.rs:9`)。`fullwidth` / `halfwidth-*` / `spellcheck` 是各自的替换函数。

配置开关是 `HashMap<String, SeverityMode>`，0 = off、1 = error、2 = warning (`config/severity.rs:4`)。`Rule::severity()` 读当前全局 `Config`；名字不在 map 里当作 Off (`rule/rule.rs:63`)。**没有**本仓这种「默认集 ∪ enable − disable」的求值，而是每条规则自己查表。

同一条 `format_fn` 服务 check 与 fix，差别在 `Rule::format` vs `Rule::lint` (`rule/rule.rs:35`)：

- `format` (fix)：只跑 `SeverityMode::Error`。Warning 的规则 **fix 时完全跳过**。
- `lint` (check)：Off 跳过，Error / Warning 都跑；第一次改写时把 `RuleResult.severity` 写成对应级别。

所以 Warning 就是「只报不修」。测试配置把 `spellcheck` 设成 `2`，于是 `format_after_rules(..., lint=false)` 不改 `ios`，`lint=true` 才改成 `iOS` (`rule/mod.rs:241`，`autocorrect/tests/.autocorrectrc.test:3`)。

另有 `textRules`：原文 `contains` 某字符串就把整段结果覆盖成 Off (还原) 或 Warning (`rule/mod.rs:152`)。这是子串级的严重级别覆盖，不是规则 id。

**顺序与幂等。** 顺序就是上述两张表的字面顺序，实现细节，**没有**写成对外契约。冲突靠后段收尾：`space-word` 可能在全角标点旁留下空格，`no-space-fullwidth` 再删掉。跑两遍：**没有** fixpoint 循环，也没有「再 format 一次不变」的测试。Markdown 大黄金 `tests/fixtures/markdown.raw.md` 只断言一遍 `format_markdown` 的输出；对期望文本再 `lint_for` 要求 `lines` 为空 (`markdown.rs:333`)，这是「修好的文本 check 干净」，不是「fix 再 fix 不变」。

`format_or_lint_with_disable_rules` 先按空白切段，无 CJK 的段跳过 `RULES`，最后整段再跑 `AFTER_RULES` (`rule/mod.rs:86`)。URL / 路径启发式 (`PATH_RE`、`PATH_HASH_RE`) 让整段 `format_part` 直接 return，避免改地址。

### 1.3 disable / ignore 怎么实现

**事实。行内指令**是 pest 文法扫注释 (`config/toggle.pest`)：

```
enable  = ${ "autocorrect" ~ (":" ~ " "* | "-") ~ ("enable" | "true")  ~ pair* }
disable = ${ "autocorrect" ~ (":" ~ " "* | "-") ~ ("disable" | "false") ~ pair* }
```

可带规则名列表 (`autocorrect-disable space-word,fullwidth`)。`format_or_lint` 遇到 `comment` / `COMMENT` 先 `toggle::parse` 写进 `Results` 上的 `Toggle` 状态机 (`code.rs:93`)。作用域是**文档序的后续 pair**，直到下一条 enable / disable，不是「只作用于本行」，也不是源码区间标记。

`Toggle::Disable(空表)` 表示全关，`is_enabled()` 为 false，后面整段跳过；`Disable(非空)` 时 `is_enabled()` 仍为 true，只把那些名字送进 `disable_rules` HashMap 过滤 (`config/toggle.rs:50`，`code.rs:97`)。同向的 `merge` 会并入规则名；Enable 与 Disable 相遇则后者覆盖 (`toggle.rs:78`)。

**忽略文件。** `autocorrect/src/ignorer.rs` 用 Rust `ignore` crate (与 ripgrep 同款) 的 `GitignoreBuilder`，在工作目录同时 `add` `.autocorrectignore` 与 `.gitignore`，匹配用 `matched_path_or_any_parents`。CLI 还有第二层：`ignore::WalkBuilder` 开了 `git_ignore(true).parents(true)` (`autocorrect-cli/src/lib.rs:147`)，再对每个条目 `ignorer.is_ignored` (`:166`)。绝对路径会先 `strip_prefix(cwd)` 再匹配 (`:88`)。**显式传入的文件同样受约束** —— walker 根就是参数里的路径，命中 ignore 就 `continue`。stdin 模式不走 walker / Ignorer。LSP / Node / Python / Ruby / Java / Wasm 都包了同一份 `Ignorer`，但**没有**跨 SDK 共用的 ignore 测试夹具，各绑一层冒烟。

### 1.4 测试怎么组织

**事实。**

- **核心：** 各模块 `#[cfg(test)]`，大量 `HashMap<&str, &str>` 的输入 / 期望对 (`format.rs` 的 `assert_cases`)。Markdown 另有一份超长 inline `indoc` 对，再加 `tests/fixtures/markdown.raw.md` / `markdown.fixed.md`。其它语言是 `tests/fixtures/<lang>.raw.*` + `.fixed.*` + `.expect.json` (lint JSON)。
- **跨语言 SDK：** 不共享那套 fixture。`autocorrect-node/__test__/index.spec.mjs`、`autocorrect-py/test_autocorrect_py.py`、`autocorrect-rb/test/` 各写几条 `format("Hello你好.")` / `Ignorer` 冒烟，FFI 接到同一份 Rust 核心。`Makefile` 的 `test:node` / `test:python` / `test:ruby` / `test:java` 是各绑一层自己跑。
- **property / 幂等：** 仓内搜不到 `proptest` / `quickcheck` /「跑两遍」断言。`cargo criterion` 只做 bench。

lint 报告粒度是**整行** `old` / `new` (`LineResult`)，不带规则 id；一条上多个规则的改写揉在一行 diff 里。

---

## 2. zhlint

### 2.1 Markdown 怎么切出可见文本

**事实。** 设计文档把流程写成四步 (`docs/design.md:3`)：parse → apply rules → join → report。Markdown 不是规则自己切的，是 **hyper parser** 先把源码收成带槽位的块。

默认 hyper parser 链 (`src/options.ts:31`)：`ignore` → `hexo` → `vuepress` → `markdown`。

**Markdown** (`src/hypers/md.ts:234`) 用 `unified().use(remark-parse).use(remark-gfm).use(remark-frontmatter)`，是真正的 mdast，不是正则冒充 CommonMark。然后：

- 只把 `paragraph` / `heading` / `table-cell` 收成 block (`md.ts:34`)。围栏代码、HTML 块、thematic break、yaml front matter **不成 block**。
- yaml 节点直接 `return` (`md.ts:93`)，front matter 整块留在后来的 non-block 缝里，**值也不改**。
- 块内 inline：`emphasis` / `strong` / `delete` / `link` / `linkReference` 记成 `HYPER` 标记 (左右定界符，中间继续解析)；`inlineCode` / `break` / `image` / `imageReference` / `footnoteDefinition` / `html` 记成 `RAW` (`md.ts:57`)，整段 (含图片 alt) 不再进字符解析。
- 链接的 `startValue` 是 `[`，`endValue` 是 `](url)` (`md.ts:179`)，锚文字是可见文本，destination 在 HYPER 右标记里，规则改不到。

Hexo `{% ... %}...{% endx %}` 与 VuePress `:::` 容器用正则在 `modifiedValue` 里换成**等长** `@` 占位，偏移才能对上原文 (`src/hypers/hexo.ts:10`，`vuepress.ts:19`)。markdown parser 跑在 `modifiedValue` 上，抽 block 文本时却用原始 `data.value` (`md.ts:253`)。

每个 block 再交给自研字符 parser (`src/parser/parse.ts:78`)：逐字分类成西文 / CJK / 标点 / 括号 / 引号组，空格变成邻接 token 的 `spaceAfter` / `innerSpaceBefore`，不再占一种 token (`docs/design.md:57`)。hyperMarks 在对应 index 插入 RAW / HYPER，RAW 一次吃掉 `[startIndex, endIndex)`。

**写回。** `join` 读各 token 的 `modified*` 拼回块字符串 (`src/join.ts:104`)；`replaceBlocks` 按 block 的 `start` / `end` (unist `offset`，半开区间) 把改过的块嵌回原文，缝里的 non-block 原样切开拷贝 (`src/replace-block.ts:30`)。位置是**源码偏移**，report 再把 offset 换成行 / 列 (`src/report.ts:37`)。行数能否不变，取决于规则是否改 `spaceAfter` 里的 `\n`：`case-linebreak` 强制把带换行的 `spaceAfter` 还原 (`src/rules/case-linebreak.ts:19`)，这是保行的关键，但**没有**「输入输出行数相等」测试。`trimSpace` 理论上能削块首尾空格；对应单测在「latest remark parser」下 `test.skip` 了 (`test/rules.test.ts:20`)。

remark 认 CommonMark / GFM 的 \`\`\` 与 `~~~`。围栏代码不是 paragraph，落在 block 缝里，整块不改 —— 与 AutoCorrect「按语言再 format 一次」相反。

### 2.2 规则怎么组织

**事实。** 没有规则 id 表。单元是 `Handler = (token, index, group) => void` (`src/parser/travel.ts:3`)。`generateHandlers(options)` 返回固定顺序的闭包数组 (`src/rules/index.ts:24`)：

1. `space-trim`
2. `punctuation-width` / `punctuation-unification`
3. `case-abbrs` (把缩写的点改写**还原**)
4. 空格：`space-hyper-mark`、`space-code`、`space-letter`、`space-punctuation`、`space-quotation`、`space-bracket`
5. `case-linebreak` (还原换行)
6. `case-zh-units` / `case-html-entity` (还原)
7. `case-pure-western` (整段西文则清掉 validations、还原 modified)

`travel` 先序遍历 group 里每个 token 再递归子 group (`travel.ts:9`)。每条规则自己看 `Options` 里对应键：`true` 执行，`false` 常表示相反策略 (例如不要空格)，`undefined` 表示**既不报也不改** (`docs/design.md:192`，`src/rules/space-letter.ts:8`)。这不是 severity，是「动不动」三态。

**顺序与幂等。** 顺序是实现约定，设计文档用 L/P/Qo/Qi/Bo/Bi/D 的组合表对齐各空格规则 (`docs/design.md:262`)，注明「This part might vary frequently」。特殊情况走「先改再还原」：`space-letter` 会给 `2011年` 插空格，`case-zh-units` 若原文两侧本来就没有空格，就把 `modifiedSpaceAfter` 清掉并 `removeValidationOnTarget` (`src/rules/case-zh-units.ts:42`)。原文已有空格则不还原，所以 `2020 年1月1日…` 这种会保持「2020 后面那一个空格」(`test/example-units-fixed.md:25`)。没有第二遍 pass，没有幂等测试；`test/examples.test.ts` 的 Vue 文档用例是 `result === input` (已经合规的文章保持不变)，不是 `run(run(x)) === run(x)`。

`run()` 总是同时算出 `result` 字符串和 `validations`。CLI 不带 `--fix` 只打印报告、不写盘 (`bin/index.js:125`)，但规则层没有「只报不修」形态。没有 error / warning 分级，validation 只有 `name` / `message` / `index` / `length` / `target` (`src/report.ts:63`)。

### 2.3 disable / ignore 怎么实现

**事实。三种完全不同的机制。**

1. **整文件 disable。** `lint` 开头用 `/<!--\s*zhlint\s*disabled\s*-->/g` 搜原文，命中则 `origin === result`、`validations: []`、`disabled: true` (`src/run.ts:53`)。**不是区间**，注释出现在文件任何位置都整份跳过 (`test/example-disabled.md`)。
2. **按片段 ignore。** HTML 注释 `<!-- zhlint ignore: prefix-,start,end,-suffix -->` (`src/hypers/ignore.ts:23`)，语法仿 Scroll To Text Fragment (`src/ignore.ts:3`)。hyper parser 把解析结果推进 `ignoredByRules`；每块 `findIgnoredMarks` 在块字符串里找所有出现，得到 `[start, end)` (`src/ignore.ts:21`)。`join` 时若 token 与 mark 重叠，把对应 `modified*` 还原成原文，validation 进 `ignoredRuleErrors` 不外报 (`src/join.ts:14`)。`.zhlintcaseignore` 与 `Config.caseIgnores` 走同一套 `parseIngoredCase` (`src/rc/index.ts:145`，`src/options.ts:160`)。**不能按规则名关**，只能圈住一段字。
3. **忽略文件。** `.zhlintignore` 按行读进 `fileIgnores` (`src/rc/index.ts:119`)。CLI 用 npm `ignore` (gitignore 语法) `gitignore().add(config.fileIgnores).createFilter()`，再 `glob.sync` 出的路径转相对路径后过滤 (`bin/index.js:81`)。`createFilter()` 返回 true 表示**保留** (未被 ignore)。**显式传入的文件同样被过滤**。不自动读 `.gitignore` (与 AutoCorrect 不同)。发现只看 `--dir` (默认 cwd) 下那一份，不向上走。

### 2.4 测试怎么组织

**事实。** Vitest (`package.json` 的 `test:vitest`)。

- `test/rules.test.ts`：单规则 `getOutput` / `lint`，输入输出写在测试里，warning 对 `index` + `target` + `message`。
- `test/examples.test.ts`：整文件黄金对 (`example-units.md` / `example-units-fixed.md` 等)。
- `test/md.test.ts`：断言 hyper parser 抽出的 mark (链接 `[` / `](xxx)`、行内代码 RAW 等)。
- `test/lint.test.ts`：`ignoredCases` API。
- hexo / vuepress / report 各有文件。

单语言，没有 SDK 矩阵。没有 property 测试。消息字符串集中在 `src/rules/messages.ts` (中文)。

---

## 3. 对照 (事实)

| 问题 | AutoCorrect (`e1a75da`) | zhlint (`c8678fe`) | 本仓现状 |
| --- | --- | --- | --- |
| MD 切分 | pest PEG，不是 CommonMark | remark-parse + GFM + frontmatter (mdast) | 正则：围栏行、CommonMark 等长反引号、最简 `](` / 裸 URL (`zh_format.py` 的 `_is_fence` / `_code_spans` / `URL_DESTINATION`) |
| 代码块 | \`\`\` 按语言**递归 format**；无 `~~~` | 不成 block，缝里原样，\`\`\` / `~~~` 都跳 | 围栏行与内容整行豁免，\`\`\` 与 `~~~` 都认 |
| 行内代码 | 单 \` 定界，内容不改；`space-backticks` 在 MD 里几乎无效 | RAW token，`spaceOutsideCode` 管外侧 | 等长反引号串，内部豁免，R7 管外侧 |
| 链接 | 锚文字改、href 原子 | HYPER：`[` + 锚文字 + `](url)`，destination 不改 | destination 与裸 URL 全局豁免 |
| 图片 | alt 当 link_string 会改 | 整段 RAW，alt 也不改 | 未单独建模；alt 落在行文里会走规则 |
| HTML | 标签不改、inner_text 改；注释当正文改 | 块级 HTML 在缝里；inline html 为 RAW | 无 HTML 豁免 |
| front matter | `---` 块内普通值会改，`tags:` 特例 | yaml 节点整块跳过 | 规范未提，按行文处理 |
| 写回 | pair 拼接；parse 失败回原文 | 按 offset 嵌块 (`replaceBlocks`) | 逐行 `split("\n")` / `join`，**不增删行**是规范 (`spec/rules.md`「处理单位」) |
| 位置 | pest `line_col`；lint 报整行 old/new | 源码 offset → 行 / 列；按 token 部件 | 行号 + 规则 id，无列号 |
| 规则单元 | 具名 `fn(&str) -> Cow<str>`，两张静态表 | `Handler` 闭包 + `Options` 三态布尔 | 稳定 id R1–R11，规范正本在 `spec/rules.md` |
| 顺序 | `RULES` 然后 `AFTER_RULES`，实现细节 | `generateHandlers` 数组，先改再还原特例 | 「修复顺序」是契约，各实现必须一致 |
| 幂等 | 单遍，无 fixpoint，无测试 | 单遍 + 还原，无 `run(run(x))` 测试 | `--fix` 循环到不动点；runner 断言 `fix(fixed) == fixed` |
| 只报不修 | Warning：lint 跑、format 不跑 | 无；`undefined` = 不报不改。CLI 不写盘只是不保存 | check 与 fix 同一启用集；tracker 有 non-fixable / warning backlog |
| 行内 disable | 注释状态机，可带规则名，后续 pair 生效 | `zhlint disabled` 整文件；`zhlint ignore:` 按片段，不按规则名 | 无 (tracker 要做) |
| 忽略文件 | `.autocorrectignore` + `.gitignore`，`ignore` crate；显式文件也过滤 | `.zhlintignore`，npm `ignore`；不读 `.gitignore`；显式文件也过滤 | 无 |
| 黄金样例 | 模块内 map + `tests/fixtures/*.{raw,fixed,expect}` | `test/example-*.md` 对 + 单测内字符串 | `spec/fixtures/<case>.{in,fixed,findings}`，可选 `.conf` |
| 跨语言共享测试 | 不共享 fixture，SDK 各自冒烟 | 无 SDK | 语言无关黄金集，实现只写薄 runner (`spec/README.md`) |

---

## 4. 对本仓 Rust 终态的 distill 判断

以下是观点。只谈机制与理由，不涉及 crate 布局。本仓已拍板的模型 (逐行、不增删行、check 看原文、fix 到不动点、规则 id 稳定、启用集 = (默认集 ∪ enable) − disable) 当约束，不建议用任一侧的默认行为去推翻。

### 4.1 值得抄

1. **「抽出可见文本 → 改 → 按原偏移嵌回」** (zhlint 的 block + `replaceBlocks`)，而不是 AutoCorrect 那种「PEG 切 pair 再拼接」。理由：本仓契约是语法结构不动、行数不变；嵌回只替换 mdast 认定的 paragraph / heading / table-cell，围栏、列表标记、yaml 自然留在缝里。AutoCorrect 的拼接依赖「文法恰好划分全文」，而那份 Markdown 文法已经不是 CommonMark (无 `~~~`、单反引号、注释当正文、代码块递归 format)。Rust 若换 parser，应对齐这条写回策略，而不是对齐 pest 那份文法。
2. **Parse 失败则整份原文返回** (AutoCorrect `FormatResult::error`)。理由：排版 linter 改坏源码的代价高于漏报；本仓现在正则路径很少会「解析失败」，一旦上真实 parser，这条是必要的安全网。
3. **行内 disable 用文档序状态机，且能带规则 id** (AutoCorrect `Toggle`)。理由：tracker 要的逃生口就是「单点误报不必动配置」。zhlint 的整文件 `disabled` 太粗；它的片段 ignore 圈的是字而不是规则，关不掉「这一段只跳过 R4」。状态机比「只作用于注释所在行」更能盖住围栏前的一段，也比源码区间标记省掉配对闭合。建议注释语法跟本仓规则 id (`R4`) 对齐，不要引入 `space-word` 这种外部分名。
4. **忽略文件用 gitignore 语法、显式传入的路径也过滤** (两侧 CLI 都是如此)。理由：与「我写在命令行上就一定要检」相反，两侧都选择了 ignore 赢，避免 CI 脚本 `git ls-files '*.md'` 把生成物又送回来。Rust 侧直接用 `ignore` crate 即可 (AutoCorrect 已验证)。要不要像 AutoCorrect 那样**默认叠加 `.gitignore`**，宜单独拍板：叠加省一份名单，但会让「源码在 gitignore 里但仍想 lint」的生成文档消失；zhlint 只读自己的 ignore 文件，更可预测。无论选哪头，显式路径与忽略文件的关系必须写进规范，两侧源码都没有「强制检」开关。
5. **等长占位保住偏移** (zhlint 给 Hexo / VuePress 的 `@`.repeat(length))。理由：本仓若永远只做 CommonMark，现在不必做；一旦要豁免某种「看起来像正文、其实是指令」的结构，先占位再 parse 比把方言写进文法便宜。
6. **lint 报告带列号 / 偏移** (AutoCorrect `c`，zhlint `index`)。理由：现在规范只要求行号 + 规则 id，黄金集也按这个比。Rust 终态若要对标 LSP (ADR-0002 / tracker)，内部保留列或 byte offset 是便宜的，对外 fixture 不必改。不要学 AutoCorrect 把一条上的多规则揉成整行 `old`/`new` —— 本仓 `.findings` 已经是「同一行同一规则可出现多次」。
7. **特例用「先改再还原」只适合选项极多、规则共享同一 token 流的时候** (zhlint 的 `skipZhUnits` / `skipAbbrs` / linebreak)。本仓已经把 `skip_zh_units` 写进 R5 判定、缩写表写进 R1，**不必再抄还原 pass**。值得抄的是这个观察：空格规则与「不要动换行」冲突时，zhlint 用强制还原换行保住行数 (`case-linebreak`)；本仓因为根本不在 token 的 `spaceAfter` 里改 `\n`，没有这个问题，但若 Rust 换 token 流，这一条要一起带走。

### 4.2 不要抄

1. **不要抄 AutoCorrect 的 Markdown pest 文法。** 它不是 CommonMark：不认 `~~~`、行内代码不是等长反引号串、会 format HTML 注释、会递归 format 围栏内容、front matter 普通值会改、图片 alt 会改。这些每一条都直接违反本仓「全局豁免」第 1–3 款 (围栏整块、行内代码内部、destination / URL)。`context.codeblock` 默认开，与本仓默认相反。既有调研 §4.1 第 6 条「按文件类型 AST 只扫字符串和注释」对代码语言成立，对 Markdown **不成立**，源码级应以此处为准。
2. **不要抄 `space-backticks` 那种「规则假定反引号还在同一段文本里」的实现。** Markdown 一切 token，规则就打空；配置默认开造成「以为在工作」。本仓 R7 显式按 span 定界串的外侧判定，这个模型要保住。
3. **不要抄 zhlint 那份巨大的 boolean `Options`。** 既有调研已说与「规则 id 稳定不复用」不合。源码还确认：`undefined` / `true` / `false` 三态、先改再还原、消息字符串与 handler 一一对应，迁移成本高，且没有本仓需要的「关 R3、其它保持」这种启用集运算。
4. **不要抄 AutoCorrect 的 Warning = 只报不修，当作默认严重级别模型。** 机制本身 (lint 跑、fix 跳过) 干净，但本仓当前契约是启用集内 check 与 fix 同行为。tracker 的 non-fixable 若落地，宜做成规则属性 (ruff 的 fixable)，不要做成配置里的 0/1/2 与规则表缠在一起。也不要抄 `textRules` 的 `contains` 覆盖 —— 子串命中就整段还原，调试时看不出是哪条规则让步，和「未知 id 必须报错」的本仓配置哲学相反。
5. **不要抄「单遍 fix、靠作者保证幂等」。** 本仓已经因为「R2 换出的半角括号会改变链接 destination 豁免范围」而把不动点写进规范 (`spec/rules.md`「处理单位」；`zh_format.py` `fix_text` 的 `while True`)。AutoCorrect / zhlint 都没有这条，也没有测试。Rust 实现必须继续对着 `spec/fixtures` 跑三条断言，而不是对着它们的单遍模型降格。
6. **不要抄 SDK 冒烟当黄金集。** AutoCorrect 多语言绑定各写 `Hello你好.`，真正的 Markdown 行为只在 Rust 测试里。本仓 ADR-0001 的 `spec/fixtures/` 才是该抄的共享方式；Rust 终态加绑定的话，绑定测试保持冒烟即可，不要再复制一份 fixture。
7. **不要抄 zhlint 整文件 `disabled` 当主逃生口，也不要把 case ignore 当作 disable 规则的替代。** 片段 ignore 适合「这一串 `( , )` 是 API 签名」这种真·原文如此；它按文本搜索所有出现，容易误伤。本仓若做，应是 disable 注释的补充，不是主机制。
8. **不要抄「英文段落把全角标点收成半角」** (AutoCorrect `halfwidth-punctuation` + CJK block 临时关)。本仓 R1 是 CJK 旁半角 → 全角，反向转换不在默认集；家规括号方向已与 zhlint 默认同向 (既有调研 §2.4)，不要为了对齐 AutoCorrect 再加一条未进 spec 的反向规则。

### 4.3 对既有调研的校准 (源码改了 README 印象的地方)

既有调研 §4 在配置层是对的。源码要求改口的只有这些：

- AutoCorrect 在 Markdown 里**会改代码块** (默认)，不是「只扫可见文本」。
- AutoCorrect `space-backticks` 默认开，但 Markdown 路径几乎不生效。
- zhlint `skipZhUnits` 不是「判定时跳过」，是「插完再还原，且只还原原文没有空格的边界」。本仓 R5 的 `skip_zh_units` 是判定豁免，语义更干净，保持。
- 两侧 CLI 对**显式文件**都应用 ignore，README 不一定写清。写本仓规范时要写死。

**未核实：** AutoCorrect `WalkBuilder` 在「路径是 walker 根且同时被 gitignore」时，是根节点仍发出再被 `Ignorer` 丢掉，还是 walker 根本不发出 —— 源码两种过滤都在，本文按「最终不处理」描述，未单步跑 CLI 证实哪一层先生效。zhlint `replaceBlocks` 在 heading 节点 offset 是否包含 `#` 标记：mdast 通常包含，字符 parser 会看到 `#`；`example-units.md` 几乎没有 ATX 标题，heading 标记会不会被空格规则改写，本文未用 vitest 实测。
