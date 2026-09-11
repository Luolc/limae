# Rust 分发预演

Rust 命令名为 `limae`，是本项目唯一的 CLI (ADR-0015 阶段 E，Python 参考实现已于 2026-09-10 删除)。本文覆盖 pre-commit 的 hook id，以及 Linux GNU / musl 的源码包与独立二进制预演，另有一节给一键安装的两条路 (`install.sh` 与 Homebrew formula)；macOS 与 Windows 不在本地预演范围内。发布 tag、GitHub Release 与 `cargo publish` 见 [发布手册](release.md)。

版本与组件的唯一配置来源是仓根 `rust-toolchain.toml`。源码包的文件范围由 `Cargo.toml` 的 `include` 声明；其中既保留生产 binary 的编译期词表，也保留 manifest 显式声明的 library、example 与 integration test target 所需源码和共享 fixture。`spec/README.md` 与 `spec/rules.md` 作为这些 fixture 和实现所遵循的规范正本随源码包分发。

## 验收源码包

1. 在 Linux 开发机的仓库根目录执行：

   ```sh
   tools/check_rust_package.sh package
   ```

   命令会创建 Cargo 源码包、解包、运行包内显式 targets 与共享 fixture 的测试、验证缺词表构建反臂，再从解包目录执行 `cargo install`。安装出的 binary 随后被移到全新目录，原解包路径也会被移走；它须在空 `PATH` 下命中实验规则并完成术语修复。命令退出 0 即完成；临时目录会自动清理。

## 验收独立 Linux binary

1. 在已具备目标的 Linux 开发机仓库根目录执行 GNU 验收：

   ```sh
   tools/check_rust_package.sh target x86_64-unknown-linux-gnu
   ```

   看到命令退出 0，表示 GNU binary 已在源码树外的全新工作目录真实运行业务规则，并作为动态 ELF 对照臂被静态产物门拒绝。

2. 在同一台机器、同一界面执行 musl 验收：

   ```sh
   tools/check_rust_package.sh target x86_64-unknown-linux-musl
   ```

   看到命令退出 0，表示 musl binary 已真实运行业务规则，且 `readelf` 正向确认它是带 `Program Headers`、不含 `INTERP` 的有效 ELF；同一道门也已拒绝合成的可执行 shell 脚本。

## 验收 pre-commit 的 hook id

