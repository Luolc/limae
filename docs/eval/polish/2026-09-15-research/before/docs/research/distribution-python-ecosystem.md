# limae 分发调研：Rust linter 怎么让别人 (尤其是 Python 项目) 用起来

> **本文是第一轮调研，部分推荐已被第二轮修订，以第二轮为准**：见 [`distribution-cross-ecosystem.md`](distribution-cross-ecosystem.md) 第 8 节「回头看上一轮，改什么、不改什么」。本文保留作演进记录，不回填第二轮的结论。

调研日期：2026-09-09。只读，未改本仓。

## 一句话结论

**下一件事是把 Rust `limae` 打成 PyPI 上的平台 wheel (`pip install limae` / `uv add --dev limae`)**，机制照抄 ruff：maturin `bindings = "bin"`，wheel 标签 `py3-none-<platform>`。pre-commit 默认 hook 改成「tiny mirror + `language: python` + 依赖那个 wheel」。CI 不要做官方 action。配置文件以独立的 `limae.toml` 为 canonical，`[tool.limae]` 只当 Python 项目的便利入口。

现在的 crates.io + GitHub Release 对「他自己的 Python 项目」几乎帮不上忙：Python 开发者不会 `cargo install`，也不会在别人的仓里预装 Rust。

## 现状 (本仓，读到原文)

| 渠道 | 状态 | 出处 |
| --- | --- | --- |
| crates.io | 已发 `0.13.0` | crates.io API，2026-09-09，`max_version=0.13.0` |
| GitHub Release | `v0.13.0` 四个 target：`x86_64/aarch64` × `linux-musl` / `apple-darwin`，各一份 `.tar.gz` + `.sha256` | `gh release view v0.13.0 --repo Luolc/limae`，2026-09-09；手册 `docs/knowledge/release.md` |
| PyPI | **没有** (`GET https://pypi.org/pypi/limae/json` → 404，2026-09-09) | 实测 |
| 消费方 pre-commit | `.pre-commit-hooks.yaml` 的 `id: limae` 是 `language: rust`，`entry: limae` | 仓内文件 |
| 本仓自己的 hook | `.pre-commit-config.yaml` 用 `language: system` + `cargo run`，只在本仓成立 | 仓内文件；README 也写了 |
| 配置 | 已实现两种同构载体：`limae.toml` 赢过 `[tool.limae]` | `spec/rules.md`「配置文件的两种载体」；`rust/config.rs` `find_config` |

Python 参考实现即将删除，根 `pyproject.toml` 现在还是 hatchling + `limae-python`。发 PyPI wheel 时这个文件要改成 maturin 的产品描述 —— 这正好跟「删 Python」叠在同一窗口，不要再为参考实现占这个名字。

---

## ① ruff 怎么做 (头号参照)

### 打包：maturin `bindings = "bin"`，不是 abi3，也不是 cibuildwheel

**读到原文：**

- ruff 仓根 `pyproject.toml` (GitHub `astral-sh/ruff` main，2026-09-09 核，当时版本字段 `0.16.6`)：

```toml
[build-system]
requires = ["maturin>=1.9.3,<2.0"]
build-backend = "maturin"

[tool.maturin]
bindings = "bin"
manifest-path = "crates/ruff/Cargo.toml"
module-name = "ruff"
python-source = "python"
strip = true
```

