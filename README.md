# lo-md-lint

Markdown linter，从中文技术写作的排版规则起步。

## 定位与愿景

- **规则先于实现**：每条规则是一段语言无关的规范 (specification)，有一个稳定的 id 与一个可开关的 flag；中文排版规则 (中英文之间空格、数字与中文之间空格、半角括号外空格、全角标点……) 是默认规则集，之后加英文规则。
- **多实现、一套黄金 fixture (golden fixtures)**：Python 版是参考实现 (reference implementation)，任何后续实现都对着同一套「输入 / 期望输出」跑，通过即合规。
- **对标 ruff 之于 Python**：长期大概率以 Rust 为主实现——一个 Rust 写的 Markdown lint，可被 Python / Node 生态经 pre-commit、包管理器等集成，也能直接当命令行工具用。
- **配置走 toml**：独立配置文件或 `pyproject.toml` 的 `[tool.lo-md-lint]` 表，逐条规则开关。

决策记录在 `docs/adr/`；agent 守则在 `AGENTS.md`。

## 现状

Python 参考实现已就位，规则集只有中文排版一套 (R1 CJK 旁的半角标点、R2 全角括号、R3 半角括号外侧空格)，尚未 flag 化；`spec/` 还没建起来。安装与被别的仓消费的方式留给分发 PR。

## 本地开发

```sh
uv sync                          # 建 .venv、装 dev 依赖
uv run pre-commit install        # 装本地钩子 (只需一次)
uv run lo-md-lint --all          # 检查全部 tracked Markdown
uv run lo-md-lint --all --fix    # 自动修复大部分违规后复查
uv run lo-md-lint <file>...      # 检查指定文件
uv run pytest -q                 # 测试
```
