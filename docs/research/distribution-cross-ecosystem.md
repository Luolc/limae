
# 第二轮：跨生态分发 (Node / TypeScript 与 AutoCorrect)

调研日期：2026-09-09。只读，未改本仓，也未改上一份报告。本地对照仓 `~/3p/huacnlee/autocorrect` 仍停在 `e1a75da` (2026-06-23)，与 `origin/main` 相同；**源码结构没变，各注册表上的版本已经分叉**，所以包名 / 版本 / 日期全部重新核，不沿用 2026-08-31 那份 internals 调研的清单。

## 一句话结论

**不要学 AutoCorrect 做「Rust 核心 + 每家一套 FFI 绑定」。** 那条路的真实产物是版本分叉，不是覆盖面。

limae 短期只有 CLI (format / polish / lint)、明确不做 LSP。跨 Python / Node / Rust / 纯文档仓的统一策略是：

**GitHub Release 的预编译二进制是唯一真相，外面套两层薄启动器 —— PyPI 的 maturin `bindings = "bin"` wheel，和 npm 的 biome / dprint 式平台包。** 不要 napi-rs，不要 pyo3 扩展模块，不要 Ruby / Java / Wasm SDK。

### 对上一轮推荐的修订

上一轮「先做 PyPI wheel + tiny pre-commit mirror」**作为 Python 路径仍然成立**，但不再是「下一件唯一的事」。Node / TypeScript 被点名为同等重要之后，**只发 PyPI 会把前端仓排除在外**；反过来只发 npm 会把 Python 仓排除在外。两条薄启动器共享同一份 Release 二进制，维护面几乎不翻倍。翻倍的是 FFI 绑定，不是 CLI wrapper。

---

## 1. AutoCorrect 现在实际发了什么 (2026-09-09 逐个核实)

源码仓仍是「一个 workspace 里塞了 node / py / rb / java / wasm / lsp / cli」(`Cargo.toml` members，读到原文)。**注册表上的版本对不上 git tag `v2.16.3`。**

| 渠道 | 包名 | 最新版 | 发布日期 | 还活着吗 | 核法 |
| --- | --- | --- | --- | --- | --- |
| GitHub Release | 二进制 `autocorrect-*` | **v2.16.3** | 2026-01-03 | 活。7 个 target (linux gnu/musl × amd64/arm64，darwin amd64/arm64，win amd64) | `gh release view --repo huacnlee/autocorrect` |
| RubyGems | `autocorrect-rb` | **2.16.3** | 2026-02-05 | 活，跟 tag 对齐 | `https://rubygems.org/api/v1/gems/autocorrect-rb.json` |
| npm (Wasm，浏览器) | `@huacnlee/autocorrect` | **2.16.2** | 2025-11-24 | 活，但比 tag 慢一个点；无 `bin`；上周下载 **76** | registry.npmjs.org |
| crates.io | `autocorrect` (库，不是 CLI) | **2.14.2** | 2025-07-25 | 活，**落后 tag 约 8 个月**。`autocorrect-cli` **不存在** | crates.io API |
| npm (Node 原生) | `autocorrect-node` | **2.14.0** | 2025-04-26 | 活但停更；上周下载 **3948**。源码 `package.json` 已写 `2.16.3`，没发上去 | registry.npmjs.org |
| PyPI | `autocorrect-py` | **2.14.0** | 2025-04-26 | 活但停更。**pyo3 abi3 扩展模块**，不是 CLI wheel | pypi.org/pypi/autocorrect-py/json |
| Maven Central | `io.github.huacnlee:autocorrect-java` | **2.10.0** | metadata `lastUpdated=20240522142757` (**2024-05-22**) | **基本停了** | repo1.maven.org maven-metadata.xml |
| Go | `github.com/longbridge/autocorrect` | 独立版本线 (proxy 上最高见 v1.2.0) | 仓 `pushed` 2025-08-29 | **不是同一份 Rust 核心的绑定**，是另一个仓 | proxy.golang.org；README 链到 `longbridgeapp/autocorrect` |
| PyPI `autocorrect` | 同名被占 | 2.6.1 | — | **别人的拼写纠正库**，无关 | pypi.org/pypi/autocorrect/json |

