# spec：规则规范与黄金 fixture

本目录是所有实现共用、与语言无关的契约，与各语言实现平级 (ADR-0001)：

- `rules.md` —— 规则的来源：稳定 id、判定条件、修复行为、豁免范围。
- `fixtures/` —— 黄金集 (golden fixtures)：可执行的部分，每加一个 case 都会成为所有实现的回归测试。
- `wordlists/` —— 词表：zh-tell-1 / zh-tell-3 / zh-tell-4 / en-tell-1 / en-tell-3 / zh-tell-5 / zh-word-1 / zh-word-2 的判定数据；格式与匹配语义以 `rules.md`「词表」为准。
- `lexicon/` —— AI 中文词典：收录读得懂但没人这么说的词，每条说明「实际想说什么 / 中文里怎么说 / 带上下文的例子」。它有三个用途：渲染进 `limae polish` 的中文层作为判据，渲染成 skill 的 `references/zh/lexicon.md`，或转成 `wordlists/` 的词表供 linter 直接报告；前两个都由 `rust/polish/lexicon.rs` 的同一个函数渲染。判据是「一个母语者会不会这样说」，不是「读者能不能看懂」。
- `skill/` —— agent skill 的手写部分：`SKILL.md` 包含 front matter (`name` / `description`) 和语言无关的正文 (判据、文件地图、边界)，`zh.md` 是中文指南；`skills/write-naturally/` 由 `cargo run --example render-skill` 根据这两份文件和 `lexicon/zh.toml` 生成，是入库产物，不在这里手改。skill 与 `polish/` 共用同一份词典，但装配清单不同：skill 不带 polish 的输出契约，polish 不带 skill 的文件地图。
- `polish/` —— `limae polish` 的 prompt spec：由通用层 `general.md` (英文) 和每种语言各自的层 (`zh.md`) 组成，形制以 ADR-0008 §九为准。它是 prompt，不是判定数据 —— 词表不放进 prompt。

这三部分位于同一目录，必须在同一个 PR 中修改，避免不一致。改规则时，先改这里，再改各实现。

## 词表

`wordlists/zh-tell-1.txt`、`zh-tell-3.txt`、`zh-tell-4.txt`、`en-tell-1.txt`、`en-tell-3.txt` 一行一条 (`#` 注释和空行忽略)，`wordlists/zh-word-1.toml` 是包含 `wrong` / `right` / `anchors` 的 `entries` 数组。中文词表按字面子串匹配；英文词表 (`en-tell-1` / `en-tell-3`) 按整词匹配，且不区分大小写。`wordlists/zh-tell-5-allow.txt` 与 `zh-word-2-allow.txt` 格式相同，但方向相反，属于**豁免表**：只有命中它才不报告 —— 语义都以 `rules.md`「词表」为准。

