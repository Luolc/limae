# ADR-0001：独立成仓、规范先于实现、多实现共用黄金 fixture

- 日期：2026-08-31
- 依据：wealth-management 仓 `python/lo-linting` 的迁出方案 (仓名 `lo-md-lint` 与简称 `mdlint` 由用户 2026-08-31 定案)

## 背景

中文 Markdown 排版检查器目前是 wealth-management 仓 uv workspace 里的一个子项目 (`lo-linting`，命令 `lo-lint-zh`)，零第三方依赖，规则来自全局守则「语言规范」。它只被 wealth-management 自己经 pre-commit `repo: local` 消费；machine-setup、butler 等其它中文仓也需要同一把 linter。下一步要加英文规则、把规则 flag 化，长期大概率用 Rust 重写——趁代码还小，先把仓、规范与测试资产的形态定下来。

## 决定

三件事作为一个决定，缺一不可：独立仓是多个消费方的前提，规范与共用 fixture 是多个实现的前提。

### 一、独立成仓

- 从 wealth-management 迁出到本仓 `Luolc/lo-md-lint`，**不带历史，一个 initial commit** (记录来源仓与来源 SHA)。理由：wealth-management 是 private 仓，其提交信息与历史版本含用户理财细节 (账户、持仓、profile)，带历史迁出等于把这些搬进 public 仓；这批代码只有十来个提交，历史的价值不抵这个风险。
- 消费方不再 `repo: local`：经 pre-commit 远端 hook (`repo: https://github.com/Luolc/lo-md-lint` + `rev`) 或 `uvx` / `uv tool install` 消费。发布形态的细节由分发 PR 定。

### 二、规则规范先于实现

- 每条规则写成语言无关的规范：一个稳定 id、一个可开关的 flag 名、判定条件、修复行为、豁免范围 (代码块、行内代码、链接语法等)。
- 中文排版规则是默认规则集。
- 规范是实现的上游：实现或 fixture 与规范不一致时，先改规范，再改实现。

### 三、多实现共用一套黄金 fixture

- 黄金集 = 输入 Markdown + 期望输出 (报告的违规、`--fix` 后的文本)，语言无关。
- Python 版是参考实现 (reference implementation)；任何未来实现 (Rust 等) 对着同一套 fixture 跑，全过才算合规。

### 位置：仓根 `spec/`

规范 `spec/rules.md` 与黄金集 `spec/fixtures/` 放在一起，仓根，与各实现平级。理由：

1. 规范与 fixture 是同一份契约的两半 (文字与可执行)，同目录、同 PR 改，不会漂移。
2. 不放 `docs/`：`docs/` 是给人读的内容 (ADR / 手册 / 调研)；规范是实现的输入，随实现的测试一起变更，属于代码域。
3. 不放 `tests/`：仓根 `tests/` 是 Python 参考实现的测试目录 (Python 以仓根为项目根，见 `AGENTS.md`「目录约定」)，黄金集不是任一语言的测试代码，放进去等于让 Python 当「主人」。
4. 不放任何实现的私有目录 (`src/`、`tests/`、`crates/`)：每个实现用相对路径 `spec/fixtures/` 引用，没有谁是上游。

## 后果

- 本仓对外是一个独立的 pre-commit hook / CLI；wealth-management 等消费方各自开 PR 改配置指到这里。
- 规则改动的顺序固定：先改 `spec/`，再改各实现；每加一条 fixture 就是所有实现的回归测试。
- Python 版的现有测试要拆成「fixture 数据 + 薄 runner」，别的实现才能复用；这由迁入之后的独立 PR 做。
- 迁出后 wealth-management 删除 `python/lo-linting`，改为消费本仓。

## 状态

accepted (用户 2026-08-31 决定)。修订 2026-08-31：迁入不带历史 (见「一、独立成仓」第一条)。
