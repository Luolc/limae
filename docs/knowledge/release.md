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

版本只有一个来源：`Cargo.toml` 的 `version`。Python 参考实现自 0.13.0 起冻结 (用户 2026-09-08 裁决)、2026-09-10 整体删除 (ADR-0015 补记)，从未有过发布物。

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

## 四、PyPI 与 npm 启动器

同一个 tag 还会发两层薄启动器 (launcher)：PyPI 上每个 target 一份 wheel (`pip install limae` / `uv add --dev limae`)，npm 上一个主包 `@limae/cli` 加四个平台包 `@limae/<platform>` (`npm i -D @limae/cli`，主包经 `optionalDependencies` 拉对应平台包，`bin/limae.js` 找到它、执行里面的二进制)。形状的选型见 [跨生态分发调研](../research/distribution-cross-ecosystem.md) §7，本文只写机制与判据。

**npm 主包为什么带 scope**：无 scope 的 `limae` 发不出去 —— `v0.13.1` 的 release run `34547841511` 里 `publish-npm` 报 `403 Package name too similar to existing packages livan,mime; try renaming your package to '@luolc/limae'`，这是 npm 的抢注防护 (typosquatting 防护)，与凭证无关、换 token 过不去。`@limae` scope 已是我们的，所以主包定名 `@limae/cli` (`@angular/cli` / `@nestjs/cli` 是这个形状的先例)，**装的命令变成 `npm i -D @limae/cli`，敲的命令仍是 `limae`** (`bin` 名没变)。调研文档记的是当时的判断，不回改。

这件事在打包脚本里留下一个必须按名字而不是文件名做的分类：`npm pack` 用包名拼文件名，`@limae/cli` 打出的是 `limae-cli-<version>.tgz`，任何 `limae-*-*.tgz` 这样的 glob 都会把它读成平台包、把 launcher 那组数成 0 个。所以 `tools/npm_publish_set.sh` 改成解开每个 tarball 读 `package.json` 的 `name` 来分组 —— 那正是 registry 认的那个值，文件名一直只是它的代理 (proxy)。

**它们不编译任何东西。** `tools/build_launchers.sh <tag> <assets-dir> <out-dir>` 从 Release 的 `limae-<target>.tar.gz` (先过它的 `.sha256` 边车) 里取出二进制原样放进 wheel 的 `.data/scripts/limae` 与 npm 平台包的 `limae`；wheel 由 `uvx wheel pack` 打出 (它算 RECORD)，npm 包由 `npm pack` 打出，仓里没有 Python、没有 `pyproject.toml`。包版本取 tag，不取 manifest：二进制是 tag 的，而 dispatch 预演时检出的是 main。描述、license、repository 三个 metadata 串取自 `cargo metadata`，只有一个来源。

**硬不变量：用户从 PyPI 或 npm 装到的二进制与 GitHub Release 上那份是同一份字节。** 「构造上就是这么做的」不算证据 —— 那对「同一份字节」与「两次编译碰巧都能跑」给出相同输出。证据是 `tools/check_launchers.sh <tag> <assets-dir> <out-dir>`：对每个 target 比两个值，一边是**从 Release tarball 里重新解出的**二进制的 sha256 (tarball 先过边车，所以这个值链到 Release 页面公布的那份)，另一边是**从打好的 wheel / npm tarball 里重新解出的**二进制的 sha256 (用安装器同一套工具解成品包，不看构建时的中间目录)；两个值都打进日志，任一不等即红。它还断言两个输出目录里**正好**是预期的那 11 个文件、每个模式命中的是不同的文件：只数命中次数的话，多一个没人认识的 wheel (会被 `publish-pypi` 整目录上传) 或一个同时带两个 tag 的 wheel 顶替两份，都能凑回同一个数 (审查方 2026-09-10 两条反臂实测假绿，同日修)。反臂 (2026-09-10 本机实测，读数记在引入这道门的 PR 描述里)：在一个 wheel 的二进制里改一个字节、重新打包 → 红并打出两个不同的值；在一个 npm 平台包里改一个字节 → 红；空的输出目录 → 红；多一个 `win_amd64` wheel → 红；双 tag wheel 顶替两份 → 红；改回 → 绿。