README 徽章仍同时挂着 Crate / NPM / PyPI / Gem / Maven (本地 README.md L9–13，`e1a75da`)。徽章给人「全家桶同步」的印象，注册表打脸。

**2026-08-31 那份 internals 调研没有过期的部分：** Node / Python / Ruby / Java / Wasm 目录都还在，FFI 接到同一份 `Ignorer`，测试仍是各绑一层冒烟 (`autocorrect-node/__test__/index.spec.mjs` 五条 `Hello你好.`)。**过期的是「都包了」隐含的「都在发」** —— Java 停了 16 个月，Node / PyPI / crates 停在 2.14.x，只有 Release 二进制和 RubyGems 跟到 2.16.3。

---

## 2. npm 那个到底是什么

**两份 npm 包，两套技术，不要混。**

### 2.1 `autocorrect-node` = napi-rs 原生 addon (给 Node 用)

**读到原文** (`autocorrect-node/package.json`、`index.js`、`src/lib.rs`、`cli.js`；npm 2.14.0)：

- `napi` 段指定包名 `autocorrect-node`，额外 triple 只有 `aarch64-apple-darwin` 和 `x86_64-unknown-linux-musl`。
- 装完导出 JS API：`format` / `formatFor` / `lintFor` / `loadConfig` / `Ignorer` / `run` (`index.js` L255–262，napi-rs 生成)。
- `bin.autocorrect = ./cli.js`，里面就是 `require('./index.js').run(process.argv)` —— CLI 是绑到同一份 `.node` 上的，不是独立二进制。
- 平台覆盖靠 `optionalDependencies` 五个分平台包：`win32-x64-msvc`、`darwin-x64`、`darwin-arm64`、`linux-x64-gnu`、`linux-x64-musl`。**没有 linux-arm64。** GitHub Release 倒是有 `linux-arm64` / `linux-musl-arm64`。绑定矩阵和 CLI 矩阵已经分叉。
- 安装时**不要本地编译**：预编译 `.node` 进分平台包。对不上平台就 `Failed to load native binding`。
- 分平台包自己的 `package.json` 带 `os` / `cpu` / `libc` 过滤器 (例：`npm/linux-x64-gnu/package.json`)。npm 只装匹配当前机器的那一个。

源码目录里的 `npm/*/package.json` 版本还写着 `2.8.1`，主包已经 `2.16.3` —— 连仓内占位文件都不同步。

### 2.2 `@huacnlee/autocorrect` = Wasm (给浏览器)

`wasm-pack` 打出来，无 `bin`，unpacked ~3.2 MB。README 定位是 Chrome 插件 / 编辑器内嵌。上周 76 次下载，几乎没人当 CLI 用。

### 2.3 前端仓实际怎么接

README「Use in NPM」(since 2.7.0)：

```bash
yarn add autocorrect-node
yarn autocorrect --lint .
```

`autocorrect-node/README.md` 给了 lint-staged 样例 (读到原文 L31–43)：

```json
{
  "lint-staged": {
    "*": ["autocorrect --lint"]
  }
}
```

这跟 husky / simple-git-hooks 的组合是 Node 生态的 pre-commit：hook 调 `lint-staged`，后者调 `node_modules/.bin/autocorrect`。

---

## 3. AutoCorrect 的 CI / 发版

**多条 workflow，不是一条。** `.github/workflows/` 里 (读到原文)：