- maturin 文档 [Bindings](https://www.maturin.rs/bindings.html) (2026-09-09)：`bin` 把 Rust 可执行文件打进 wheel 的 scripts 目录，装完出现在 venv 的 `PATH` 上。自动检测条件是「只有 binary target、没有 pyo3 / cdylib」。
- PyPI `ruff 0.16.6` 的文件名 (2026-09-09 读 `https://pypi.org/pypi/ruff/json`) 全是 `ruff-0.16.6-py3-none-<platform>.whl`，共 17 个平台 wheel + 1 个 sdist。例如：
  - `py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64`
  - `py3-none-musllinux_1_2_x86_64`
  - `py3-none-macosx_11_0_arm64`
  - `py3-none-win_amd64`
- `py3` = 任意 Python 3；`none` = 不依赖 CPython ABI。装进 3.8 的 venv 和 3.13 的 venv 是同一份二进制。
- ruff 的发布 workflow (`.github/workflows/release.yml`，2026-09-09 原文) 由 cargo-dist 生成；PyPI 上传 job 是 `uv publish wheels/*`，wheel 由自定义 `build-binaries` 产出。**整份 workflow 没有 cibuildwheel。**

**关键词纠偏 (重要)：**

| 词 | 实际用在哪 | 跟 limae 的关系 |
| --- | --- | --- |
| **maturin `bindings = "bin"`** | 把 Rust CLI 当 Python 脚本分发 | **就是这条路** |
| **abi3 / `Py_LIMITED_API`** | pyo3 扩展模块的稳定 ABI，一份 wheel 跨多个 CPython 次版本 | **用不上**。limae 不是 Python 扩展，不链 `libpython` |
| **cibuildwheel** | 为每个 CPython 版本各编一份扩展模块 | **用不上**。`py3-none` 一份平台一份就够 |
| **平台 wheel** | `manylinux` / `musllinux` / `macosx` / `win` | **要做**。Linux 优先 musllinux (本仓 Release 已经是 musl 静态) |

**推断 (不是原文)：** ruff 的 `python-source = "python"` 是一层薄 Python 包装 (例如 `find_ruff_bin`)，不是 linter 本体。limae 第一版可以不要这层 —— maturin `bin` 会把 `limae` 可执行文件直接放进 `scripts/`。

### `[tool.ruff]` 怎么被 Rust 读到

不是 Python 去解析再喂给 Rust。Rust 自己读 toml。

**读到原文：**

- 文档 [Configuring Ruff](https://docs.astral.sh/ruff/configuration/) (2026-09-09)：认 `pyproject.toml` / `ruff.toml` / `.ruff.toml`，独立文件省略 `[tool.ruff]` 前缀。同目录优先级 `.ruff.toml` > `ruff.toml` > `pyproject.toml`。没有 `[tool.ruff]` 的 `pyproject.toml` 被跳过。最近的一份整份生效，不与父级逐键合并 (可用 `extend` 显式继承)。
- 源码 `crates/ruff_workspace/src/pyproject.rs` 的 `settings_toml` / `ruff_enabled` (GitHub 2026-09-09)：逐级祖先目录，先看独立文件，再看 `pyproject.toml` 里有没有 `tool.ruff`。

limae 已经是同一形状：`rust/config.rs` `find_config` 从 cwd 向上，`limae.toml` 赢过含 `[tool.limae]` 的 `pyproject.toml`，没有该表的 `pyproject.toml` 继续向上，碰到 `.git` 停。规范正本 `spec/rules.md`「发现顺序」。**配置发现不是分发问题，已经做完。**

### ruff 的其它安装渠道 (文档 [Installing Ruff](https://docs.astral.sh/ruff/installation/)，2026-09-09)

| 渠道 | 场景 |
| --- | --- |
| `uv add --dev ruff` / `pip install ruff` | **Python 项目的默认** |
| `uvx ruff@VERSION` | 一次性、不进依赖 |
| `uv tool install ruff` / `pipx` | 全局 CLI |
| `curl -LsSf https://astral.sh/ruff/install.sh \| sh` | 非 Python 机器；cargo-dist 安装器 |
| Homebrew / conda-forge / 发行版包 / Docker 镜像 | 后置，有量再做 |
| crates.io | 给 Rust 开发者，不是主路径 |

---

## ② pre-commit：消费方怎么接一个 Rust 二进制

本仓自己的 `language: system` + `cargo run` 只在本仓成立，理由也对 (dogfood 必须编当前源码)。**给别人的官方 hook 不能是这个。**

现在 `.pre-commit-hooks.yaml` 的 `id: limae` 是 `language: rust`。pre-commit 文档 [#rust](https://pre-commit.com/#rust) (2026-09-09)：`cargo install --bins` 装进缓存；没有 rustc 时 **pre-commit 会自己 bootstrap rust**；首次要联网编一次。

### 三条路对照

| 路 | 消费方要装什么 | 首次耗时 | 离线 | 谁在用 |
| --- | --- | --- | --- | --- |
| **A. `language: rust`** (现状) | 无 (framework 会装 rustc) | **分钟级** (拉 toolchain + 编译) | 差：要 crates.io 与 GitHub | 本仓当前默认；typos 的 `typos-src` 是备选 |
| **B. `language: python` + PyPI wheel** | Python (pre-commit 自己就有) | **秒级** (下一份 ~10 MB 的 `py3-none` wheel) | 好：wheel 可进缓存 / 私有 index | **ruff、typos 默认、taplo 镜像** |
| **C. 下载 GitHub Release 预编译二进制** | 无 / 视实现 | 秒级 | 好：tarball 可缓存 | 文档常这么写，实现上多半还是 B (PyPI 上那份就是预编译的) |

**读到原文，各家选了哪条：**

- **ruff**：单独仓 [`astral-sh/ruff-pre-commit`](https://github.com/astral-sh/ruff-pre-commit) (README 原文，2026-09-09：*"Distributed as a standalone repository to enable installing Ruff via prebuilt wheels from PyPI."*)。`.pre-commit-hooks.yaml` 是 `language: python`，`entry: ruff check --force-exclude`。`pyproject.toml` 只有 `dependencies = ["ruff==0.16.6"]`。pre-commit `pip install .` 装的是这个空包，真正的二进制来自 PyPI wheel，**不编译主仓**。
- **typos** ([`crate-ci/typos`](https://github.com/crate-ci/typos)，2026-09-09)：三个 id 写在同一仓。默认 `typos` = `language: python`，`setup.py` 里 `install_requires=['typos==1.50.1']` (占位包名 `pre_commit_placeholder_package`)；`typos-docker` = `language: docker`；`typos-src` = `language: rust`。文档写「从 GitHub releases 装预编译」，实现上默认 id 装的是 **PyPI 上的 `typos` wheel** (1.50.1 也是 `py3-none-*`，2026-09-09 读 pypi json)。文档那句和 setup.py 不完全一致 —— 以 setup.py 为准。
- **taplo**：官方仓太大，社区镜像 [`ComPWA/taplo-pre-commit`](https://github.com/ComPWA/taplo-pre-commit) (2026-09-09)：`language: python`，`dependencies = ["taplo==0.9.3"]`，跟 ruff 同一形状。
- **dprint**：没有官方 first-party hook；社区 [`trim21/dprint-pre-commit`](https://github.com/trim21/dprint-pre-commit) 是 Python 镜像。dprint 自己走 `dprint.json` + 安装器，不靠 pre-commit 当主入口。

**为什么默认不选 `language: rust`：** 第一次 commit 要等编译，CI (尤其 pre-commit.ci) 更痛；消费方仓库往往没有、也不该有 Rust 工具链。ruff 把 mirror 拆出去，就是为了让 `pip install` 命中 wheel 而不是对着 200 MB 的 monorepo 现场 `maturin build`。

**limae 的具体约束：** 根 `pyproject.toml` 一旦改成 maturin 产品包，消费方若 `repo: https://github.com/Luolc/limae` + `language: python`，pre-commit 会对 git clone 跑 `pip install .`，于是 **现场编译**，wheel 白做。所以官方 hook **不能**继续指着主仓的 maturin 项目。

### 推荐 (pre-commit)

**默认 hook 做成 ruff 那种 tiny mirror** (`Luolc/limae-pre-commit` 或同组织下一个几十行的仓)：

```yaml
# 消费方 .pre-commit-config.yaml
repos:
  - repo: https://github.com/Luolc/limae-pre-commit
    rev: v0.14.0   # 与 limae 版本锁在一起
    hooks:
      - id: limae
```

mirror 的 `pyproject.toml`：

```toml
[project]
name = "limae-pre-commit"
version = "0.0.0"
dependencies = ["limae==0.14.0"]
```

`.pre-commit-hooks.yaml`：`language: python`，`entry: limae`，`files: \.md$`。

主仓保留 `id: limae-src` (`language: rust`) 给没有匹配 wheel 的平台当逃生口。本仓 dogfood 继续 `language: system` + `cargo run`，不要跟消费方 hook 混。

**代价：** 多一个仓、发版时 mirror 的 `rev` / 依赖版本要跟 PyPI 同步 (ruff 用 `publish-mirror` job 自动推)。typos 那种「同仓 dummy setup.py」能省一个仓，但跟「根 pyproject 变成 maturin 产品」打架 —— 同一份 `pyproject.toml` 不能既是产品又是占位包。

**不要做：** 让消费方 `language: system` 自己装；不要把 `cargo run` 写进 README 当推荐。

---

## ③ CI：别人的 GitHub Actions 怎么接

### 推荐：不提供官方 action

ruff 有 [`astral-sh/ruff-action`](https://github.com/astral-sh/ruff-action)，文档 [Integrations](https://docs.astral.sh/ruff/integrations/) (2026-09-09) 同时给了「`pip install ruff` 三行」和 action 两条。action 的本质仍是装二进制再跑 `ruff check`。limae 没有 ruff 那个安装面，为三行 YAML 养一个 action 仓不划算。

**Python 项目 (用户自己的仓，也是最常见的消费方)：**

```yaml
- uses: actions/checkout@v5
- uses: astral-sh/setup-uv@v6          # 或 actions/setup-python
- run: uvx limae@0.14.0 --all          # 或: uv add --dev limae && uv run limae --all
```

`uvx` / `pip install` 命中平台 wheel，runner 不必有 Rust。版本钉在命令里或钉在 `pyproject.toml` 的 dev 依赖里。

**非 Python 项目：**

```yaml
- uses: actions/checkout@v5
- uses: taiki-e/install-action@v2
  with:
    tool: limae@0.14.0
- run: limae --all
```

[`taiki-e/install-action`](https://github.com/taiki-e/install-action) README (2026-09-09)：清单里没有的工具走 `cargo-binstall`，从 GitHub Release 下预编译。limae 已经有 Release tarball，**不改产物命名的话这条现在就能试** (未实测 `install-action` 对 limae 的解析；cargo-binstall 认不认当前 `limae-<target>.tar.gz` 这一步我没跑，标「查不到 / 未测」)。

更笨、但零依赖的写法：按 runner 的 arch 下对应 `limae-*-unknown-linux-musl.tar.gz`，校验 `.sha256`，跑二进制。本仓 Release 的 Linux 两个是全静态 musl (`docs/knowledge/release.md`)，GitHub-hosted `ubuntu-latest` 直接能跑。

**不要把 `curl | sh` 当 CI 主路径。** ruff 的安装器是给人的机器用的。CI 里 pin 版本 + 校验和，比安装器脚本稳。

**`cargo install limae` 不要当 CI 推荐：** 每次 job 编译，慢，还要 rustc。crates.io 留给「我就是要编源码」的人。

---

## ④ 配置文件：独立 `limae.toml`，不要把 `[tool.limae]` 当主入口

**判断：canonical 是 `limae.toml`。`[tool.limae]` 只是 Python 项目的适配器，文档默认示例用独立文件。**

理由：

1. **limae 不是 Python linter。** ruff 把 `pyproject.toml` 当主配置，是因为它 lint 的就是 Python，消费方几乎都有这份文件。markdownlint (`.markdownlint.json` / `.markdownlint-cli2.jsonc`)、dprint (`dprint.json`)、prettier (`.prettierrc`) 这些跨语言工具都用自己的文件。
2. **消费方会包括 Rust / JS / 纯文档仓。** 强迫 `machine-setup` 这类仓为三行开关去建一份 `pyproject.toml`，是把 Python 包装成门槛。ADR-0003 原文已经写了这个区分 (独立文件给非 Python，表给 Python)。
3. **独立文件赢过同层 `pyproject.toml`** 已经写进规范 (`spec/rules.md` 发现顺序第 1 条)。把 pyproject 当「主」会跟现有优先级打架。
4. **PEP 518 的 `[tool]` 是开放命名空间**，谁都能占；「能写进 pyproject」不等于「应该以它为默认」。typos 同时认 `typos.toml` / `_typos.toml` / `.typos.toml` / `[tool.typos]` / `Cargo.toml` 的 metadata —— 它是通用拼写检查，所以独立文件仍在。taplo 同时认 `.taplo.toml` 与 `[tool.taplo]`。跨生态工具的重心都在自己的文件上。

**保留 `[tool.limae]`，不要删。** 用户自己的 Python 项目三行开关不该再多一个文件；同构剪切粘贴已经实现。README 里 Python 示例可以继续给 `[tool.limae]`，但「配置」一节的第一份示例应该是 `limae.toml`。

**不要学 ruff 再加 `.limae.toml` 隐藏变体。** ADR-0003 否掉过：多一个名字就多一条「谁赢」。现在只有 `limae.toml`，保持。

---

## 给 limae 的可执行计划 (按顺序)

Python 参考实现删除与这件事重叠，不要拆成互不相干的两条发布线。

1. **把根 `pyproject.toml` 改成 maturin `bindings = "bin"` 的产品描述** (包名 `limae`，与 crates.io 同名)。PyPI 名目前空着 (404)。`requires-python` 放宽到一个宽下限 (ruff 是 `>=3.7`，typos 是 `>=3.8`) —— wheel 不链 Python，下限只是安装器过滤。
2. **CI 打平台 wheel，至少覆盖现有四个 GitHub Release target。** 用 `PyO3/maturin-action` (文档推荐 `maturin generate-ci github` 生成骨架；bin 示例见其 README 的 `messense/auditwheel-symbols`)。Linux 用 **musllinux** (本仓已经验收静态 musl)，macOS 两个 darwin。Windows 本仓 Release 现在没有，第一轮可以不做 —— 在 README 写明。**不要上 cibuildwheel，不要开 abi3。**
3. **`uv publish` 上 PyPI** (Trusted Publishing，跟 crates.io 那条 OIDC 同思路)。验收：在一台没有 rustc 的机器上 `uvx limae@<ver> --version` 能跑。
4. **开 tiny mirror `limae-pre-commit`**，发版时把 `dependencies = ["limae==<ver>"]` 和 tag 一起推。README 的「作为 pre-commit 远端 hook」那段改指 mirror；主仓 `language: rust` 改名为逃生口。
5. **README 给三份最小接入：**
   - Python 项目：`uv add --dev limae` + `[tool.limae]` 或 `limae.toml`
   - 任意项目的 pre-commit：mirror 那段 YAML
   - CI：`uvx limae@<ver> --all` (Python job) 或下 musl tarball (非 Python job)
6. **先不做：** 官方 GitHub Action、Homebrew、conda、`curl | sh` 安装器、Windows wheel、LSP。有人问再做。crates.io 继续发，但不再当消费方主路径。

**代价：** maturin 发布矩阵是新的 CI 负担；mirror 仓要跟版本。换来的是：用户在自己的 Python 项目里 `uv add --dev limae`，跟加 ruff 同一手感，机器上不用 Rust。

---

## 证据等级

- **实测 / 读到原文：** 仓内文件与 ADR；crates.io API；PyPI 404 与 ruff/typos 的 json 文件列表；`gh release view`；ruff / maturin / pre-commit / typos / taplo-pre-commit / ruff-action / install-action 的文档与源文件 (URL 与日期写在各节)。
- **推断：** ruff `python-source` 对 limae 第一版可省；cargo-binstall / install-action 对当前 tarball 文件名是否开箱即用 (未跑)。
- **查不到：** 没有找到 ruff 使用 cibuildwheel 的证据 (它的 release.yml 里没有这个词)。没有找到 dprint 官方 first-party pre-commit hook。
