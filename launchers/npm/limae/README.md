# limae

[limae](https://github.com/Luolc/limae) 的 npm 启动器 (launcher)：`npm i -D limae` 之后 `npx limae` 即可用。

这个包不含任何代码逻辑，`bin/limae.js` 只是找到 npm 按平台装进来的 `@limae/<platform>` 包、执行里面的二进制。那个二进制与同版本 GitHub Release 上 `limae-<target>.tar.gz` 里的是同一份文件，打包时逐个比对过 sha256。

支持的平台：`linux-x64`、`linux-arm64` (静态 musl 二进制，glibc 与 musl 系统都能跑)、`darwin-x64`、`darwin-arm64`。没有 Windows。

用法、规则与配置见 [仓库 README](https://github.com/Luolc/limae#readme)。