| 文件 | 触发 | 干什么 |
| --- | --- | --- |
| `release.yml` | tag `v*` | 编 7 个 target 的 **CLI 二进制**，挂 GitHub Release，再推 Docker |
| `release-node.yml` | tag `v*` | napi-rs 五平台编 `.node`、在 18/20 上测、`npm publish` |
| `release-wasm.yml` | tag `v*` | nightly + wasm-pack + binaryen，发 `@huacnlee/autocorrect` |
| `release-py.yml` | **workflow_dispatch** | maturin-action 六组 job (macos/windows/linux/cross/musl)，twine 上传 |
| `release-crate.yml` | **workflow_dispatch** | `cargo publish`，且步骤带 `if: startsWith(github.ref, 'refs/tags/v')` |
| `release-rubygem.yml` | **workflow_dispatch** (+ 一条旧 branch) | oxidize-rb 交叉编 gem |
| `release-java.yml` | **workflow_dispatch** | 四平台编 `.so/.dll/.dylib` |
| `ci.yml` | PR / main | 除了 `cargo test`，还 `make test:node` / `test:python`……每家绑一层现场编译 |

版本同步靠 Makefile 的 `VERSION_FILES` + `rg`/`sed` (`Makefile` L4, `version:` 目标)。**没有机器人在 tag 时把所有渠道一起推。** 结果就是第 1 节那张表。

额外的机制坑 (读到原文，不是推断「他们懒」)：`release-crate.yml` 只在 `workflow_dispatch` 上跑，发布步骤却要求 `github.ref` 是 tag —— 从 main 手动点 Dispatch 时 ref 是 `refs/heads/main`，publish **直接跳过**。这能解释 crates.io 停在 2.14.2。

官方 GitHub Action `huacnlee/autocorrect-action` 是 **Docker** (`action.yml` `runs.using: docker`)，上次 push 2025-04-09，比最新 Release 旧。

---

## 4. 代价：多一个生态绑定实际多重

用 AutoCorrect 当对照臂，不靠感觉。

**一次性：** napi-rs / pyo3 / rb-sys / JNI 各要一份 `cdylib`、一份生成胶水、一份平台矩阵。`release-node.yml` 277 行，含 Debian/Alpine docker、strip、Node 18+20 双测。`release-py.yml` 六组 maturin job。Java 四平台编动态库。

**每次 PR：** `ci.yml` 的 `test_node` / `test_python` 要在 runner 上编对应 addon。不是「多跑一条 `cargo test`」。

**每次发版：** 渠道数 × (编、测、token、失败重跑)。AutoCorrect 把「自动」只给了 Release 二进制 / Node / Wasm；Python / crate / Ruby / Java 改手动。手动的那些就是先停更的那些。

**持续 issue 面：**

- 平台矩阵分叉 (Node 没 linux-arm64，Release 有)。
- 版本号分叉 (用户 `yarn add autocorrect-node` 拿到 2.14.0，`brew install` 可能是 2.16.3)。
- napi / pyo3 / Node / Python 版本升级各是独立的 breaking。
- 冒烟测试不共享黄金集 —— 2026-08-31 调研已写，2026-09-09 源码未改。绑定「绿」证明不了 Markdown 行为。

**下载量对照 (npm last-week，2026-08-31–09-06，api.npmjs.org)：** `@biomejs/biome` 1318 万，`oxlint` 1904 万，`dprint` 20 万，`autocorrect-node` 3948，`@huacnlee/autocorrect` 76。AutoCorrect 的 Node FFI 没换来生态级用量，却付了全套矩阵的税。

**给 limae 的数字直觉 (推断，不是实测分钟数)：** 只发 CLI 二进制 + 两个薄启动器，发版是「一组 compile + 两段打包上传」。每加一个真正的 FFI 生态，大约再加一条 5+ 平台的编译矩阵和一套冒烟。AutoCorrect 加到 5 家绑定之后，有 3 家已经落后两个小版本以上。limae 现在没有专职发版团队，复制这条路会先死在同步上。

---

## 5. Node / TypeScript：Rust CLI 怎么进前端仓

主流不是 FFI，是 **「npm 包里塞预编译 CLI」**。装完 `npx <tool>` 或 `package.json` 的 `scripts` 就能跑，**安装不编译**。

