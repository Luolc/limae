# Rust 分发预演

Rust 命令名为 `limae`，是本项目正式的 CLI (ADR-0015 阶段 E)；Python 那份改名 `limae-python`、保留为 deprecated 的参考实现。本文覆盖 pre-commit 的两个 hook id，以及 Linux GNU / musl 的源码包与独立二进制预演；macOS、Windows、发布 tag、GitHub Release 与 `cargo publish` 均不在本轮验收范围内。

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

## 验收 pre-commit 的两个 hook id

默认 id `limae` 就是 Rust hook (`language: rust`)；Python 参考实现挪到 id `limae-python` (`language: python`)，它同时是**回退路径** —— 消费仓要退回参考实现，把 id 改成 `limae-python` 即可，不必改 `rev`。[pre-commit 的 Rust language 合同](https://pre-commit.com/#rust) 会在自己的缓存中用 Cargo 安装 binary，不要求预装全局 `limae`。本手册与验收脚本以机器已有可用 Rust 工具链为前提；首次安装需要联网取得源码与 Cargo 依赖。Rust 安装失败时不会静默回落到 Python。

过渡期的 opt-in id `limae-rs` 已随这次切换删除：默认 id 已经是 Rust，再留一个同义 id 只是第二个名字指同一件东西 (ADR-0013 对旧名别名的处置同理)。已经钉在旧 `rev` 上的消费仓不受影响 —— 它们装出来的仍是那个 `rev` 上的定义。

1. 在 Linux 开发机的仓库根目录、提交待验收改动后执行：

   ```sh
   tools/check_rust_precommit.sh
   ```

   命令会从当前 `HEAD` 建立本地 Git 源副本，在全新消费仓与隔离的 `PRE_COMMIT_HOME` 中安装 `limae` (Rust) 与 `limae-python` 两个 hook —— 后者跑的就是回退路径。两条路径分别检查合成违规、执行修复，再复查修后 clean；脚本还会在原始 `PATH` 前放置两个名字的失败探针 (`limae` 退 97、`limae-python` 退 98)，所以任何一臂真跑起来了，跑的就一定是 pre-commit 缓存内装出来的那个。违规退出 1 且不写回，修复会写回并因 pre-commit 检出文件变化而退出 1，修后 clean 退出 0。命令退出 0 即完成；临时源码副本、消费仓和缓存会自动清理。

脚本不安装工具链或 target。本仓 CI 在 GitHub Actions 的临时 Linux runner 上运行 pre-commit 消费仓验收，再为钉住的 toolchain 安装 musl target，并逐项运行 package / GNU / musl 三条验收。实际发布前仍须由用户裁决许可证并完成发布认证；这些用户项不阻塞本文的无发布副作用预演。
