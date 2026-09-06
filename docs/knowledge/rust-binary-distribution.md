# Rust 分发预演

Rust 命令当前仍名为 `limae-rs`。它是迁移期入口，不取代 Python 的 `limae`，也不代表项目已进入正式发布阶段。本文覆盖 pre-commit 的 Rust opt-in，以及 Linux GNU / musl 的源码包与独立二进制预演；macOS、Windows、发布 tag、GitHub Release 与 `cargo publish` 均不在本轮验收范围内。

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

## 验收 pre-commit opt-in

Rust hook 使用独立 id `limae-rs`；既有 `limae` 仍是默认 Python hook。消费仓要试用 Rust 实现时，将 hook id 显式改为 `limae-rs`，pre-commit 会在自己的缓存中用 Cargo 安装 binary，不要求预装全局 `limae-rs`。迁移期不会在 Rust 安装失败时静默调用 Python。

1. 在 Linux 开发机的仓库根目录、提交待验收改动后执行：

   ```sh
   tools/check_rust_precommit.sh
   ```

   命令会从当前 `HEAD` 建立本地 Git 源副本，在全新消费仓与隔离的 `PRE_COMMIT_HOME` 中安装 Rust 和 Python 两个 hook。两条路径分别检查合成违规、执行修复，再复查修后 clean；脚本还会在原始 `PATH` 前放置失败探针，确认真实调用的是 pre-commit 缓存内安装的 executable。违规退出 1 且不写回，修复会写回并因 pre-commit 检出文件变化而退出 1，修后 clean 退出 0。命令退出 0 即完成；临时源码副本、消费仓和缓存会自动清理。

脚本不安装工具链或 target。本仓 CI 在 GitHub Actions 的临时 Linux runner 上运行 pre-commit 消费仓验收，再为钉住的 toolchain 安装 musl target，并逐项运行 package / GNU / musl 三条验收。实际发布前仍须由用户裁决许可证并完成发布认证；这些用户项不阻塞本文的无发布副作用预演。
