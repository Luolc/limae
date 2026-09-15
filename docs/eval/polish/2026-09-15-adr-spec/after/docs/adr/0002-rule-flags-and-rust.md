# ADR-0002：规则 flag 化、`[tool.lo-md-lint]` 配置表与 Rust 主实现

- 日期：2026-08-31

## 背景

ADR-0001 定下规范先于实现，以及多实现共用 fixture；本批只做搬迁，不改变规则形态。

## 决定

下一阶段方向如下，本批暂不实施：

- 每条规则可单独开关 (flag 化)，默认集为中文排版规则。
- 通过 toml 配置：独立配置文件或 `pyproject.toml` 的 `[tool.lo-md-lint]` 表。
- 主实现转为 Rust，定位类似于 ruff 之于 Python；Python 版保留为参考实现。

细节 (flag 命名、配置文件发现顺序、Rust crate 布局与分发) 届时另起 ADR 确定，不在此展开。

## 状态

proposed (2026-08-31)。flag 化与 `[tool.lo-md-lint]` 配置表已由 ADR-0003 落地 (2026-08-31)；Rust 主实现部分仍 proposed。
