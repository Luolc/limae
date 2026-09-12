# limae

The PyPI launcher for [limae](https://github.com/Luolc/limae): `pip install limae` or `uv add --dev limae`, then run `limae`.

The wheel carries no Python code. It drops a prebuilt `limae` binary into the scripts directory, and that binary is the same file that ships as `limae-<target>.tar.gz` in the GitHub Release of the same version, verified against its SHA-256 checksum when the wheels are built.

Platforms: Linux x86_64 and aarch64 (static musl builds — the manylinux and musllinux wheels carry the same file), macOS x86_64 and arm64. No Windows.

Usage, rules and configuration live in the repository README. The [English summary](https://github.com/Luolc/limae/blob/main/README.en.md) covers installation and the basic commands; the [full documentation](https://github.com/Luolc/limae#readme) is in Chinese.
