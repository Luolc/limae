# limae

[limae](https://github.com/Luolc/limae) 的 PyPI 启动器 (launcher)：`pip install limae` 或 `uv add --dev limae` 之后 `limae` 命令即可用。

这个 wheel 不含任何 Python 代码，只把预构建的 `limae` 二进制装进 scripts 目录。那个二进制与同版本 GitHub Release 上 `limae-<target>.tar.gz` 里的是同一份文件，打包时逐个比对过 sha256。

支持的平台：Linux x86_64 / aarch64 (静态 musl 二进制，manylinux 与 musllinux 两种 wheel 装的是同一份文件)、macOS x86_64 / arm64。没有 Windows。

用法、规则与配置见 [仓库 README](https://github.com/Luolc/limae#readme)。
