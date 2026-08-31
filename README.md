# lo-md-lint

Markdown linter，从中文技术写作的排版规则起步。

## 定位与愿景

- **规则先于实现**：每条规则是一段语言无关的规范 (specification)，写在 `spec/rules.md`，有一个稳定的 id、可以逐条关掉；中文排版规则 (中英文之间空格、数字与中文之间空格、半角括号外空格、全角标点……) 是默认规则集，之后加英文规则。
- **多实现、一套黄金 fixture (golden fixtures)**：黄金集在 `spec/fixtures/`，Python 版是参考实现 (reference implementation)，任何后续实现都对着同一套「输入 / 期望输出」跑，通过即合规。
- **对标 ruff 之于 Python**：长期大概率以 Rust 为主实现 —— 一个 Rust 写的 Markdown lint，可被 Python / Node 生态经 pre-commit、包管理器等集成，也能直接当命令行工具用。
- **配置走 toml**：独立配置文件 `lo-md-lint.toml` 或 `pyproject.toml` 的 `[tool.lo-md-lint]` 表，两者同构，用 `disable` / `enable` 两个键逐条开关规则；绝大多数规则默认启用，个别默认关闭的规则在规范条目里标明。

决策记录在 `docs/adr/`；agent 守则在 `AGENTS.md`。

## 现状

Python 参考实现已就位，规则集是中文排版一套：宽度转换 (R1 CJK 旁的半角标点含句号、R2 全角括号、R10 全角数字)、空格 (R3 半角括号外侧、R4 CJK–拉丁字母、R5 CJK–数字、R6 数字–单位、R7 行内代码定界符、R8 破折号两侧、R11 全角标点旁去空格、R9 链接前，默认关)。每条都可单独开关 (ADR-0003 / ADR-0004)。`spec/` 已建起来：规则规范在 `spec/rules.md`，黄金 fixture 在 `spec/fixtures/`，格式与 runner 的判定见 `spec/README.md`；Python 的薄 runner 是 `tests/test_fixtures.py`。

## 使用

作为 pre-commit 远端 hook (推荐)，`rev` 固定到一个 tag ([tag 列表](https://github.com/Luolc/lo-md-lint/tags))：

```yaml
repos:
  - repo: https://github.com/Luolc/lo-md-lint
    rev: <tag>
    hooks:
      - id: lo-md-lint
```

默认只检查、不修复；要自动修复就自己加 `args: ["--fix"]`。

不接 pre-commit、手动或在 CI 里一次性跑：

```sh
uvx --from git+https://github.com/Luolc/lo-md-lint@<tag> lo-md-lint --all
uvx --from git+https://github.com/Luolc/lo-md-lint@<tag> lo-md-lint <file>...
```

### 开关某条规则

启用集 = (默认集 ∪ `enable`) − `disable`，不写配置就是默认行为。配置模型的正本是 `spec/rules.md`「配置」，这里只举例。

`pyproject.toml` 里 (Python 项目)：

```toml
[tool.lo-md-lint]
disable = ["R3"]
enable = ["R9"]
```

或者仓库根放一个 `lo-md-lint.toml` (键结构相同，只是不带表头)：

```toml
disable = ["R3"]
enable = ["R9"]
```

临时在命令行上开关，整体覆盖配置文件：

```sh
lo-md-lint --disable R3 <file>...
lo-md-lint --disable R1,R3 --all
lo-md-lint --enable R9 --all
```

## 本地开发

```sh
uv sync                          # 建 .venv、装 dev 依赖
uv run pre-commit install        # 装本地钩子 (只需一次)
uv run lo-md-lint --all          # 检查全部 tracked Markdown
uv run lo-md-lint --all --fix    # 自动修复大部分违规后复查
uv run lo-md-lint <file>...      # 检查指定文件
uv run pytest -q                 # 测试
```
