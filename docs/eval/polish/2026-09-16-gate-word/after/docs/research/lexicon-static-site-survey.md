# 词典静态页的技术选型调研

- 日期：2026-09-06。调研者：`limae-research-site` (只读，未改任何仓内文件)。
- 修订：2026-09-06，按 orchestra 提出的六条事实与论证修正 (见文末「修订记录」)。
- 结论一句话：**当前这一页不需要 React，也不建议引入静态站点生成器 (static site generator, SSG)**；现有「程序读 TOML 生成一份自包含 HTML」的路线在这个规模下已经够用，缺的不是框架，而是几处 HTML 文档层和流程层的补齐。
- 外部资料均取自各方案的官方当前文档，取用日期为 2026-09-06，链接见文末。

## 一、现状核实 (本机实测，2026-09-06)

**本节读数取自 2026-09-06，此后仓库状态已变，本文不再追更**；下面第 2 点的具体结论已被推翻，见后文按语。

派活方描述的现状经独立核对属实，另补充如下读数：

1. **生成链路**：`spec/lexicon/zh.toml` (115 行，`preface` / `standard` / `threshold` 加 9 个 `[[entry]]`) → `tools/render_lexicon.py` (279 行，只用标准库 `tomllib` / `html` / `unicodedata` 和字符串拼接) → `site/index.html`。
2. **产物当前可复现**：在临时沙箱中以只读方式软链 `spec/` 后重跑生成脚本，产物与仓内 `site/index.html` **字节一致** (`cmp` 通过)。另在 `tests/` 与 `.pre-commit-config.yaml` 中**未找到**检查这件事的自动检查 (`.pre-commit-config.yaml` 的 limae 钩子只匹配 `\.md$`；查找是静态的，并不穷尽)。**这两条只支持「今天一致，且未发现自动守卫」**，不支持对维护者是否有意同步作任何推断 —— 那需要另外的证据。**已被推翻 (2026-09-08)**：`tools/check_lexicon_render.sh` 现已作为 CI 的 required check 之一存在 (`AGENTS.md` 的质量门列表)，正是本文第六节第 2 条建议的「重跑生成器、与 `site/index.html` 字节一致」检查；它不在 `tests/` 或 `.pre-commit-config.yaml` 中，所以本条按字面查找的两个位置仍然没有结果，但「未找到检查这件事的自动检查」这一结论已不成立。
3. **产物形态**：21 600 字节。对产物文本做静态扫描，`<script>` / `<img>` / `<link>` / `src=` / `url(` / `@import` / `http(s)://` 的计数**均为 0** —— 即未见任何外链资源声明；未在浏览器中实测实际网络请求。同时，`<!DOCTYPE html>` 为 0 处、`<html lang>` 为 0 处、`<meta charset>` 为 0 处、`<meta viewport>` 为 0 处。
4. **托管**：仓内只有 `.github/workflows/ci.yml`，没有 Pages 部署工作流；`gh api repos/Luolc/limae/pages` 返回 404 —— 这只说明**未取得 Pages 配置**，不能据此断言站点未启用。
5. **Rust 侧已有的料**：根 `Cargo.toml` 已依赖 `toml` (带 `parse` + `serde`)，`Cargo.lock` 中已有 `serde_derive` / `syn` / `quote` / `proc-macro2`。这只说明**部分依赖已共享**，不足以推出任一模板方案的边际编译成本 —— 编译时间与产物体积本轮**未测**，因此不作成本结论。

## 二、把问题拆成三层

用户的三个疑问其实分属三层，混在一起谈容易变成「要不要上 React」这种错误的二选一：

- **浏览器运行什么**：当前是一份零脚本的 HTML+内联 CSS。这一层与用什么工具生成无关。React 不必然意味着客户端 SPA —— `react-dom/server` 的 `renderToStaticMarkup` 输出不带 hydration 的静态 HTML，Next 的 `output: 'export'` 也是每个路由一份 HTML。**不推荐 React 的依据是需求与维护成本**：这一页没有需要额外客户端状态或组件运行栈的需求，引入它会多出一整套 Node 工具链与升级面。
- **构建时用模板还是拼字符串**：这才是用户真正所问的「工程上更严谨」。当前的问题不是没有框架，而是 HTML 结构和生成代码揉在一起 —— 改一处版式就要改一行 `f"..."`，也没有任何检查保证每处转义都被调用。
- **托管与构建在哪跑**：GitHub Pages 的分支来源只允许**仓根 `/` 或 `/docs`** 两个目录，`site/` 两者都不是；因此要么走 Actions 工作流部署，要么调整目录。官方对「非 Jekyll 构建」的建议本身就是编写一个 Actions 工作流。这一层与前两层完全解耦。

