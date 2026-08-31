# spec：规则规范与黄金 fixture

本目录是所有实现共用的契约，语言无关，与各语言实现平级 (ADR-0001)：

- `rules.md`——规则的正本：稳定 id、判定条件、修复行为、豁免范围。
- `fixtures/`——黄金集 (golden fixtures)：可执行的那一半，每加一个 case 就是所有实现的回归测试。

两半同目录、同 PR 改，不会漂移。改规则的顺序固定：先改这里，再改各实现。

## fixture 文件格式

一个 case 由三个同名文件组成，`<case>` 是 case 名：

| 文件 | 内容 |
| --- | --- |
| `<case>.in` | 输入的 Markdown 文本 |
| `<case>.fixed` | 期望的 `--fix` 输出；「应保持不变」的 case 与 `.in` 逐字相同 |
| `<case>.findings` | 期望的违规列表，每行 `<行号> <规则 id>`，如 `3 R1`；空文件表示无违规 |

约定：

- **三个文件都必须存在**，都是 UTF-8、都以一个换行结束。空的 `.findings` 就是零字节文件——不允许省略，省略与「无违规」无法区分。
- **`.findings` 的顺序是实现报告违规的顺序**：先按行号升序，同一行内按 `rules.md` 的规则顺序 (R1、R2、R3)，同一条规则内按出现位置。同一行的同一条规则可以出现多次，如 `（测试）` 的两处 R2。
- **`.in` 与 `.fixed` 逐行对齐**：修复不增删行 (见 `rules.md`「处理单位」)，两个文件的行数永远相同。
- **只用合成假数据**：`ACME`、`$1,000`、`Foo` 这类；不得出现任何真实的个人或业务信息 (`AGENTS.md`「隐私边界」)。
- **后缀不是 `.md`**：fixture 的 `.in` 故意含违规，若用 `.md` 后缀就会被本仓自己的 dogfooding 钩子与 `lo-md-lint --all` (它按 `git ls-files '*.md'` 取文件) 扫到并报红。换后缀比在钩子里加 `exclude` 更省事，也让 fixture 不被当成文档。

## case 的粒度

- **单行 case 按主题聚成一个文件**：一行一个 case，**case 之间用空行隔开**——空行让每个 case 各自成块，行内代码的反引号不会跨 case 配对，聚合前后的判定完全一致。当前的主题文件是 `r1-cjk-punct`、`r2-fullwidth-parens`、`r3-paren-spacing`、`inline-code-spans`、`mixed-rules`。
- **跨行 case 单独成一个文件**：围栏代码块、跨行的行内代码、引用块、表格、列表这些的判定依赖行与行的关系，聚合会改变语义。

## runner 的判定

每个实现写一个薄 runner，遍历 `fixtures/*.in`，对每个 case 断言三条：

1. **修复正确**：`fix(<case>.in) == <case>.fixed`。
2. **修复幂等 (idempotent)**：`fix(<case>.fixed) == <case>.fixed`。
3. **报告正确**：`check(<case>.in)` 的 (行号、规则 id) 列表 == `<case>.findings`。

Python 参考实现的 runner 是 `tests/test_fixtures.py`，不到三十行——三条断言之外不加别的逻辑，别的语言照抄即可。

## 加一个 case

**先手写期望，再跑 runner**：`.fixed` 与 `.findings` 是规范的一部分，从实现的输出反向生成等于让实现给自己出考题。写完跑一遍 runner，红了先判断是期望错还是实现错 (「linter 是工具，不是法律」)。
