# 发布手册

`limae` 的发布分两条独立的线：**GitHub Release 的二进制产物**由 tag 触发的 workflow 自动产出，**crates.io 的 `cargo publish`** 在维护者的开发机上执行，走 API token 路径而不是 Trusted Publishing。本文覆盖两条线各自的步骤、判据与它们之间的先后关系。

本文的步骤默认可以由 agent 执行；标了 👤 的那几步必须由人做，因为它们在浏览器里，不是命令。

工具链版本与 target 的唯一配置来源是仓根 `rust-toolchain.toml`；本地分发预演 (源码包、独立 Linux binary) 见 [Rust 分发预演](rust-binary-distribution.md)，本文不重复。token 的存放位置、`op run` 注入方式与权限配置以 [维护者的开发机发布手册 (私有仓)](https://github.com/Luolc/machine-setup/blob/main/docs/knowledge/crates-io-publishing.md) 为准，本文只写「什么时候做哪一步」。

## 凭证红线

**任何 agent 不读、不接触 crates.io token 的值。** 保证这一点的是 `op run` 的机制，不是「把这一步推给人」 —— token 由 `op run` 按次注入子进程的环境，谁敲的这条命令与值会不会被读到无关。所以 `op run -- cargo publish` 这一步**可以由 agent 执行**。

**不做 `cargo login`**：它会把 token 写进 `~/.cargo/credentials.toml`，从此长期落盘。也**永远不用 `--token <值>`** 这种把值放进命令行的写法 —— 命令行会进 shell 历史、进进程表、进任何一份日志。输出只回显退出码与状态，值不进任何输出。

token 若曾以任何形式出现在会话记录、命令输出或仓内文本里，按全局守则先轮换、再处理泄漏。验证凭证只看存在与长度，值本身不进任何命令输出。

维护者机器上 `~/.cargo/credentials.toml` 在 2026-09-08 实测不存在 (只查了存在性，没有读取内容)。**这是刻意的，不是还没配**：不跑 `cargo login`，token 就不落盘，每次发布由 `op run` 现注入。

## 版本策略

单一版本序列：`Cargo.toml` 与 `pyproject.toml` 的 `version` 保持同一个值，由 Rust 实现往前推。

Python 参考实现自 0.13.0 起**冻结、不再发布** (用户 2026-09-08 裁决)：它的入口 `limae-python` 与测试继续保留、继续跑，是差分臂的对照一侧，但不再打 wheel、不再上传任何包索引。`pyproject.toml` 里的 `version` 因此只是跟随，不表示有对应的 Python 发布物。

## `cargo publish --dry-run` 不能当验收

`cargo publish --dry-run` 对「crates.io 会不会接受这份 manifest」**零分辨力**：它在本地打包并编译，根本走不到服务端校验，manifest 会被接受和会被拒绝这两种情况下**都退出 0**。2026-09-07 的实测读数是：在 scratch 副本里跑 `cargo publish --dry-run --no-verify --allow-dirty --locked`，manifest 当时缺 crates.io 必填的 `description`，命令仍退出 0，只打印一行 `warning: manifest has no description…`。

一个对两种情况给出相同输出的观察，不能用来在两者之间做选择 —— 更糟的是它看起来像一次检查，于是关掉了下游复核。所以：

- **可以拿来当证据的**：字段确实在 `Cargo.toml` 里 (直接读文件)，以及 `tools/check_rust_package.sh package` 真的打得出包、能从包里装出可运行的 binary，并且把包内每个显式 target 的**运行时输入**也跑过一遍。
- **不可以拿来当证据的**：`--dry-run` 退出 0。

manifest 到底合不合格，只有真正 `cargo publish` 那一刻由 crates.io 给出答复。`--dry-run` 仍然值得跑，它是一次**本地打包检查** (打得出包吗、编译得过吗)；只是**发布成不成功以真 `cargo publish` 的退出码为准**，不以它为准。

同一个形状咬过一次，记在这里：`Cargo.toml` 的 `include` 曾经漏掉 `spec/lexicon/zh.toml`，而随包分发的 `render-lexicon` example 在运行时要读它。包打得出来、example 编译得过、`cargo test` 全绿 —— 因为编译一个 example 不需要它的运行时输入。**「编译过了」对「装出来能不能用」零分辨力**；只有在解包目录里真的跑一次那个 example 才分得开 (PR #141 审查方发现，同 PR 修复：入 `include`，并让打包门真跑它、外加一条抽掉词典的对照臂)。

## 一、首发 (crate 尚不存在时，只做一次)

crates.io 的 Trusted Publishing 无法在 crate 存在之前配置，官方原文：

> To get started with Trusted Publishing, you'll need to publish your first release manually. After that, you can set up trusted publishing for future releases.

—— [crates.io: development update](https://blog.rust-lang.org/2025/07/11/crates-io-development-update-2025-07/) (2025-07-11)。[crates.io 文档](https://crates.io/docs/trusted-publishing) 的 Prerequisites 一节把同一件事写成：

> Your crate must already be published to crates.io (initial publish requires an API token)

这里的 manually 是**相对 Trusted Publishing 而言**的 —— 指首发得用 API token 而不是 OIDC，**不是**「必须由人手动敲」。因此首发只能带 API token 跑，用不了 Trusted Publishing；执行者可以是 agent。

1. **在维护者的开发机上**，仓库根目录，确认工作区干净、且就是要发布的那个 commit：

   ```sh
   git status --porcelain && git rev-parse HEAD
   ```

   完成判据：第一条命令**无任何输出**，第二条打出的 SHA 与你要发布的 commit 相同。

2. 同一台机器、同一个目录，按 CI 的门顺序裸跑全部质量门 (命令清单见仓根 `AGENTS.md`)，**不接管道**。

   完成判据：每一道门都退出 0。输出太长就整条重定向到文件、再单独看退出码 —— `| tail` / `| grep` 会把退出码换成管道末端那个程序的，通过与失败给出同一个 0。

3. 同一台机器，确认打包门能真的产出包：

   ```sh
   tools/check_rust_package.sh package
   ```

   完成判据：退出 0。

4. 同一台机器，按 [私有仓手册](https://github.com/Luolc/machine-setup/blob/main/docs/knowledge/crates-io-publishing.md) §4 用 `op run` 注入 token 并执行 `cargo publish` —— 命令以那份手册为准，这里不重复。可以先跑一次 `cargo publish --dry-run` 做**本地打包检查**，但**它不是发布成不成功的判据** (理由见上面那一节)。

   完成判据：`cargo publish` 退出 0 —— 只看退出码与状态，token 的值不进任何输出；且 <https://crates.io/crates/limae> 能打开、显示的版本号与 `Cargo.toml` 的 `version` 相同。

5. 推一个同版本的 tag，让二进制产物那条线也跑起来 —— 步骤见下面「三、二进制产物」。

## 二、首发之后：配置 Trusted Publishing

crate 一旦存在，就可以把 token 从 CI 里彻底去掉，改由 GitHub 的 OIDC 换取一枚短时 token。

1. 👤 **在浏览器里**打开 <https://crates.io/crates/limae>，进入 Settings → Trusted Publishing，点 "Add"，选择 GitHub，按官方文档填四个字段：

   - **Repository owner:** `Luolc`
   - **Repository name:** `limae`
   - **Workflow filename:** 那个要发布的 workflow 的文件名 (官方举的例子就是 `release.yml`)
   - **Environment:** 可选；只有在仓库里建了 GitHub Actions environment 时才填

   完成判据：Trusted Publishing 列表里出现一条指向 `Luolc/limae` 的记录。

2. 之后**另开一个 PR**把 publish job 加进 workflow。官方文档给的形状是：

   ```yaml
   jobs:
     publish:
       runs-on: ubuntu-latest
       environment: release  # Optional: for enhanced security
       permissions:
         id-token: write     # Required for OIDC token exchange
       steps:
       - uses: actions/checkout@v6
       - uses: rust-lang/crates-io-auth-action@v1
         id: auth
       - run: cargo publish
         env:
           CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}
   ```

   —— 逐字取自 [crates.io 文档](https://crates.io/docs/trusted-publishing) 的 GitHub Actions Setup 一节 (2026-09-08 核，源码在 `rust-lang/crates.io` 仓 `svelte/src/routes/docs/trusted-publishing/+page.svelte`)；同一份 YAML 也出现在上面那篇官方博客里，两处只差 `actions/checkout` 的大版本。

   **官方这份示例的 `permissions:` 只有 `id-token: write`，没有 `contents: read`。** 本仓的 `ci.yml` 顶层写着 `permissions: contents: read`，那是它自己的最小权限选择，不是 Trusted Publishing 的要求；把两者混在一起会写出一份官方没有背书的配置。真要加 `contents: read`，理由是 `actions/checkout` 需要读仓库，而不是 crates.io 要求它。

   `rust-lang/crates-io-auth-action` 的 README 只写了 `token` 这一个输出，以及「The action's `post` step automatically revokes the token when the job completes」；permissions 与 `CARGO_REGISTRY_TOKEN` 的接线它没写，要看上面 crates.io 文档那一段。

## 三、二进制产物 (GitHub Release)

`.github/workflows/release.yml` 在推 `v[0-9]+.[0-9]+.[0-9]+` 形状的 tag 时建 GitHub Release，并附上四个 target 的 `limae` 二进制：`x86_64-unknown-linux-musl`、`aarch64-unknown-linux-musl`、`x86_64-apple-darwin`、`aarch64-apple-darwin`。Linux 两个是全静态 musl —— 依赖树里没有 openssl，limae 本身也不做任何网络访问。

1. **在维护者的开发机上**，仓库根目录，确认要打 tag 的 commit 已经在 `origin/main` 上：

   ```sh
   git fetch origin && git rev-parse HEAD origin/main
   ```

   完成判据：两个 SHA 相同。不同就停下 —— 不要给未合入的 commit 打 tag。

2. 同一台机器，打 tag 并推上去 (把 `0.13.0` 换成 `Cargo.toml` 里的 `version`)：

   ```sh
   git tag v0.13.0 && git push origin v0.13.0
   ```

   完成判据：`git ls-remote --tags origin v0.13.0` 打出一行。

3. 同一台机器，盯 workflow：

   ```sh
   gh run watch --exit-status "$(gh run list --workflow=release.yml --limit=1 --json databaseId --jq '.[0].databaseId')"
   ```

   完成判据：命令退出 0。

4. 同一台机器，核对产物真的挂上去了：

   ```sh
   gh release view v0.13.0 --json assets --jq '.assets[].name'
   ```

   完成判据：四个 target 各有一个 `limae-<target>.tar.gz` 与一个对应的 `.sha256`，共八个文件名。

**这个 workflow 至今没有被任何一次真实的 tag 推送执行过。** 静态检查 (YAML 能解析、action 名与版本、`bin:` 与 `[[bin]] name` 一致、tag glob 匹配) 全部通过，但那些检查对「它在 GitHub 上跑不跑得起来」零分辨力 —— 第一次真推 tag 之前，上面第 3、4 步的完成判据都还没有被验证过。
