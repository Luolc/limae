# 发布手册

`limae` 的发布分两条独立的线：**GitHub Release 的二进制产物**由 tag 触发的 workflow 自动产出，**crates.io 的 `cargo publish`** 首发时在维护者的开发机上带 API token 执行 (0.13.0 即如此)，此后交给同一个 tag 触发的 workflow、走 Trusted Publishing 的 OIDC 路径。本文覆盖两条线各自的步骤、判据与它们之间的先后关系。

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

**措辞要准：这去掉的是 limae 的 CI 对长期 token 的需要，不是那枚 token。** 用户 2026-09-08 裁决**保留**它 (以后还有别的 Rust project 要发)，所以配好 Trusted Publishing 之后它**仍然存在、半径不变** —— 无有效期、crate scope 留空、覆盖账号下的全部 crate。不要把这件事写成「长期 token 已消除」。

1. 👤 **在浏览器里**打开 <https://crates.io/crates/limae>，进入 Settings → Trusted Publishing，点 "Add"，选择 GitHub，按官方文档填四个字段：

   - **Repository owner:** `Luolc`
   - **Repository name:** `limae`
   - **Workflow filename:** `release.yml` —— 发布 job 所在的 workflow 文件名，本仓就是这个值 (官方文档举的例子恰好也是 `release.yml`)
   - **Environment:** 可选；只有在仓库里建了 GitHub Actions environment 时才填

   完成判据：Trusted Publishing 列表里出现一条指向 `Luolc/limae` 的记录。

2. publish job 已经在 `.github/workflows/release.yml` 里，这一步不必再做，留在这里的是它的出处与形状。官方文档给的形状是：

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

   仓内那份与官方示例的差异有四处，各有理由：`actions/checkout` 用本仓其余 workflow 的同一个大版本而不是示例里的版本；不写 `environment:` (仓库里没有建 GitHub Actions environment，字段 4 旁边那个 Environment 也因此留空)；**加了 `contents: read`**，理由就是上一段那条 —— job 级 `permissions:` 会把没列出的 scope 全部置为 none，而 `actions/checkout` 要读仓库；以及下面这条 tag 与 manifest 的守卫。

### tag 与 manifest 版本必须逐字相等

`cargo publish` 上传的是**当前 `Cargo.toml` 的 `[package].version`**，tag 根本不是它的输入。所以推 `v0.13.1` 到一个 manifest 写着 `0.14.0` 的 commit，发出去的是 `0.14.0` —— 而 crates.io 的版本**不能覆盖、不能删除**，`yank` 也只是标记、不删代码；同一次运行建出的 GitHub Release 却仍叫 `v0.13.1`，两条线就此分叉。

`publish` job 因此在**认证之前**断言 `$GITHUB_REF_NAME` 等于 `v` 加上 manifest 里的版本。判据写成正向链而不是排除已知的坏情况：版本必须**读得出来** (`cargo metadata` 与 `jq -er` 任一失败即退出)，且必须**等于** tag；其余一切情况都停在这一步。

这个守卫只属于 `publish`。**`auth-check` 不带它** —— 那个 job 验的是 OIDC 链路通不通，与发哪个版本无关，`workflow_dispatch` 触发时也根本没有 tag 可比。

三臂读数 (2026-09-08 本机跑 job 里那段 shell，不是在 runner 上)：`v0.13.0` 对 manifest `0.13.0` 退出 0；`v0.13.1` 对同一 manifest 退出 1；在没有 manifest 的目录里退出 4。第三臂守的是「读不出来时不许放行」这半条 —— 只测前两臂的话，一个把版本读成空串的实现在 tag 也为空时同样能绿。**这三臂只证明那段 shell 的判断对，不证明它在 GitHub runner 上跑得起来** (`GITHUB_REF_NAME` 由 Actions 注入、`jq` 由 runner 镜像提供，两者本机都是手工给的)。

### 三种验证方式与各自的分辨力

配好之后想确认它真的能用，有三条路，代价与结论的确定性各不相同。**这里不替你选**，只写清楚每条路给得出什么。

- **A. 只跑认证，不发布。** `release.yml` 里的 `auth-check` job：在 Actions 页面手动触发 (`workflow_dispatch`)，它做 checkout + auth action，断言拿到了 token 就停，不跑 `cargo publish`。不消耗版本号，随时可重跑。**结论是单向的** —— 成功 ⇒ owner / repository / workflow 文件名三项都对上了、OIDC 链路通，这一步确定；失败 ⇒ 指向的是配置或链路本身，**不再与「事件类型不被接受」混同** —— 这条原因已被实测排除：合入 #144 后 `gh workflow run release.yml --ref main` 触发了 run `34293764632` (event `workflow_dispatch`)，`auth-check` 结论 `success` (2026-09-08 实测)，说明 crates.io 接受 `workflow_dispatch` 触发的 OIDC token。同一次运行里 `publish`、`create-release`、`upload-assets` 三个 job 全部 `skipped`，证明 `if` 条件按设计工作、手动触发不会在非 tag 的 ref 上建 Release。**这不是「已完全验证」**：这次运行证明的只是认证那一步通了，`publish` job 本身 (认证之后的 `cargo publish`，以及 tag 与 manifest 一致性守卫在 runner 上的实际行为) 仍然一次都没被真实执行过。

  另有一条形状上的约束：**workflow 文件名是配置的一部分**，所以这个认证测试必须跑在 `release.yml` 里面。把它挪进一个新建的 `tp-test.yml`，即使配置完全正确也会失败 —— 那是假阴性。同理，`workflow_dispatch` 的入口只在默认分支上暴露，所以要先合进 main 才点得到。

- **B. 等下一次真发布。** 什么都不做，下次推 tag 时由 `publish` job 给出答复。**确定**，且失败的代价比看起来小：`cargo publish` 的服务端是原子的，不会发出去一半；认证失败只是这一个 job 红，改完重跑即可，版本号没被消耗，GitHub Release 与二进制产物那条线由别的 job 承担、不受影响。这条路上「发错版本号」那个风险由上面那道 tag 与 manifest 的守卫挡着，它在认证之前就跑。

- **C. 故意烧一个补丁版号。** 现在就 bump 一个 patch 版本、推 tag，让真发布立刻给出答复。**立刻确定**，代价是 crates.io 上多一个版本号 —— crates.io 的版本不可删除 (只能 yank)。

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

这条线**已经被一次真实的 tag 推送执行过**：`v0.13.0` 于 2026-09-08 触发 release workflow (run `34213744281`)，结论 success，`gh release view v0.13.0` 列出八个文件名 —— 四个 target 各一个 `.tar.gz` 与一个 `.sha256`。上面第 3、4 步的完成判据至此都拿到了读数。

**这不覆盖同一个文件里后加的两个 job** (`publish` 与 `auth-check`，见「二」)：那次运行发生在它们存在之前，它们各自的第一次真实执行仍未发生。
