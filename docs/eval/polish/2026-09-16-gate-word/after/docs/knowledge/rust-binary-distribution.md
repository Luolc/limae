# Rust 分发预演

Rust 命令名为 `limae`，是本项目唯一的 CLI (ADR-0015 阶段 E，Python 参考实现已于 2026-09-10 删除)。本文说明 pre-commit 的 hook id，以及 Linux GNU / musl 的源码包和独立二进制预演；另有一节说明一键安装的两条路径 (`install.sh` 与 Homebrew formula)。macOS 与 Windows 不在本地预演范围内。发布 tag、GitHub Release 与 `cargo publish` 见 [发布手册](release.md)。

版本与组件只在仓根 `rust-toolchain.toml` 配置。源码包的文件范围由 `Cargo.toml` 的 `include` 声明：其中保留生产 binary 的编译期词表，也保留 manifest 显式声明的 library、example 与 integration test target 所需源码和共享 fixture。`spec/README.md` 与 `spec/rules.md` 是这些 fixture 和实现遵循的规范，随源码包分发。

## 验收源码包

1. 在 Linux 开发机的仓库根目录执行：

   ```sh
   tools/check_rust_package.sh package
   ```

   命令会创建并解包 Cargo 源码包，运行包内显式 targets 和共享 fixture 的测试，验证缺词表构建反臂，再从解包目录执行 `cargo install`。安装出的 binary 随后会移到全新目录，原解包路径也会移走；它须在空 `PATH` 下命中实验规则并完成术语修复。命令退出 0 即完成；临时目录会自动清理。

## 验收独立 Linux binary

1. 在具备目标的 Linux 开发机仓库根目录执行 GNU 验收：

   ```sh
   tools/check_rust_package.sh target x86_64-unknown-linux-gnu
   ```

   命令退出 0 表示 GNU binary 已在源码树外的全新工作目录中真实运行业务规则，并作为动态 ELF 对照臂被静态产物检查拒绝。

2. 在同一台机器、同一界面执行 musl 验收：

   ```sh
   tools/check_rust_package.sh target x86_64-unknown-linux-musl
   ```

   命令退出 0 表示 musl binary 已真实运行业务规则，且 `readelf` 已正向确认它是带 `Program Headers`、不含 `INTERP` 的有效 ELF；同一道检查也已拒绝合成的可执行 shell 脚本。

## 验收 pre-commit 的 hook id