### 样本 (2026-09-09 读 npm registry)

**biome (`@biomejs/biome` 2.5.12，2026-09-03)：** 主包只有一个 `bin/biome` 的 JS 启动器，`optionalDependencies` 8 个 `@biomejs/cli-<platform>`，里面是真正的二进制。文档第一句是 `npm i -D -E @biomejs/biome`，然后 `npx @biomejs/biome check`。JS API 另包 `@biomejs/js-api`，跟 CLI 拆开。

**dprint (`dprint` 0.57.4，2026-09-05)：** 同样 `bin` + 15 个 `@dprint/<os>-<arch>-<libc>` optionalDeps，另有 `postinstall: node ./install.cjs` 做下载兜底。配置是自己的 `dprint.json`，不进 `package.json` 业务字段。

**oxlint (`oxlint` 1.82.0，2026-09-07)：** 看起来像 biome，实际是 **napi-rs** (`napi.packageName = @oxlint/binding`，19 个 binding 包)。它有理由：oxlint 要当 ESLint 的替换，编辑器和 JS 工具链要 in-process。这是「JS linter 写在 Rust 里」，不是「给 Markdown 文件跑一个 CLI」。

**autocorrect-node：** 见上，napi + 顺手挂了 `bin`。前端仓用的其实是那个 `bin`，不是 `format()`。

### 前端仓的三种接法

1. **`devDependencies` + `npx` / `package.json` scripts** (biome / dprint 的默认)。版本锁在 lockfile 里。
2. **husky + lint-staged** 在 commit 时只跑暂存的 `*.md`。AutoCorrect 自己的 README 就是这个形状。
3. **pre-commit 框架** 在 Node 仓里也能跑 (ruff-pre-commit 那套)，但前端仓常常没有 Python；不要把 Python 预提交框架当成 Node 的主入口。

`npx limae` 只有在 npm 上有包时才成立。没有 npm 包，前端开发者不会去 `curl` GitHub Release。

---

## 6. 绑定 vs 只发 CLI

**判断：limae 只发 CLI，不要 FFI。**

AutoCorrect 做 FFI 是因为它的产品故事包括「在 Ruby China / V2EX 存内容时 in-process 格式化」(README「典型应用场景」)。那是库，不是 linter CLI。

limae 短期三个功能全是命令行；`polish` 还要再 exec 别人的 CLI。没有「在 Next.js 请求路径里 `import { format } from 'limae'`」这种需求。不做 LSP，编辑器插件要接也可以 spawn CLI (markdownlint-cli2、dprint 都这样)。

FFI 的唯一好处是少一次进程启动。对「扫一批 `.md`」这个负载，启动成本可以忽略。它换来的是 napi 矩阵、abi3 wheel、Node 版本政策，和「绑定测过了但 CLI 行为没测」的假绿。

所以：Node 要的是 `npx limae --all`，不是 `import { format }`。

---

## 7. 跨生态的统一策略

**一条能覆盖大部分的路：Release 二进制为源，薄启动器按生态各做一份，不做原生绑定。**

```
          同一组 musl / darwin (及以后 windows) 二进制
                         │
          ┌──────────────┼──────────────┐
          ▼              ▼              ▼
   GitHub Release    PyPI wheel      npm 平台包
   (cargo install    (maturin bin,   (optionalDependencies,
    / 手解 tar)       py3-none-*)     biome 形状)
          │              │              ▼
          │              ▼         npx limae / lint-staged
          │         uv add --dev
          │         pre-commit
          │         (language: python)
          ▼
     纯文档仓 / Rust 仓直接下 tar 或 cargo install
```

| 消费方 | 怎么装 | 怎么跑 hook |
| --- | --- | --- |
| Python 项目 | `uv add --dev limae` | pre-commit tiny mirror (上一轮) |
| Node / TS 项目 | `npm i -D limae` | husky + lint-staged，或 `npx limae` |
| Rust 仓 | crates.io 的 `limae` CLI 已在；或 Release tar | pre-commit (mirror) 或 `cargo bin` |
| 纯文档仓 | 哪个包装器方便用哪个；CI 解 musl tar | 同上 |