## 三、比较表 (按本仓实际规模：单页、9 条词条、零交互)

| 方案 | 浏览器运行 | 新增依赖 / 工具链 | 与 Rust 迁移的关系 | 复现现有产物 | 主要代价 |
| --- | --- | --- | --- | --- | --- |
| ① 现状 + 模板分离 (Askama / MiniJinja) | 零脚本，一份 HTML | Rust crate 一个，无外部运行时 | 直接落进 E 的收尾补项 | 可做到，需模板空白控制 | 空白控制要与今天的单行输出逐字对齐 |
| ② Zola (Rust SSG) | 零脚本 | 一个预编译二进制 (本机无，需另装) | 另一条构建链，与 Rust 库并行 | 未验证；可自写模板，未证明不能复现 | 为一页引入 `content/` / `templates/` / 配置的整套约定 |
| ② Hugo | 零脚本 | 一个预编译二进制，**不需要 Go 编译器** | 无关 | 未验证；同上 | 同上；用户已有经验是真实的成本优势 |
| ③ Astro | 默认零脚本，可选局部交互 (islands) | Node + npm 依赖树 | 无关 | 未验证 | 为一页引入一整套 Node 工具链与升级面 |
| ④ React / Vite 或 Next `output: 'export'` | 静态导出后默认带 hydration 运行时 (`renderToStaticMarkup` 则不带) | Node + 框架 + 构建器 | 无关 | 未验证 | 当前需求不需要组件运行栈 |
| Eleventy | 零脚本 | Node 18+ | 无关 | 未验证 | 比 Astro 轻，但仍是「为一页引入 Node 构建」 |

Hugo 官方 Pages 工作流中的 Dart Sass 是**该工作流为 Sass 转译准备的可选项**，不是 Hugo 的普遍硬依赖。

**关于「字节一致」的定位** (这一条上一版写得过宽，现修正)：字节一致是 tracker 为 Rust 迁移 E 设定的**既定验收** —— 「同源生成产物字节一致」，用于证明换实现没有换行为；它**不是**「网页永远不能改」的产品要求。补 head 同样会改变字节。因此，三类改动应分开处理：

- **重构等价**：内容与视觉不变，只换生成实现 → 按字节一致验收。
- **经批准的改进**：补 doctype / charset / 可访问名等 → 重新生成并确立**新的 baseline**，字节一致的验收对象随之更新。
- **换生成工具**：产物形态由工具决定 → 属于新方案，交 orchestra 与用户裁决。

因此，本轮不推荐 SSG 的理由是**构建依赖与当前单页需求**，不是「SSG 必然与字节一致冲突」 —— Hugo / Zola 都可以自写模板，本轮**没有验证**它们能否复现现有产物，也不宣称它们不能。

## 四、推荐