本仓的唯一 id `limae` 是 Rust hook (`language: rust`)。[pre-commit 的 Rust language 合同](https://pre-commit.com/#rust) 会在自己的缓存中用 Cargo 安装 binary，不要求预装全局 `limae`。本手册与验收脚本以机器已有可用 Rust 工具链为前提；首次安装需要联网取得源码与 Cargo 依赖。

同名的 id 另有一条路径：镜像仓 `Luolc/limae-pre-commit` 的 `language: python` hook，装的是 PyPI 上的预构建 wheel、不需要 Rust 工具链，但没有 Windows 产物。两条并存不是替换，取舍与它自己的验收读数见 [发布手册](release.md)「五、pre-commit 镜像仓」，本文只覆盖本仓这条。

过渡期的 opt-in id `limae-rs` 已随默认 id 切换删除，回退 id `limae-python` 已随 Python 参考实现删除 (2026-09-10)：留一个同义或已无实现的 id 只是第二个名字指同一件东西 (ADR-0013 对旧名别名的处置同理)。已经钉在旧 `rev` 上的消费仓不受影响 —— 它们装出来的仍是那个 `rev` 上的定义。

1. 在 Linux 开发机的仓库根目录、提交待验收改动后执行：

   ```sh
   tools/check_precommit_consumer.sh
   ```

   命令会从当前 `HEAD` 建立本地 Git 源副本，在全新消费仓与隔离的 `PRE_COMMIT_HOME` 中安装 `limae` hook，检查合成违规、执行修复，再复查修后 clean；脚本还会在原始 `PATH` 前放置一个同名失败探针 (`limae` 退 97)，所以任何一臂真跑起来了，跑的就一定是 pre-commit 缓存内装出来的那个。pre-commit 本身由 `uvx pre-commit@4.2.0` 现取，不需要装机步骤，仓里也没有 Python 项目。违规退出 1 且不写回，修复会写回并因 pre-commit 检出文件变化而退出 1，修后 clean 退出 0。命令退出 0 即完成；临时源码副本、消费仓和缓存会自动清理。

脚本不安装工具链或 target。本仓 CI 在 GitHub Actions 的临时 Linux runner 上运行 pre-commit 消费仓验收，再为钉住的 toolchain 安装 musl target，并逐项运行 package / GNU / musl 三条验收。许可证已由用户于 2026-09-07 裁决为 Apache-2.0，`LICENSE` 已在仓内，`Cargo.toml` 的 SPDX 值为 `Apache-2.0`；实际发布前仍须完成发布认证，该用户项不阻塞本文的无发布副作用预演。

## 验收一键安装

两条路的产物都在 Release 上，装的是同一批二进制：`install.sh` (仓根，由 CI 的 `build` job 复制进 `site/`，经 GitHub Pages 送到 `https://limae.luolc.com/install.sh`) 与 Homebrew formula (由 `tools/render_homebrew_formula.sh` 渲染、release workflow 的 `homebrew` job 推到 `Luolc/homebrew-tap`)。

### `install.sh`

正臂在一个干净 HOME 里跑完整条路，判据落在**真检查一个文件**上，不落在 `--help` 上 —— `--help` 退 0 在「真能用」与「只是能启动」两种情况下给出相同输出。

```sh
H=$(mktemp -d) && cp /etc/skel/.profile /etc/skel/.bashrc "$H"/
env -i HOME="$H" PATH=/usr/bin:/bin SHELL=/bin/bash sh install.sh
printf '# 标题\n\n这是中文abc混排。\n' > "$H/probe.md"
env -i HOME="$H" PATH=/usr/bin:/bin SHELL=/bin/bash bash -lc 'limae "$HOME/probe.md"'
```

新登录 shell 里命中 zh-typography-4、退出 1 即正臂成立；同一个 shell 里检查一份合规文件必须退出 0，否则「真在查」与「见谁都报」分不开。

对照臂逐条都要跑，它们比正臂重要：

| 臂 | 怎么造 | 应当看到 |
| --- | --- | --- |
| 校验和 | 把下载到的 tarball 改一个字节，边车不动 | 报 checksum mismatch、退出 1、`~/.local/bin` 下什么都没有 |
| 校验和 (正) | 同一条路，字节不动 | 装成功 —— 一道把什么都拒掉的门在负臂上同样绿 |
| 不支持的 CPU / OS | PATH 前面放一个假 `uname` | 报错并列出四个支持的 target，不静默改装 x86_64 |
| 没有 sha256 工具 | PATH 里只留 `sh` / `curl` / `tar` 等，抽掉 `sha256sum` / `shasum` / `openssl` | 失败退出，不是「跳过校验」后装上 |
| 幂等 | 连装两次 | rc 里只有一个 `# >>> limae >>>` 块 |
| chezmoi | 给该 HOME 配一份 `chezmoi.toml`，让 rc 进 chezmoi 源 | 不写 rc，改为打印要加的那行；把 `chezmoi` 从 PATH 上拿掉再跑，同一个 HOME 会写进去 |

改一个字节那两臂要控制服务的字节，所以是把 tarball 与边车放在本地 HTTP 服务上、用一份只改了传输的脚本副本跑 (`--proto '=https' --tlsv1.2` 与 `base` 两行)，校验逻辑本身一个字节没动。2026-09-10 在 dev-oregon 上全部跑过，读数与上表一致。

### Homebrew formula

渲染与它自己的负臂可以在 Linux 上跑：

```sh
tools/render_homebrew_formula.sh v0.13.2 <装着 .sha256 边车的目录> /tmp/limae.rb
```

退出 0 并打印 `rendered …`，即四个 digest 都读到了、四条 URL 都带上了这个 tag。负臂三条：tag 不是 `v<x>.<y>.<z>`、边车缺一份、边车里不是 digest，各自报错退出 1。

**`brew audit` / `brew install` / `brew test` 在本地一条都跑不了** —— 开发机是 Linux 且没有 Homebrew (2026-09-10 实测 `command -v brew` 无输出)，连 `ruby -c` 这样的语法检查都做不了，机器上没有 ruby。所以「formula 文件渲染出来了」这个观察对「formula 能装」与「语法对但装不起来」给出相同输出，不能拿它当验收。真正的验收在 release workflow 的 `homebrew` job 里：它跑在 macOS runner 上，把 formula 放进一个本地 tap，依次 `brew audit --strict`、`brew install`、`brew test`，全过之后才推 tap，而且这一串在 `workflow_dispatch` 上对着一个已有 tag 就能整条跑 —— 推 tap 那一步才是唯一只在 tag push 上执行的。**第一次 dispatch 就是这三条命令的第一次运行**，在那之前它们的读数是空的。
