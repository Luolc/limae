#!/usr/bin/env node
// Thin launcher: runs the `limae` binary shipped by the platform package
// that npm selected through `optionalDependencies`, and nothing else. The
// binary is the same file attached to the GitHub Release for the same
// version (tools/check_launchers.sh asserts the sha256 at packaging time).
"use strict";

const { spawnSync } = require("node:child_process");

// process.platform-process.arch -> package. Linux ships one static musl
// binary that also runs on glibc systems, so there is no `-musl` variant.
// Windows is absent because no Windows binary is built; see README.md.
const PACKAGES = {
  "linux-x64": "@limae/linux-x64",
  "linux-arm64": "@limae/linux-arm64",
  "darwin-x64": "@limae/darwin-x64",
  "darwin-arm64": "@limae/darwin-arm64",
};

const key = `${process.platform}-${process.arch}`;
const pkg = PACKAGES[key];
if (pkg === undefined) {
  console.error(
    `limae: no prebuilt binary for ${key}; supported: ${Object.keys(PACKAGES).join(", ")}`,
  );
  process.exit(1);
}

let binary;
try {
  binary = require.resolve(`${pkg}/limae`);
} catch {
  console.error(
    `limae: ${pkg} is not installed; it is an optional dependency of limae, so ` +
      "reinstall without --omit=optional / --no-optional",
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
if (result.error !== undefined) {
  console.error(`limae: failed to run ${binary}: ${result.error.message}`);
  process.exit(1);
}
if (result.signal !== null) {
  process.kill(process.pid, result.signal);
}
process.exit(result.status);
