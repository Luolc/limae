# Rust 分发预演

Rust 命令名为 `limae`，是本项目唯一的 CLI (ADR-0015 阶段 E，Python 参考实现已于 2026-09-10 删除)。本文覆盖 pre-commit 的 hook id，以及 Linux GNU / musl 的源码包与独立二进制预演；macOS 与 Windows 不在本地预演范围内。发布 tag、GitHub Release 与 `cargo publish` 见 [发布手册](release.md)。

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

唯一的 id `limae` 是 Rust hook (`language: rust`)。[pre-commit 的 Rust language 合同](https://pre-commit.com/#rust) 会在自己的缓存中用 Cargo 安装 binary，不要求预装全局 `limae`。本手册与验收脚本以机器已有可用 Rust 工具链为前提；首次安装需要联网取得源码与 Cargo 依赖。

过渡期的 opt-in id `limae-rs` 已随默认 id 切换删除，回退 id `limae-python` 已随 Python 参考实现删除 (2026-09-10)：留一个同义或已无实现的 id 只是第二个名字指同一件东西 (ADR-0013 对旧名别名的处置同理)。已经钉在旧 `rev` 上的消费仓不受影响 —— 它们装出来的仍是那个 `rev` 上的定义。

1. 在 Linux 开发机的仓库根目录、提交待验收改动后执行：

   ```sh
   tools/check_precommit_consumer.sh
   ```

   命令会从当前 `HEAD` 建立本地 Git 源副本，在全新消费仓与隔离的 `PRE_COMMIT_HOME` 中安装 `limae` hook，检查合成违规、执行修复，再复查修后 clean；脚本还会在原始 `PATH` 前放置一个同名失败探针 (`limae` 退 97)，所以任何一臂真跑起来了，跑的就一定是 pre-commit 缓存内装出来的那个。pre-commit 本身由 `uvx pre-commit@4.2.0` 现取，不需要装机步骤，仓里也没有 Python 项目。违规退出 1 且不写回，修复会写回并因 pre-commit 检出文件变化而退出 1，修后 clean 退出 0。命令退出 0 即完成；临时源码副本、消费仓和缓存会自动清理。

脚本不安装工具链或 target。本仓 CI 在 GitHub Actions 的临时 Linux runner 上运行 pre-commit 消费仓验收，再为钉住的 toolchain 安装 musl target，并逐项运行 package / GNU / musl 三条验收。许可证已由用户于 2026-09-07 裁决为 Apache-2.0，`LICENSE` 已在仓内，`Cargo.toml` 的 SPDX 值为 `Apache-2.0`；实际发布前仍须完成发布认证，该用户项不阻塞本文的无发布副作用预演。