**各实现都从同一份 `spec/wordlists/` 读取词表，不在代码中手抄词条**。Rust 通过 `rust/resources.rs` 的 `include_str!` 在构建时嵌入；文件缺失会导致构建失败，运行时不会再查找资源路径。修改词表只改这里；Rust 需要重新构建发行版，但规则逻辑不变，见 [ADR-0015 §三](../docs/adr/0015-rust-migration.md#三共享资源与必要依赖)。

## fixture 文件格式

一个 case 由三个同名文件和一个可选的第四个文件组成，`<case>` 是 case 名：

| 文件 | 内容 |
| --- | --- |
| `<case>.in` | 输入的 Markdown 文本 |
| `<case>.fixed` | 期望的 `--fix` 输出；「应保持不变」的 case 与 `.in` 逐字相同 |
| `<case>.findings` | 期望的违规列表，每行 `<行号> <规则 id>`，如 `3 zh-typography-1`；空文件表示无违规 |
| `<case>.conf` | 可选：这个 case 的配置，内容就是独立配置文件 `limae.toml` 的内容 —— 配置键见 `rules.md`「配置」；没有这个文件 = 默认配置 |

约定：

- **三个文件都必须存在**，都是 UTF-8，并且都以一个换行结束。空的 `.findings` 是零字节文件 —— 不允许省略，否则无法区分省略和「无违规」。
- **`.conf` 只在这个 case 需要修改配置时才存在**：它是唯一可选的文件，其余三个始终齐全。带 `.conf` 的 case 使用它算出的配置运行 (启用集见 `rules.md`「配置」，其余键使用 `rules.md` 的默认值)，不带的使用默认配置 —— 「配置」本身也是规范的一部分，因此配置维度的期望也要手写进 `.fixed` 与 `.findings`。
- **`.findings` 的顺序与实现报告违规的顺序一致**：先按行号升序，同一行内按 `rules.md` 的规则顺序 (zh-typography-1、zh-typography-2、……)，同一条规则内按出现位置。同一行的同一条规则可以出现多次，如 `（测试）` 的两处 zh-typography-2。
- **严重度不写进 `.findings`**：某处违规是 error 还是 warning，只由 `rules.md`「规则属性」的默认值和这个 case 的 `.conf` 决定，runner 需要时自行推导 —— 因此 `severity` 键不会改变任何 fixture 文件的内容。
- **`.in` 与 `.fixed` 逐行对齐**：修复不增删行 (见 `rules.md`「处理单位」)，两个文件的行数始终相同。
- **只用合成假数据**：如 `ACME`、`$1,000`、`Foo`；不得出现任何真实的个人或业务信息 (`AGENTS.md`「隐私边界」)。
- **行内指令是 `.in` 的普通内容**：`rules.md`「行内指令」中的注释行由被测实现自行解析，runner 不做特殊处理 —— 指令行既不出现在 `.findings` 中，也不会被 `.fixed` 改写。
- **后缀不是 `.md`**：fixture 的 `.in` 有意包含违规；若使用 `.md` 后缀，会被本仓自己的 dogfooding 钩子和 `limae --all` (它递归扫描当前目录下所有 `*.md`) 扫到并报红。换后缀比在钩子中加 `exclude` 更省事，也避免把 fixture 当作文档。

## case 的粒度

- **单行 case 按主题放在同一个文件中**：一行一个 case，**case 之间用空行隔开** —— 空行让每个 case 分别成块，行内代码的反引号不会跨 case 配对，聚合前后的判定完全一致。当前主题文件包括每条规则至少一个 (`zh-typography-1-cjk-punct` 到 `zh-typography-11-fullwidth-punct-space`)，以及 `inline-code-spans`、`url-protection`、`mixed-rules`、`mixed-spacing`、`mixed-punct`。
- **跨行 case 单独放在一个文件中**：围栏代码块、跨行的行内代码、行内指令、引用块、表格和列表的判定依赖行之间的关系，聚合会改变语义。
- **忽略文件无法表达**：`.limae-ignore` (`rules.md`「忽略文件」) 决定「哪些文件进入本次运行」，三个 fixture 文件无法表达这一点，由各实现在各自的测试中覆盖。

## runner 的判定

每个实现都写一个简单的 runner，遍历 `fixtures/*.in`，根据 `<case>.conf` 算出该 case 的配置 (没有该文件则使用默认配置)，再用它运行 `check` 与 `fix`，并对每个 case 断言三项：

1. **修复正确**：`fix(<case>.in) == <case>.fixed`。
2. **修复幂等 (idempotent)**：`fix(<case>.fixed) == <case>.fixed`。
3. **报告正确**：`check(<case>.in)` 的 (行号、规则 id) 列表 == `<case>.findings`。

除这三项外，runner 不作其他断言，**尤其不检查退出码** —— 退出码是 CLI 的契约 (`rules.md`「退出码」)，由各实现在自己的测试中覆盖。

Rust 的 runner 是 `rust/tests/pipeline.rs` 中的 `all_golden_cases_use_the_document_api` —— 除这三项断言外不加其他逻辑，其他语言照抄即可。

## 加一个 case

**先手写期望，再跑 runner**：`.fixed` 与 `.findings` 是规范的一部分；根据实现输出反向生成，相当于让实现自己出考题。写完后运行一次 runner；如果报红，先判断是期望错了还是实现错了 (「linter 是工具，不是法律」)。