**首选：保持「程序生成一份自包含 HTML」的形态，把 HTML 从代码中抽成模板文件，用 [Askama](https://docs.rs/askama/latest/askama/)。**

理由：

- **模板错误在编译期暴露**。Askama「implements a type-safe compiler for Jinja-like templates」，模板经 derive 宏在编译期展开成 Rust 代码，字段写错、类型不对都会导致编译失败。**这只是模板与数据的一致性检查，不是 HTML 合法性检查，也不是可访问性检查** —— 后两项要另配检查，换任何模板引擎都不会自动获得。
- **默认转义**。Askama 按扩展名自动进行 HTML 转义，`|safe` 才放行 —— 与今天手写 `html.escape` 相比，转义从「每处记得调用」变成「默认发生，例外要显式写」。今天漏掉一次 `html.escape`，没有任何检查会报错。
- **不需要运行时发现文件**。模板随二进制编译进去，与 `rust/resources.rs` 用 `include_str!` 嵌入词表的既有做法目标一致 (机制不同：Askama 通过 derive 宏读取模板文件，不是 `include_str!` 本身)。

**次选：[MiniJinja](https://docs.rs/minijinja/latest/minijinja/)** —— 官方文档写的是「implemented on top of serde」且依赖精简，**不是「无依赖」** (上一版这句不实，已改)。它同样可以把模板嵌进二进制：`add_template(name, include_str!("..."))`，因此「运行时解析渲染」不等于「必须从磁盘加载模板」；**「改模板不必重编译」只适用于把模板放在二进制之外的方案**。代价是模板与数据不匹配要到渲染时才报。**切换触发条件**：确实需要在不重编译的情况下改模板 (例如把版式交给非 Rust 使用者调整)，或模板要在运行时才能确定。

两者的编译时间与体积差异本轮**未测**，不作为推荐依据；推荐 Askama 的依据只有上面第一、二条。

**Python 侧**：迁移期保留 `tools/render_lexicon.py` 作参考，不建议为它引入 Jinja2 —— 那会在一个即将 deprecated 的入口上新增依赖。

## 五、切换触发条件 (满足其一才重开此题)

1. **多页文档**：站点扩展为「首页 + 规则文档 + 多语言」这类结构，需要导航、分页、RSS。此时 **Hugo 与 Zola 并列备选**：Hugo 的优势是用户已有经验 (真实的成本优势)，Zola 的优势是单个预编译二进制且 `load_data()` 直接读 TOML、不强制套主题 (「Unlike some SSGs, Zola makes no assumptions regarding the structure of your site」)。**不因为库是 Rust 就优先 Zola** —— 网站与库的实现语言没有必须一致的理由。
2. **少量交互** (按词条筛选、搜索、拼音跳转)：先写几十行原生 JS，不引入任何框架。这一步不应触发选型变更。
3. **交互发展成应用**：出现跨组件共享状态、路由、局部更新，再谈 **Astro islands** (静态为主，只给需要的部分发 JS)。
4. **内容改由非工程手段维护**：词典正文不再由 `spec/lexicon/zh.toml` 唯一决定 —— 这与本仓「同一批内容只写一处」的约定冲突，要先改约定，不是先换工具。

## 六、真正该修的工程点 (与框架无关，今天就能做)

1. **补完整 HTML 文档**：`<!DOCTYPE html>`、`<html lang="zh-Hans">`、`<meta charset="utf-8">`、`<meta name="viewport" content="width=device-width, initial-scale=1">`。当前四项全缺。**编码识别在缺 charset 时取决于宿主响应头与浏览器嗅探** —— 本轮未实测 `file://` 或任何宿主下的实际表现，所以理由不写成「必乱码」，而是：显式声明才不依赖宿主。这一项与选型完全无关，建议最先做。
2. **产物一致性检查**：在 CI 中加一条「重跑生成器，产物与仓内 `site/index.html` 字节一致」的检查。这条检查天然有分辨力：改了 `spec/lexicon/zh.toml` 却忘记重跑时，它必然报错。改动产物的 PR 一并更新 baseline (见第三节的三类改动)。
3. **语义与可访问性**：整页缺 `<main>`；词条 `<section>` 没有可访问名 (无 `<h2>` 或 `aria-label`)，屏幕阅读器和「按标题跳转」都无法使用。词条标题可直接由 `term` 生成，成本极低。**这类检查模板引擎不提供**，要么人工核对，要么另配工具。
4. **字体回退**：回退链已含 Windows 的 `KaiTi` / `SimSun` 与 Linux 的 `AR PL UKai CN` / `Noto Serif CJK SC`，所以上一版「Linux/Windows 访客必落 `serif`」是错的，已删 —— 各平台实际落到哪一个取决于该机安装了什么字体，本轮**未实测**。仍**不建议**为此引入 web font (体积与外部请求的代价大于收益)。
5. **转义边界**：`_inline()` 按成对反引号切分，**奇数个反引号会把尾段包成 `<code>…</code>`** (本机实测：输入一段带**单个**反引号的文本，输出会把反引号之后的整段包进 `<code>` 标签；上一版写成「被当普通文本」，方向写反了)。这不是安全问题 (`html.escape` 先跑)，但会静默造成版式错误，迁 Rust 时值得配一个合成用例。
6. **托管**：编写一个 Pages 部署工作流 (`actions/configure-pages` → `actions/upload-pages-artifact` → `actions/deploy-pages`，权限 `contents: read` / `pages: write` / `id-token: write`，环境 `github-pages`)，仓库设置中把 Source 改成 GitHub Actions。这样 `site/` 可以留在原处，不必迁就分支来源的 `/` 或 `/docs`。
7. **本地预览**：本机无 hugo / zola，有 node 与 python3。当前产物是单文件，`python3 -m http.server` 或直接打开即可查看 —— 这是现状的一项真实优势，SSG 方案会把它换成「先装工具链」。

## 七、建议的最小目录结构 (若采纳模板分离)

```
spec/lexicon/zh.toml             # 内容以此为准，不变
tools/site/index.html.tmpl       # 新增：页面骨架 (模板源)
tools/site/style.css             # 可选：把 STYLE 抽出来，模板 include
rust/examples/render_lexicon.rs  # 迁移后的生成器 (dev-only target)
site/index.html                  # 生成产物，继续提交进仓
tools/render_lexicon.py          # 迁移期保留作参考
```

要点：**`site/` 按仓规只放生成产物**，模板源不放进这个待部署目录，而是放在 `tools/site/` 或独立的模板源目录。生成器是开发工具，建议做成 `rust/examples/` 这类 **dev-only target**，不预设第二个可 `cargo install` 的生产 bin —— ADR 目前只有一个生产 CLI。生成器读 `spec/` 与模板，写 `site/index.html`，路径合同不变。**具体目录名与 Cargo `include` 在实施 PR 中核定**，本轮未做布局实验。**注意**：当前产物几乎是一整行 (仅 `<style>` 内含换行；`wc -l` 数 143 个换行符，而文件末字节是 `>`、无结尾换行，因此实际 144 行)。Jinja 系模板要用空白控制 (`{%-` / `-%}`) 才能逐字复现，并且必须保证渲染结果末尾不额外补一个换行 —— 这两处是采用模板时最容易出错的地方，建议写进任务的验收步骤。

## 八、来源 (全部取用于 2026-09-06)

- Askama：<https://docs.rs/askama/latest/askama/>、<https://github.com/askama-rs/askama>
- MiniJinja：<https://docs.rs/minijinja/latest/minijinja/>、<https://docs.rs/minijinja/latest/minijinja/fn.default_auto_escape_callback.html>、<https://github.com/mitsuhiko/minijinja>
- Zola：<https://www.getzola.org/documentation/getting-started/overview/>、<https://www.getzola.org/documentation/templates/overview/>
- Hugo：<https://gohugo.io/content-management/data-sources/>、<https://gohugo.io/host-and-deploy/host-on-github-pages/>
- Astro：<https://docs.astro.build/en/getting-started/>、<https://docs.astro.build/en/concepts/why-astro/>
- Next.js 静态导出：<https://nextjs.org/docs/app/guides/static-exports>
- React 静态渲染：<https://react.dev/reference/react-dom/server/renderToStaticMarkup>
- Eleventy：<https://www.11ty.dev/docs/>
- GitHub Pages：<https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site>、<https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages>

## 九、证据等级

- 第一节与第六节第 5 条为本机实测 (沙箱重跑、`cmp`、`grep` 计数、直接调用 `_inline()`、`command -v`)，命令均只读，未改动仓内任何文件。
- 各方案特性引自上列官方文档，已标注取用日期。`renderToStaticMarkup` 一条由 orchestra 提供链接，本轮未另行取页核对。
- **未做的事**：未测任何方案的编译时间与产物体积，未验证 Hugo / Zola / Astro 能否复现现有产物，未实测缺 charset 时各宿主的编码表现，未实测跨平台字体落点，未在浏览器中实测网络请求。凡未测的，正文都不写成结论。

## 十、修订记录 (2026-09-06，按 orchestra 六条)

1. 删去 MiniJinja「默认无依赖」 —— 官方写的是 implemented on top of serde、依赖精简；补上它同样可用 `include_str!` 嵌入模板，「改模板不重编译」只适用于外置模板方案。
2. 不再把 Askama 的 derive 宏说成 `include_str!`；删去「边际成本小」这个未测的结论；写明编译期检查不覆盖 HTML 合法性与可访问性。
3. 删去「SSG 产物由工具决定 → 必与字节一致冲突」；改为按构建依赖与当前需求推荐，并区分重构等价 / 经批准的改进 (新 baseline) / 换工具三类改动；补充 React `renderToStaticMarkup` 可输出无 hydration HTML。
4. 修正 `_inline()` 奇数反引号的方向 (尾段被包成 `<code>`，实测)；删去「Windows 必落 serif」(回退链已含 KaiTi / SimSun)；缺 charset 不再推「必乱码」，改为编码识别依赖宿主、建议显式声明。
5. 删去「今天一致是巧合」这个无证据的归因，改为「当前 `cmp` 一致、未发现自动守卫」；「零外部请求」改为「静态扫描未见外链资源声明」，并补做 `url(` / `@import` / `http(s)://` / `src=` 的计数。
6. Hugo 改为预编译二进制，不需要 Go 编译器，Dart Sass 标注为该工作流的可选步骤；Zola 不再因 Rust 实现而优先，与 Hugo 并列，并写明用户已有 Hugo 经验是真实成本优势；删去 React「唯一理由」的绝对表述。
7. 第七节目录示例按仓规校正：`site/` 只放生成产物，模板源移到 `tools/site/`；生成器改为 `rust/examples/` 这类 dev-only target，不预设第二个生产 bin；具体目录与 Cargo `include` 留给实施 PR 核定。