平台映射写在 `tools/build_launchers.sh` 顶部那张表里，两处取舍：**Linux 只有 musl 产物**，静态 musl 二进制在 glibc 系统上照跑，所以同一份文件在 PyPI 上发两个 wheel (manylinux 与 musllinux 各一，pip 按它探测到的 libc 选)、在 npm 上只发一个包 (`os` / `cpu` 不带 `libc` 字段，两类系统都装)；**没有 Windows 产物**，所以映射里没有 Windows 那一行，`optionalDependencies` 与 wheel 矩阵都不写一个不存在的 target，启动器在不支持的平台上退出 1 并说明。

### 预演 (dry run)：不发布任何东西

PyPI 的版本号**永远不能删除或重用**，npm 只有 72 小时反悔窗口，所以验收看「机器对不对」，不看「发出去没有」。在 Actions 页面手动触发 `release.yml` (`workflow_dispatch`)，`tag` 填一个**已存在的** Release tag：`launchers` job 下载那个 tag 的 8 个资产、构建、跑 sha256 比对、`twine check --strict`、`npm publish --dry-run`、在 runner 上真装一次 x86_64 Linux 的 wheel 与 npm 包并跑 `limae --help`，然后停；`auth-check` (crates.io) 与 `npm-auth-check` (`npm whoami`，token 过期或贴错即红) 同时跑。**`tag` 不给默认值**：默认值会过期，「忘了填」与「就要这个」会给出相同行为。

同一条预演在本机也能裸跑 (`gh release download <tag> --pattern 'limae-*' --dir <assets-dir>` 之后依次跑上面两个脚本)，退出码为准、不接管道。

### 预演里没有 PyPI 那一臂，这是有意的

crates.io 与 npm 在 dispatch 上各有一个只验凭证的 job，PyPI 没有。原因是 PyPI 的这一臂**做不到「只验证」**：**它的 mint-token 端点对 pending publisher 会在 mint 那一刻建出项目、把 pending 转正** (warehouse 仓 `warehouse/oidc/views.py` 的 `mint_token`：`project_service.create_project(...)` 与 `oidc_service.reify_pending_publisher(...)`，2026-09-10 读 main 分支)。跑一次就在 PyPI 上留下一个项目 —— 不烧版本号，但对外可见、不可逆。