三家薄启动器 (Release / PyPI / npm) 编的是**同一份 binary**，不是三份语义。pre-commit 继续走 Python wheel，因为 pre-commit 自己是 Python —— 这不是「偏袒 Python」，是 hook 框架的运行时。

**不要：** 每家 FFI；不要官方 Action (AutoCorrect 那个 Docker action 自己都停更)；不要 Wasm，除非真做浏览器插件。

---

## 8. 回头看上一轮，改什么、不改什么

上一轮见 [`distribution-python-ecosystem.md`](distribution-python-ecosystem.md)。

| 上一轮 | 这一轮 |
| --- | --- |
| 下一件事 = PyPI maturin bin wheel | **改成：Release 二进制为源，PyPI 与 npm 两个薄启动器并列。** 若必须排期，可以先 PyPI (用户自己的 Python 仓先吃到)，但文档和发版设计一开始就要给 npm 留位，别把根 `pyproject.toml` 做成「Python 专属产品」以至于 npm 要另起一套编译。 |
| tiny `limae-pre-commit` mirror | **保留。** Node 仓若用 pre-commit，照样吃这份；若用 husky，走 npm `bin`。mirror 不是 Node 的主入口，也不是障碍。 |
| 不做官方 Action | **保留。** AutoCorrect 做了，还是 Docker，而且落后 Release。 |
| 配置 canonical = `limae.toml` | **保留，更硬。** 前端仓没有 `pyproject.toml`；biome / dprint / AutoCorrect 全都用自己的文件 (`.autocorrectrc` / `biome.json` / `dprint.json`)。`[tool.limae]` 继续当 Python 适配器。 |
| 不做 LSP | **未改，本轮再次确认。** |

**硬撑「只做 PyPI 就覆盖别人的仓」不成立。** 硬撑「要服务 Node 就得 napi-rs」也不成立 —— biome 和 dprint 用 optionalDeps 装 CLI，才是 Node 生态对 Rust 工具的标准答案。

建议的落地顺序 (执行时，不是本调研的范围)：

1. 保持 / 收紧 GitHub Release 矩阵 (已有四个 musl/darwin；以后要 npm 再补 windows)。
2. PyPI `bindings = "bin"` (上一轮，服务 Python + pre-commit)。
3. npm：主包 `limae` + `@limae/cli-<platform>` optionalDeps，JS 只做 spawn，**从 Release 产物打包，不要再编一次**。
4. README 给两段最小接入：`uv add --dev limae` 和 `npm i -D limae`，hook 分别指向 pre-commit mirror 与 lint-staged。

**代价：** npm 多 8 个左右的平台包和一段发布脚本，比 napi 矩阵小一个数量级。**不要**为了「完整」去发 Wasm / gem / jar。AutoCorrect 用五年证明了那些渠道会先死。

---

## 证据等级

- **读到原文 / 实测：** 本地仓 `e1a75da` 的 README、Makefile、Cargo workspace、各 `release-*.yml`、`autocorrect-node/{package.json,index.js,src/lib.rs,cli.js,__test__,npm/*/package.json}`、`autocorrect-py/Cargo.toml`；2026-09-09 对 npm / PyPI / crates.io / RubyGems / Maven / GitHub Release / npm downloads API / `gh api` 的现场查询。
- **推断：** 「Release 产物直接打进 npm 平台包、不必重编」在 biome / dprint 上成立，limae 没做所以没测；AutoCorrect crate workflow 的 tag-guard 导致停更，是读 YAML 的机制推论，没有翻他们的 Actions 日志。
- **查不到：** `autocorrect-py` 的 pypistats 本次请求无有效 JSON；Go 仓与 Rust 核心是否共享代码未下进去核对 (版本线已经独立，足够判定「不是同一套绑定」)。
