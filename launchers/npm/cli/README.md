# @limae/cli

The npm launcher for [limae](https://github.com/Luolc/limae): `npm i -D @limae/cli`, then run `npx limae` (the package is scoped, the command is not).

This package carries no logic of its own. `bin/limae.js` finds the `@limae/<platform>` package that npm installed for the current machine and executes the binary inside it. That binary is the same file that ships as `limae-<target>.tar.gz` in the GitHub Release of the same version, checked sha256 by sha256 when the packages are built.

Platforms: `linux-x64`, `linux-arm64` (static musl builds, so they run on glibc and musl systems alike), `darwin-x64`, `darwin-arm64`. No Windows.

Usage, rules and configuration live in the [repository README](https://github.com/Luolc/limae#readme), which is written in Chinese.