本仓唯一的 id `limae` 是 Rust hook (`language: rust`)。[pre-commit 的 Rust language 合同](https://pre-commit.com/#rust) 会在自己的缓存中用 Cargo 安装 binary，不要求预装全局 `limae`。本手册与验收脚本以机器已有可用 Rust 工具链为前提；首次安装需要联网获取源码与 Cargo 依赖。

同名 id 还有另一条路径：镜像仓 `Luolc/limae-pre-commit` 的 `language: python` hook，安装的是 PyPI 上的预构建 wheel，不需要 Rust 工具链，但没有 Windows 产物。两条路径并存，不互相替换；取舍与各自的验收读数见 [发布手册](release.md)「五、pre-commit 镜像仓」，本文只覆盖本仓这条。

过渡期的 opt-in id `limae-rs` 已随默认 id 切换删除，回退 id `limae-python` 已随 Python 参考实现删除 (2026-09-10)：保留同义或无实现的 id，只会给同一件事多加一个名字 (ADR-0013 对旧名别名的处置同理)。已经钉在旧 `rev` 上的消费仓不受影响 —— 它们装出来的仍是那个 `rev` 上的定义。

1. 在 Linux 开发机的仓库根目录、提交待验收改动后执行：

   ```sh
   tools/check_precommit_consumer.sh
   ```

   命令会从当前 `HEAD` 建立本地 Git 源副本，在全新消费仓和隔离的 `PRE_COMMIT_HOME` 中安装 `limae` hook，检查合成违规、执行修复，再复查修后 clean；脚本还会在原始 `PATH` 前放置同名失败探针 (`limae` 退 97)，因此只要任一臂真跑起来，运行的一定是 pre-commit 缓存中安装的版本。pre-commit 本身由 `uvx pre-commit@4.2.0` 即时获取，不需要装机步骤，仓里也没有 Python 项目。违规退出 1 且不写回；修复会写回，并因 pre-commit 检出文件变化而退出 1；修后 clean 退出 0。命令退出 0 即完成；临时源码副本、消费仓和缓存会自动清理。

脚本不安装工具链或 target。本仓 CI 在 GitHub Actions 的临时 Linux runner 上运行 pre-commit 消费仓验收，再为钉住的 toolchain 安装 musl target，并分别运行 package / GNU / musl 三条验收。许可证已由用户于 2026-09-07 裁决为 Apache-2.0，`LICENSE` 已在仓内，`Cargo.toml` 的 SPDX 值为 `Apache-2.0`；实际发布前仍须完成发布认证，该用户项不影响本文无发布副作用的预演。

## 一键安装的旋钮与 PATH 行为

`curl -fsSL https://limae.luolc.com/install.sh | sh` 安装本机对应的 Release 二进制。三个环境变量供需要自行安排的人使用，其余人无需设置：

| 变量 | 作用 | 默认 |
| --- | --- | --- |
| `LIMAE_VERSION` | 安装指定的 Release tag，如 `v0.13.2` | 最新的那个 |
| `LIMAE_INSTALL_DIR` | 安装目录 | `~/.local/bin` |
| `LIMAE_NO_MODIFY_PATH` | 非空时不写任何文件，只打印要添加的那行 | 未设 |

```sh
curl -fsSL https://limae.luolc.com/install.sh | LIMAE_VERSION=v0.13.2 LIMAE_NO_MODIFY_PATH=1 sh
```

安装目录不在 `PATH` 上时，脚本会向当前 shell 的 rc 文件写入一段带成对标记的块；重复执行不会写入第二遍，开一个新终端即可使用。若想立刻在当前终端使用，脚本会打印那一行 `export`。**有两种情况只打印、不写文件**：rc 文件由 chezmoi 管理时 (用 `chezmoi source-path` 判定)，因为写入内容会在下次 `chezmoi apply` 时消失，用户会看到「装过了又没了」；fish 以及其它脚本无法确定 rc 语法的 shell，则打印 `fish_add_path` 那一行。只有 zsh 与 bash 会写文件，理由与横向调研读数见 [`docs/tracker.md`](../tracker.md) 里「一键安装」那条。

无法识别的 OS / CPU、校验和不匹配、机器上没有任何 sha256 工具，这三种情况都会报错退出且不安装任何内容 —— 不猜平台，也不会在无法校验时跳过校验。各自的对照臂见下一节。

## 验收一键安装

两条路径的产物都在 Release 上，安装的是同一批二进制：`install.sh` (仓根，由 CI 的 `build` job 复制进 `site/`，经 GitHub Pages 发布到 `https://limae.luolc.com/install.sh`) 与 Homebrew formula (由 `tools/render_homebrew_formula.sh` 渲染，release workflow 的 `homebrew` job 推送到 `Luolc/homebrew-tap`)。

### `install.sh`

正臂在干净 HOME 中跑完整条路径，判据落在**实际检查一个文件**上，不使用 `--help` —— `--help` 退 0 无法区分「确实可用」与「只能启动」。

```sh
H=$(mktemp -d) && cp /etc/skel/.profile /etc/skel/.bashrc "$H"/
env -i HOME="$H" PATH=/usr/bin:/bin SHELL=/bin/bash sh install.sh
printf '# 标题\n\n这是中文abc混排。\n' > "$H/probe.md"
env -i HOME="$H" PATH=/usr/bin:/bin SHELL=/bin/bash bash -lc 'limae "$HOME/probe.md"'
```

新登录 shell 中命中 zh-typography-4、退出 1，正臂才成立；同一 shell 中检查一份合规文件必须退出 0，否则无法区分「确实在检查」与「对所有文件都报错」。

对照臂逐条都要运行，它们比正臂更重要：

| 臂 | 怎么造 | 应当看到 |
| --- | --- | --- |
| 校验和 | 把下载到的 tarball 改一个字节，边车不动 | 报 checksum mismatch、退出 1、`~/.local/bin` 下什么都没有 |
| 校验和 (正) | 同一条路，字节不动 | 装成功 —— 一道拒绝所有内容的检查在负臂上同样会绿 |
| 不支持的 CPU / OS | PATH 前面放一个假 `uname` | 报错并列出四个支持的 target，不静默改装 x86_64 |
| 没有 sha256 工具 | PATH 里只留 `sh` / `curl` / `tar` 等，抽掉 `sha256sum` / `shasum` / `openssl` | 失败退出，不是「跳过校验」后装上 |
| 幂等 | 连装两次 | rc 里只有一个 `# >>> limae >>>` 块 |
| chezmoi | 给该 HOME 配一份 `chezmoi.toml`，让 rc 进 chezmoi 源 | 不写 rc，改为打印要加的那行；把 `chezmoi` 从 PATH 上拿掉再跑，同一个 HOME 会写进去 |

改一个字节的两臂需要控制服务提供的字节，因此将 tarball 与边车放在本地 HTTP 服务上，再用一份只改了传输的脚本副本运行 (`--proto '=https' --tlsv1.2` 与 `base` 两行)；校验逻辑本身没有改动。2026-09-10 已在 dev-oregon 全部运行，读数与上表一致。

### Homebrew formula

渲染和自己的负臂可以在 Linux 上运行：

```sh
tools/render_homebrew_formula.sh v0.13.2 <装着 .sha256 边车的目录> /tmp/limae.rb
```

退出 0 并打印 `rendered …`，表示四个 digest 都已读到，四条 URL 都带有这个 tag。负臂有三条：tag 不是 `v<x>.<y>.<z>`、边车缺一份、边车里不是 digest，均报错退出 1。

**`brew audit` / `brew install` / `brew test` 在本地都无法运行** —— 开发机是 Linux，且没有 Homebrew (2026-09-10 实测 `command -v brew` 无输出)，连 `ruby -c` 这样的语法检查也无法运行，因为机器上没有 ruby。因此，「formula 文件渲染出来了」无法区分「formula 能安装」与「语法正确但安装失败」，不能作为验收。真正的验收在 release workflow 的 `homebrew` job：它在 macOS runner 上运行，将 formula 放入本地 tap，依次执行 `brew audit --strict`、`brew install`、`brew test`，全部通过后才推送 tap；而整条流程可通过 `workflow_dispatch` 对已有 tag 运行，只有推送 tap 那一步只会在 tag push 时执行。**第一次 dispatch 就是这三条命令的首次运行**，此前没有它们的读数。

渲染出的 formula **不写 `version`**：Homebrew 可从下载 URL 扫出版本号，`brew audit --strict` 会将显式声明判为冗余 (2026-09-11 run 34566953423 的读数：`` `version 0.13.2` is redundant with version scanned from URL ``，也是 `brew audit` 首次执行)。删除该行后，「版本号是否正确」便没有断言约束：URL 命名改动时，版本号可能静默出错，而扫出错误版本的 formula 仍会通过 audit。`tools/check_formula_version.sh` 接手这项检查：它在 `homebrew` job 中、audit 之前运行，将 `brew info --json=v2` 读出的 `versions.stable` 与 `${TAG#v}` 正向比对。

这条断言的两臂可以在 Linux 上运行 —— 将假 `brew` 放在 PATH 前面，只让它按 `STUB_VERSION` 输出 `brew info --json=v2` 的形状：版本等于 tag 时退出 0 并打印读到的版本；版本差一个补丁号 (`0.13.1` 对 `v0.13.2`) 时退出 1，并同时报出两个值 (2026-09-11 dev-oregon 实测)。另外四条负臂同日实测均退 1：`versions.stable` 是 `null`、`brew info` 自己退非 0、tag 不是 `v<x>.<y>.<z>`、参数个数不对。**假 `brew` 验的是比对逻辑，不是真实 Homebrew 的 JSON 形状** —— 后者与 audit、install、test 一样，只有 runner 上那次 dispatch 能确认。