曾经有过一个 `pypi-auth-check` job 挂在 dispatch 的布尔输入后面，默认 false，为的是在**首发之前**验一次那条链通不通 —— 那时没有别的办法。`limae` 0.13.1 与 0.13.2 都已发到 PyPI，那个问题因此被更强的证据回答了，它也随之删掉 ([PR #173](https://github.com/Luolc/limae/pull/173))：项目已存在、publisher 已转正，它再也验不到发布没验过的东西，只剩一个永远绿的开关和那个副作用。

**别把它照抄到新项目里。** 一个带不可逆副作用的检查，代价要与它买到的东西一起算：它买到的只是「更早知道」，而 TP 配错时真实发布本来就在上传前安全失败。

PyPI 侧的 `environment:` 留空：pending publisher 页面上 Environment name 显示 `(Any)` (用户 2026-09-10 截图，经 orchestra 转述)，与 crates.io 那边一致，所以 `publish-pypi` 不写 `environment:` —— 写了反而是给自己加一个没被要求的条件。

### 发布顺序与失败重跑

tag 推上去后三家按「越不可撤销越先」发，且互相串着：`publish` (crates.io，先断言 tag == manifest) → `publish-pypi` (`pypa/gh-action-pypi-publish`，OIDC) → `publish-npm` (四个平台包先发、主包 `@limae/cli` 最后，`optionalDependencies` 落地即可解析)。链上任一家红，后面的不发。

**中途失败后可以 `gh run rerun --failed` 续，靠的是两个发布 job 各自先问 registry**：PyPI 与 npm 都不允许同名文件 / 同 name@version 上传两次，所以「从头再传一遍」会撞上已经落地的那几个，而 PyPI action 的 `skip-existing` 分不开「同一份已传」与「别的东西占了这个名字」。`tools/pypi_upload_set.sh <project> <version> <dir>` 用 PyPI 的 JSON API 拿到该版本已有文件的 sha256，与本次成品逐个比：不在 → 留着上传，同 sha256 → 从目录删掉，不同 → 退出 1 什么都不传 (PyPI 答 404 即全传，其它状态码一律退出 1)；`tools/npm_publish_set.sh <dir> [--dry-run]` 对每个 tgz 用 `npm view <name@version> dist.integrity --json` 比本地 tarball 的 sha512，先比完全部再发第一个，任一不同即停；只有 registry 明确答 E404 才算「不在」，连不上、5xx 之类一律退出 1 (查不到不等于没有)。三臂读数 (2026-09-10 本机)：PyPI 侧对 `six 1.17.0` 的真 wheel 报 dropped、假文件名报 to be uploaded、改一字节的同名文件退出 1；npm 侧对 `npm pack` 下来的两个真包报 skipped、重新打包内容不同的那份退出 1、我们自己那 5 个未发布的包全部 to be published、registry 指到不可达地址时退出 1 且零次 publish；PyPI 侧另有一臂：全部 wheel 都已在 PyPI 时 `publish-pypi` 那步数出 `count=0`、上传 action 被跳过 (计数前开 `nullglob`，否则空目录数出的是模式自己那一个)。

npm 上五个包全部带 `@limae` scope，scoped 包默认 restricted，所以 `publishConfig.access = "public"` 写在每个 `package.json` 里跟着包走，不依赖发布命令怎么写。

### npm 的凭证：今天是 `NPM_TOKEN`，要迁到 Trusted Publishing

**今天的状态**：`publish-npm` 与 `npm-auth-check` 用的是仓库 secret `NPM_TOKEN` (2026-09-10 存入)，经 `actions/setup-node` 的 `registry-url` 接到 `NODE_AUTH_TOKEN`。三家里只有 npm 还留着长期凭证，crates.io 与 PyPI 都走 OIDC。

**要迁的理由是「CI 里不留长期凭证」**，与那枚 token 什么时候到期无关 —— 到期只是提醒，不是理由。

**顺序是这件事的全部难点，而且错的那一侧不可逆：**

> **必须先由用户在 npmjs.com 上把五个包一个个配好 (下面 👤 那一节)，才能改 CI。**
> 反过来做，下一次推 tag 时 `publish-npm` 直接发不出去 —— 而那时 crates.io 与 PyPI 已经收下了这个版本号，两家都不能删除或重用，只能 bump 一个新版本往前走。

所以**本仓今天的 `release.yml` 仍是 token 那条路，一个字没改**，等配置到位再开 PR 改 CI。

#### 👤 用户要做的：给五个包各配一次 Trusted Publisher

npm 的 Trusted Publishing **是逐包配的，没有 scope 级或 org 级入口** (官方文档原文：「Navigate to your package settings on npmjs.com and find the "**Trusted Publisher**" section.」)，所以五个包就是五次，每次填的内容完全一样。包必须已经存在才配得了 —— 五个包都已在 0.13.2，这个前提满足。

在浏览器里做，先登录 npm 账号。手机浏览器也打得开，但这一节每个包都有四个要**逐字照抄**的字段、外加一个必须手选的勾，**能用电脑就用电脑**。对下面五个包各做一遍：

1. `@limae/cli`
2. `@limae/linux-x64`
3. `@limae/linux-arm64`
4. `@limae/darwin-x64`
5. `@limae/darwin-arm64`

每个包的步骤，一次一个动作：

1. 打开这个包的页面，例如 <https://www.npmjs.com/package/@limae/cli>，进它的 **Settings**。

   直链多半是 `https://www.npmjs.com/package/<包名>/access`，但**这个路径没核过** —— npmjs.com 对没登录的请求一律答 403，连一个明摆着不存在的路径也答 403 (2026-09-10 四臂实测：包页面、`/access`、不存在的包、瞎编的路径，四个都是 403)，所以「403」对「路径对不对」零分辨力。直链打不开就从包页面点进 Settings，官方文档给的也只是这一句：「Navigate to your package settings on npmjs.com」。
2. 找到 **Trusted Publisher** 这一栏，在 **Select your publisher** 下面点 **GitHub Actions** 那个按钮。
3. 填四个字段，**逐字照抄，大小写敏感**：
   - **Organization or user**：`Luolc`
   - **Repository**：`limae`
   - **Workflow filename**：`release.yml` —— 只填文件名，不要填 `.github/workflows/release.yml` 这样的路径，且必须带 `.yml` 后缀
   - **Environment name**：**留空** (本仓没有建 GitHub Actions environment，与 crates.io、PyPI 两边的配置一致)
4. **Allowed actions：必须亲手勾上 `npm publish`。这一步单独成一步，因为它是这一节唯一会静默出错的地方。**

   官方注记原文：「Configurations created after Sep 03, 2026 are automatically set to allow `npm stage publish`, and you can choose whether to also permit direct publishing with `npm publish`.」今天 (2026-09-10) 落在这条里 —— **按默认保存，配出来的是「只允许 staged publish」**，而我们的 `publish-npm` 敲的是 `npm publish`，会被拒。

   **这件事在配置过程中看不出来**：五个包全配完、每一步都显示成功，然后下一次发版红。所以要盯着这一栏本身看，**不要以「保存成功了」当判据**。

   **这一步的完成判据**：保存前，`npm publish` 那个勾是**勾上的** (`npm stage publish` 勾不勾都行，它本来就总是允许)。

5. 保存。

   **这个包的完成判据**：页面刷新后，Trusted Publisher 一栏里出现一条记录，显示 GitHub Actions、`Luolc`、`limae`、`release.yml`，**且 allowed actions 那一项里看得到 `npm publish`**。看不到就是没配上，重来一次 —— 别放过去。

五个包 (`@limae/cli`、`@limae/linux-x64`、`@limae/linux-arm64`、`@limae/darwin-x64`、`@limae/darwin-arm64`，2026-09-10 从 registry 核过，均为 0.13.2) 都做完之后，回报一句「五个包都配好了」，CI 那边的改动才开始。

**npm 不替你验配置**，官方原文：「npm does not verify your trusted publisher configuration when you save it. Double-check that your repository, workflow filename, and other details are correct, as errors will only appear when you attempt to publish.」所以上面第 3 步的逐字照抄是这一节唯一的判据来源，填错要到下一次真发布才会知道。

#### CI 侧要改什么 (等配置到位后另开 PR)

记在这里是为了到时候不必重新查一遍，**这些今天都还没做**：

- `publish-npm` 加 `id-token: write` (`contents: read` 保留，`actions/checkout` 要读仓库)。
- **`actions/setup-node` 那一步去掉 `registry-url`，而不是只删 `NODE_AUTH_TOKEN`。** 带 `registry-url` 时它会往 `.npmrc` 写一行 `//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}`；`NODE_AUTH_TOKEN` 不存在时它展开成空串，npm 认为「auth 已经配好了」，于是**根本不发起 OIDC 交换**，报 `ENEEDAUTH` 或 404。(来源：[actions/setup-node#1551](https://github.com/actions/setup-node/issues/1551)、[npm/documentation#1960](https://github.com/npm/documentation/issues/1960)，2026-09-10 读，两个 issue 都还开着。**这一条是从 issue 读来的，本仓没有在 runner 上亲验过。**)
- npm CLI **>= 11.5.1**、Node **>= 22.14.0** (官方文档)。`node-version: 24` 今天满足 Node 那一半，npm 那一半随 runner 镜像走 —— 改 CI 时加一条正向断言把版本打进日志，别假定。
- **不要加 `--provenance`。** OIDC + 公开仓 + 公开包三条同时成立时 npm 自动生成 provenance，官方原文：「This happens by default—you don't need to add the `--provenance` flag to your publish command.」三条我们都成立。
- `npm-auth-check` 一并删掉：它验的是 `NPM_TOKEN` (`npm whoami`)，token 没了它就没有对象。**npm 侧没有 crates.io `auth-check` 那样的等价物** —— npm 的 OIDC 交换只挂在 `npm publish` 上，没有一条「只认证不发布」的命令。所以这一步换来的是**预演里少一条 npm 凭证臂**，OIDC 通不通只有下一次真发布才知道；这是这次迁移的已知代价，不要拿一个看起来像检查的东西补上它。
- `NPM_TOKEN` 这个仓库 secret 由用户在配置与 CI 改动都落地、且下一次发版验证成功之后再删。
- `repository.url` 必须与 GitHub 仓库逐字相符 (官方对 GitHub 这条路的要求)。`tools/build_launchers.sh` 生成的是 `git+https://github.com/Luolc/limae.git`，已相符，不必改。

## 五、pre-commit 镜像仓

同一个 tag 还会写第三处：[`Luolc/limae-pre-commit`](https://github.com/Luolc/limae-pre-commit)。它是一个只有五个文件的仓 (`.pre-commit-hooks.yaml`、`pyproject.toml`、`README.md`、`AGENTS.md`、`LICENSE`)，**仓库语言英文** (与 `Luolc/homebrew-tap` 同一个理由：读者是任何生态的 pre-commit 用户)。

**它解决的是什么**：pre-commit 一共 21 种 language，**没有一种是「下载预编译 binary」**。`language: python` 之所以等价于零额外依赖，是因为 pre-commit 自己就是 Python 写的 —— 能跑 pre-commit 就一定有 Python，ruff 钻的也是这个空子。于是消费方 `pip install` 的是我们已经在发的那个 wheel，里面是预构建二进制 (那个 wheel **没有一行 Python 代码**：只有 `limae-<version>.data/scripts/limae` 加 dist-info，`Root-Is-Purelib: false`)。

**为什么是独立一个仓，不放本仓**：这条合同要求 pre-commit 能在克隆下来的 hook 仓根上 `pip install .`，也就是仓根要有 `pyproject.toml`。放本仓等于把 `pyproject.toml` 放回一个刚刚整体删除 Python 的仓 (ADR-0015 补记)，与那次改动的目的直接冲突。

**两条 hook 并存，不是替换**：Release 只有四个 target、没有 Windows 产物，所以 Windows 上 `pip install limae` 找不到 wheel、镜像仓那条装不上，而本仓的 `language: rust` 从源码编译、在 Windows 上能用。取舍表在 [README](../../README.md)「接 pre-commit」，本仓的 `.pre-commit-hooks.yaml` 因此保留。

### `mirror` job：推，不轮询

ruff 用一个 `mirror.py` 轮询 PyPI，是因为它的镜像仓与主仓解耦。我们不解耦：`release.yml` 的 `mirror` job 在 `publish-pypi` 之后直接写过去，版本号自己就知道，不必让机器人去猜。

`needs: publish-pypi` 是这里唯一的顺序要求，理由是**故障会挪地方**：镜像仓上一个 tag 若把 `limae==` 钉到 PyPI 还没有的版本，报错不在这次 run 里，而在几天后某个消费者的仓库里、`pip install` 的时候。

job 做四件事，每件都留正向判据：

1. **先断言 PyPI 真的服务这个版本，再写任何东西**：`uvx --from "limae==<version>" limae --help`。`needs` 只证明上传那一步返回了成功，不证明索引已经答得出来 —— 两者之差正是上一段说的那种故障。这一步同时也真跑了一次 wheel 里的二进制。
2. **改 pin**：`sed` 改 `pyproject.toml` 的 `dependencies` 那一行，随后 `grep -Fxq` 断言改完的文件里**确实**有那一行。判据在 grep 上，不在 sed 上 —— `sed` 的模式一个都没匹配上时它照样退出 0。
3. **提交**：pin 没变就不提交 (重跑时的常态)，变了才 commit 并推 `main`。
4. **打 tag** `v<version>`：tag 已存在且指向本次要打的那个 commit，就是这个 job 跑过一次，绿；指向别处则退非零，**不移动别人的 tag**。`gh run rerun --failed` 因此可以续。

   这一步的比较对象必须是 **commit**，不是 tag ref 本身指向的对象。annotated tag 那一行 `ls-remote` 给的是 **tag object** 的 sha，而不是它指向的 commit；且 `git ls-remote --tags origin "refs/tags/<tag>"` 这种带 pattern 的查询**会把 `^{}` 那行滤掉**，于是拿一个 tag object 去比一个 commit，本该绿的重跑会红。所以这里列出全部 tag、优先取 `refs/tags/<tag>^{}` 那行的 sha，没有才退回直接那行。这个 job 自己打的是 lightweight tag，但手工或从 GitHub 界面切出来的是 annotated，两者要分得开而不是假定不会出现。

   四臂读数 (2026-09-10 本机，隔离的 bare remote，同一段代码只换 `$TAG`)：annotated tag 指向 `HEAD` → 退 0；lightweight tag 指向 `HEAD` → 退 0；**annotated tag 指向另一个 commit → 退 1** (对照臂：一个恒判相等的「修法」在这一臂上会绿)；tag 不存在 → 走创建分支。改回带 pattern 的写法时，第一臂给出的是 tag object 的 sha、比较不等 (审查方 2026-09-10 在 PR #169 以 P1 提出，本机独立复现)。

**凭证**：仓库 secret `PRECOMMIT_MIRROR_TOKEN`，一枚能写 `Luolc/limae-pre-commit` 的 token，由用户创建并存入。job 的 `permissions: {}` —— 它不碰本仓，`GITHUB_TOKEN` 一项权限都不需要。

### 镜像仓里哪些是生成的

只有 `pyproject.toml` 里 `limae==` 那一行，和 `v<version>` 那些 tag。`.pre-commit-hooks.yaml`、`README.md`、`AGENTS.md`、`LICENSE` 都是手写的，workflow 一个字都不碰 —— 改 hook 定义 (`entry`、`files`、加一个 id) 是对那个仓的普通改动。这条写在那个仓的 `AGENTS.md` 里。

### `args:` 里不放规则开关

`--disable` / `--enable` 是**替换**配置文件而不是叠加 (`rust/config.rs` 的 `resolve`：`if cli.is_present() { return resolve_cli(cli); }`)，命令行上一旦出现其中任何一个，`limae.toml` 与 `pyproject.toml` 的 `[tool.limae]` 根本不会被读，**且不报错、不警告**。所以镜像仓的 hook 定义里没有 `args:`，README 两边都写明规则一律进配置文件。

可执行证据 (2026-09-10 本机，`uvx --from limae==0.13.2 limae`，消费仓根放 `disable = ["zh-typography-4"]` 的 `limae.toml`、一份含 `中A` 的 `.md`)：不带 flag 时退出 0、报 `OK: 1 file(s) clean`；带一个**无关的** `--disable zh-typography-3` 时退出 1 并报出 `zh-typography-4`，摘要行是 `1 error(s), 0 warning(s)` —— 配置被整个忽略了，而输出里没有任何一句提到这件事。

### 验收：真消费一次，分辨力在失败探针上

形状照 `tools/check_precommit_consumer.sh`：全新空仓 + 隔离的 `PRE_COMMIT_HOME`，`.pre-commit-config.yaml` 指向镜像仓，`PATH` 最前面放一个同名的失败探针 (`limae` 退 97)。**没有这一臂，整个验收在「镜像仓工作了」与「这台机器上本来就有 limae」两种情况上给出相同输出。**

2026-09-10 本机读数 (`rev` 取镜像仓首个 commit `1605d4da`)：`install-hooks` 退 0，缓存里正好一个 `py_env-*/bin/limae` 且它跑得起来、**没有任何 `rustenv-*`** (装的是 wheel 不是编译产物)；违规退 1 且报 `zh-typography-4`、文件未被改写；`--fix` 那一臂退 1 且写回 `中 A`；修后复查退 0；放 `limae.toml` 关掉 `zh-typography-4` 后同一份违规文件退 0 (配置被尊重)；把 `--disable zh-typography-3` 写进 `args:` 后退 1 并报 `zh-typography-4` (配置被忽略)。
