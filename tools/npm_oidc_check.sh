#!/usr/bin/env bash
# Verify the npm Trusted Publishing configuration of every package this
# repository publishes, without publishing anything.
#
#   tools/npm_oidc_check.sh <path to the launcher package.json>
#
# npm has no `whoami` equivalent for OIDC: the exchange is wired into
# `npm publish` alone (npm 11.19.0, `lib/commands/publish.js` is the only
# file that requires `lib/utils/oidc.js`), and `npm publish --dry-run`
# cannot answer the question either — `oidc()` is documented in its own
# source as "intended to never throw" and returns `undefined` on every
# failure, so the dry run exits 0 whether the exchange worked or not.
#
# What this script does instead is the exchange itself, the same two
# requests the CLI makes: ask GitHub for an OIDC token with the audience
# `npm:<registry host>`, then POST it to the registry's per-package
# exchange endpoint and require a short-lived npm token back. A trusted
# publisher that does not exist, or whose organization / repository /
# workflow filename does not match this run, cannot answer with one.
#
# The token coming back is the verdict, not the status code that carries
# it: npm answers a successful exchange with 201 Created, and an answer
# with no token in it is a failure at any status.
#
# The endpoint is npm's own, read out of the CLI rather than out of the
# documentation, so it is not a published API and can move. It moving
# fails this check while publishing still works — a false negative, which
# is the direction that gets looked at rather than the one that gets
# believed.
#
# What a green run does NOT prove: that the `npm publish` action is
# permitted. A configuration saved with only `npm stage publish` ticked
# is the one trap of the npmjs.com setup (docs/knowledge/release.md), and
# whether the exchange response distinguishes the two is unknown — which
# is why the response's key names, values never, go into the log.
#
# Credentials: no token is printed, and none reaches a command line —
# both Authorization headers are handed to curl out of files (`-H @file`)
# inside a 0700 temporary directory that is removed when the script exits,
# so `ps` on a shared runner shows none of it. The npm token is
# short-lived and npm publishes no revocation endpoint, so it is left to
# expire.

set -euo pipefail

registry_host=registry.npmjs.org

fail() {
  printf 'npm_oidc_check: %s\n' "$1" >&2
  exit 1
}

[[ $# -eq 1 ]] || fail 'usage: tools/npm_oidc_check.sh <path to the launcher package.json>'
manifest=$1
[[ -f $manifest ]] || fail "$manifest does not exist"

# The set of packages is read off the launcher manifest rather than listed
# here: it is the one that tools/build_launchers.sh packs, so a target
# added there is checked here without a second edit. Every name is
# asserted to be ours — a manifest this script cannot recognise stops it
# rather than quietly checking four packages out of five.
names=()
while IFS= read -r name; do names+=("$name"); done < <(
  jq -er '[.name] + (.optionalDependencies // {} | keys) | .[]' "$manifest"
)
[[ ${#names[@]} -ge 2 ]] || fail "$manifest names ${#names[@]} package(s); expected the launcher and its platform packages"
for name in "${names[@]}"; do
  [[ $name == @limae/* ]] || fail "$manifest names $name, which is not an @limae/* package"
done
printf 'npm_oidc_check: %s package(s) to check: %s\n' "${#names[@]}" "${names[*]}"

[[ -n ${ACTIONS_ID_TOKEN_REQUEST_URL:-} ]] ||
  fail 'ACTIONS_ID_TOKEN_REQUEST_URL is unset; this job needs `permissions: id-token: write`'
[[ -n ${ACTIONS_ID_TOKEN_REQUEST_TOKEN:-} ]] ||
  fail 'ACTIONS_ID_TOKEN_REQUEST_TOKEN is unset; this job needs `permissions: id-token: write`'

work=$(mktemp -d "${TMPDIR:-/tmp}/limae-npm-oidc.XXXXXX")
trap 'rm -rf "$work"' EXIT

# GitHub's token endpoint already carries a query string, so the audience
# is appended. The colon is percent-encoded here because the value is a
# fixed literal; nothing about it is derived at run time.
printf 'Authorization: Bearer %s\n' "$ACTIONS_ID_TOKEN_REQUEST_TOKEN" >"$work/gh.header"
status=$(curl -sS -o "$work/id.json" -w '%{http_code}' \
  -H @"$work/gh.header" \
  -H 'Accept: application/json' \
  "${ACTIONS_ID_TOKEN_REQUEST_URL}&audience=npm%3A${registry_host}")
[[ $status == 200 ]] || fail "GitHub refused an OIDC token with the npm audience: HTTP $status"
# The id_token goes from the response straight into the header file: it is
# never a shell variable, so it cannot end up in an argument list or in
# `set -x` output. `jq -e` is the assertion that there was one.
jq -e '(.value | type) == "string" and (.value | length) > 0' "$work/id.json" >/dev/null ||
  fail 'GitHub returned no id_token value'
jq -r '"Authorization: Bearer " + .value' "$work/id.json" >"$work/npm.header"
printf 'npm_oidc_check: got a GitHub OIDC token for audience npm:%s\n' "$registry_host"

# Every package is checked before the script gives its verdict: one run
# should name all the misconfigured ones, not the first of them.
failed=()
for name in "${names[@]}"; do
  # `@limae/cli` -> `@limae%2fcli`, the escaping npm-package-arg does.
  escaped=${name/\//%2f}
  status=$(curl -sS -o "$work/exchange.json" -w '%{http_code}' -X POST \
    -H @"$work/npm.header" \
    "https://${registry_host}/-/npm/v1/oidc/token/exchange/package/${escaped}")
  keys=$(jq -r 'if type == "object" then (keys | join(",")) else type end' "$work/exchange.json" 2>/dev/null || echo 'unparseable')
  # The status code is reported, not judged: npm answers this endpoint with
  # 201 Created (run 34568232100, 2026-09-11, all five packages), and a check
  # spelled `== 200` turned that into a red run while a token was sitting in
  # the response. Which 2xx the registry picks is its business; what this
  # script asked for is a token, so having one is the whole judgement — an
  # answer without one fails whatever its status, 404 and 401 included.
  # Values never leave this line: `jq -e` reports the shape through its exit
  # code.
  if jq -e '(.token | type) == "string" and (.token | length) > 0' "$work/exchange.json" >/dev/null 2>&1; then
    printf '%s: exchange ok, HTTP %s, token issued, keys [%s]\n' "$name" "$status" "$keys"
  else
    message=$(jq -r '.message // empty' "$work/exchange.json" 2>/dev/null | head -c 200 || true)
    printf '%s: no token in the response, HTTP %s, keys [%s]%s\n' \
      "$name" "$status" "$keys" "${message:+, message: $message}" >&2
    failed+=("$name")
  fi
done

if [[ ${#failed[@]} -gt 0 ]]; then
  fail "no trusted publisher answered for: ${failed[*]}"
fi
printf 'npm_oidc_check: all %s package(s) exchanged a token\n' "${#names[@]}"
