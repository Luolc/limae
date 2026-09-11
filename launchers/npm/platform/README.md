# @limae/&lt;platform&gt;

[limae](https://github.com/Luolc/limae) 的平台二进制包，由 `@limae/cli` 主包经 `optionalDependencies` 按平台自动选装。不要直接依赖它：装 `@limae/cli` 就好。

包里那个 `limae` 文件与同版本 GitHub Release 上 `limae-<target>.tar.gz` 里的是同一份，打包时逐个比对过 sha256。
