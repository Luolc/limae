# ADR-0018：`SKILL.md` 的 frontmatter 必须是合法 YAML (yaml-frontmatter-1)

- 日期：2026-10-09
- 承接：ADR-0006 (三轴属性)、ADR-0010 (规则 id 的格式)

## 背景

用户 2026-10-09 提出这项检查 (经协调方转达)。起因是另一个仓的一份 `SKILL.md`：description 里有一个没加引号的「: 」，YAML 解析失败，GitHub 渲染时报错，skill 的加载方也读不到这段 frontmatter。这种错在本地看不出来，写的人和审的人都容易漏掉。limae 已经挂在各仓的 pre-commit 和 CI 上，在这里实现、测试一次，各仓升级后就都有了，不必各写一份脚本。

## 决定

1. **新增规则 `yaml-frontmatter-1`，新开 `yaml-frontmatter` 一族**。按 ADR-0010，语言段写这条规则判的是哪种文本：它判的是 YAML，不是中文或英文行文，所以语言段是 `yaml`；家族段写它管的那一类问题，即文件开头的 frontmatter。以后再加 frontmatter 的检查 (比如必填键)，序号接着排。
2. **三轴取 non-fixable · error · stable，默认启用**。不合法的 frontmatter 一定是坏的，不存在误报率待验证的问题，所以不走 experimental；它要在提交时直接挡住，所以是 error；同一个错误有好几种改法，没有唯一修法，所以不修复。
3. **只查文件名正好是 `SKILL.md` 的文件，只查第一行 `---` 到下一行 `---` 之间的块**。frontmatter 是 skill 格式的约定，别的 Markdown 开头的 `---` 可能只是分隔线。这是第一条判定依赖文件名的规则：`Pipeline::check_file` 带上路径，原来的 `Pipeline::check` 照旧只看文本；黄金集加一个可选的 `<case>.path` 文件来表达文件名。
4. **有开头没闭合也报，报在第 1 行**。第一行是 `---` 却找不到闭合的 `---`，这份 skill 一定读不出来；不报就等于放过一种比「: 」更严重的坏法。
5. **解析器用 `saphyr-parser`**。它是纯 Rust 的 YAML 1.2 事件解析器，没有 `unsafe`，只产出事件、不构造类型，tag 不会被执行；新增的传递依赖只有 `arraydeque` 一个 (`thiserror` 本来就有)。本规则只需要「解析成不成功、错在哪一行哪一列」，事件流正好够用。

## 放弃的方案

- **`serde_yaml`**：已停止维护。基于 libyaml 的几个后继 (`serde_yaml_ng`、`serde_norway` 等) 实现的是 YAML 1.1，还要带上整套 serde 反序列化，而这里用不到反序列化。
- **`yaml-rust2`**：同样是 YAML 1.2，但除了解析器还带文档模型，默认还拉上 `encoding_rs` 与 `hashlink`；两者都源自 `yaml-rust`，`saphyr-parser` 只有解析器这一层，更小。
- **查所有 Markdown 的 frontmatter**：会把普通文档开头的分隔线当成 frontmatter，误报，而且别的 Markdown 本来就没有这个约定。
- **顺带查结构** (重复键、缺 `name` / `description`)：事件解析器不判重复键，结构检查要另写一层；需求只要语法这一项，等有实际需要再开新序号。

## 后果

- 各仓升级 limae 后，`SKILL.md` 里不合法的 frontmatter 会让 pre-commit 与 CI 变红；要豁免某个文件用 `.limae-ignore`，要整条关掉用配置的 `disable`；行内指令写进 YAML 块本身就会让它不合法。
- 规则从 21 条变成 22 条，默认集从 10 条变成 11 条。

## 状态

已采纳。
