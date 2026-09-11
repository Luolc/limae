#!/bin/sh
# One-shot installer for the `limae` command.
#
#   curl -fsSL https://limae.luolc.com/install.sh | sh
#
# It downloads the Release binary for this machine, verifies its sha256
# against the sidecar published beside it, drops it in ~/.local/bin, and — if
# that directory is not on PATH yet — adds it to the shell rc file, so that
# "open a new terminal" is the whole of what is left to do.
#
# Three knobs, all optional and all documented in the README:
#
#   LIMAE_VERSION          a Release tag such as v0.13.2; default: the latest
#   LIMAE_INSTALL_DIR      where the binary lands; default: ~/.local/bin
#   LIMAE_NO_MODIFY_PATH   set to anything non-empty: print the line to add,
#                          write no file
#
# Everything up to the last line of this file is a definition, and `main` is
# called from that last line. A `curl | sh` cut off mid-transfer therefore
# defines some functions and exits, instead of running half an installation.
# rustup and ollama do this; deno, bun, dprint and starship do not, which is
# what makes it worth the three lines.
set -eu

REPO='Luolc/limae'
BIN_NAME='limae'

# Every target the Release actually carries, printed when detection fails.
# The message names what exists rather than guessing at a near miss: an
# installer that falls through to "probably x86_64 Linux" hands the user a
# binary that cannot run on their machine, and the error then arrives from
# the kernel rather than from here.
SUPPORTED_TARGETS='
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-musl
  x86_64-apple-darwin
  aarch64-apple-darwin'

# A paired marker, so this block can be found again on a second run and by a
# human reading their own rc file.
MARK_BEGIN='# >>> limae >>>'
MARK_END='# <<< limae <<<'

say() { printf '%s\n' "$*"; }

err() {
  printf 'install.sh: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || err "\`$1\` is required but was not found"
}

# Linux is always musl: the Release carries only the static musl builds, and
# those run on glibc systems too, so a libc probe would have nothing to choose
# between. Unknown OS or CPU is an error.
detect_target() {
  os="$(uname -s)"
  cpu="$(uname -m)"
  case "$os" in
    Linux) os_part='unknown-linux-musl' ;;
    Darwin) os_part='apple-darwin' ;;
    *) err "unsupported operating system \`$os\`. limae releases:$SUPPORTED_TARGETS" ;;
  esac
  case "$cpu" in
    x86_64 | amd64) cpu_part='x86_64' ;;
    aarch64 | arm64) cpu_part='aarch64' ;;
    *) err "unsupported CPU \`$cpu\`. limae releases:$SUPPORTED_TARGETS" ;;
  esac
  printf '%s-%s\n' "$cpu_part" "$os_part"
}

# sha256sum (GNU), shasum (macOS), openssl (either). None of the three present
# is a failure and not a skip: a skipped checksum and a passing one leave the
# same silence behind, and "it installed fine" would then mean nothing.
file_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$1" | tr -d '\r' | sed 's/.*= *//'
  else
    err 'found none of sha256sum, shasum or openssl, so the download cannot be verified; refusing to install unchecked bytes'
  fi
}

download() {
  curl -fsSL --proto '=https' --tlsv1.2 -o "$2" "$1" || err "download failed: $1"
}

# `/releases/latest` redirects to `/releases/tag/<tag>`, so the tag comes out
# of the redirect without an API call, a token or a JSON parser. Resolving it
# up front (rather than downloading through `/latest/download/`) is what lets
# the run print which version it installed — `limae` itself has no
# `--version` flag to ask afterwards.
resolve_version() {
  url="$(curl -fsSL --proto '=https' -o /dev/null -w '%{url_effective}' \
    "https://github.com/$REPO/releases/latest")" ||
    err 'could not reach GitHub to look up the latest release'
  tag="${url##*/}"
  # Positive check on what came back. A redirect that landed anywhere else
  # would otherwise be pasted into a download URL as if it were a tag.
  case "$tag" in
    v[0-9]*) ;;
    *) err "no version tag in \`$url\`; set LIMAE_VERSION to a tag such as v0.13.2" ;;
  esac
  printf '%s\n' "$tag"
}

# The line to put on PATH, written against $HOME when the directory is under
# it, so the same line is portable to another machine and reads well in a
# dotfiles repository.
# shellcheck disable=SC2016  # $HOME and $PATH are meant to stay literal:
# this writes a line into someone's rc file, where they are expanded.
path_line() {
  case "$1" in
    "$HOME"/*) printf 'export PATH="$HOME/%s:$PATH"\n' "${1#"$HOME"/}" ;;
    *) printf 'export PATH="%s:$PATH"\n' "$1" ;;
  esac
}

# What to tell someone whose file this script will not write.
manual_line() {
  case "${SHELL:-}" in
    */fish) printf 'fish_add_path %s\n' "$1" ;;
    *) path_line "$1" ;;
  esac
}

# Only the two shells whose rc syntax this script is sure of get a file
# written; everything else gets the line printed. A wrong `export` line in a
# fish config is worse than a correct one copied by hand.
rc_file() {
  case "${SHELL:-}" in
    */zsh) printf '%s\n' "${ZDOTDIR:-$HOME}/.zshrc" ;;
    */bash)
      if [ "$(uname -s)" = Darwin ]; then
        # A new terminal on macOS is a login shell, which reads exactly one
        # of these, in this order. Creating .bash_profile while .profile is
        # the file in use would shadow it — so an existing file wins, and
        # .bash_profile is only created when there is nothing at all.
        for candidate in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile"; do
          if [ -f "$candidate" ]; then
            printf '%s\n' "$candidate"
            return 0
          fi
        done
        printf '%s\n' "$HOME/.bash_profile"
      else
        # On Linux a new terminal is an interactive non-login shell.
        printf '%s\n' "$HOME/.bashrc"
      fi
      ;;
    *) return 1 ;;
  esac
}

# chezmoi renders this file from a source repository. A line appended here
# survives until the next `chezmoi apply` and then silently disappears, which
# reads to the user as "I installed it and now it is gone". So the line gets
# printed for them to put in the source instead. A chezmoi that errors for
# any other reason counts as unmanaged, which is the direction that keeps the
# common case working.
chezmoi_managed() {
  command -v chezmoi >/dev/null 2>&1 || return 1
  chezmoi source-path "$1" >/dev/null 2>&1
}

# Prints the line and why it was not written, then the two ways to use it.
print_manual() {
  manual="$(manual_line "$1")"
  export_line="$(path_line "$1")"
  say ''
  say "$2"
  say ''
  say "  $manual"
  say ''
  # Only fish gets a different line for the two purposes; for everyone else
  # printing the same `export` twice under two headings would just look like
  # two things to do.
  if [ "$manual" = "$export_line" ]; then
    say 'Running that same line here makes limae available in this terminal too.'
  else
    say 'This terminal, to use limae right away:'
    say ''
    say "  $export_line"
  fi
}

setup_path() {
  bin_dir="$1"

  case ":${PATH:-}:" in
    *":$bin_dir:"*)
      say ''
      say "$bin_dir is already on PATH. Run \`limae --help\` to start."
      return 0
      ;;
  esac

  if [ -n "${LIMAE_NO_MODIFY_PATH:-}" ]; then
    print_manual "$bin_dir" 'LIMAE_NO_MODIFY_PATH is set, so no file was touched. Add this line yourself:'
    return 0
  fi

  if ! rc="$(rc_file)"; then
    print_manual "$bin_dir" "\$SHELL is \`${SHELL:-unset}\`, whose rc syntax this script does not write. Add this line yourself:"
    return 0
  fi

  if chezmoi_managed "$rc"; then
    print_manual "$bin_dir" "$rc is managed by chezmoi, and a line appended here would disappear at the next \`chezmoi apply\`. Put this line in the chezmoi source instead:"
    return 0
  fi

  if [ -e "$rc" ] && grep -Fq "$MARK_BEGIN" "$rc"; then
    say ''
    say "$rc already carries a limae block; it was left as it is."
  else
    {
      printf '\n%s\n' "$MARK_BEGIN"
      path_line "$bin_dir"
      printf '%s\n' "$MARK_END"
    } >>"$rc" || err "could not write $rc"
    say ''
    say "Added $bin_dir to PATH in $rc."
  fi

  say ''
  say 'New terminals: nothing left to do — open one.'
  say 'This terminal:'
  say ''
  say "  $(path_line "$bin_dir")"
}

main() {
  # Configuration goes through the environment, so an argument here is a
  # misunderstanding rather than something to interpret.
  [ "$#" -eq 0 ] ||
    err "unexpected argument \`$1\`; this script is configured through LIMAE_VERSION, LIMAE_INSTALL_DIR and LIMAE_NO_MODIFY_PATH"

  need_cmd curl
  need_cmd tar
  need_cmd uname
  need_cmd mktemp

  bin_dir="${LIMAE_INSTALL_DIR:-$HOME/.local/bin}"
  target="$(detect_target)"
  version="${LIMAE_VERSION:-$(resolve_version)}"
  tarball="$BIN_NAME-$target.tar.gz"
  sidecar="$BIN_NAME-$target.sha256"
  base="https://github.com/$REPO/releases/download/$version"

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT INT TERM

  say "limae $version ($target) -> $bin_dir"

  download "$base/$tarball" "$tmp/$tarball"
  download "$base/$sidecar" "$tmp/$sidecar"

  # The sidecar is `<sha256>  <filename>`. The digest is compared here rather
  # than by `sha256sum -c`, whose implementations disagree about the filename
  # field — and the file it names is not the name it has on disk anyway.
  expected="$(cut -d' ' -f1 <"$tmp/$sidecar")"
  [ "${#expected}" -eq 64 ] || err "$sidecar does not contain a sha256 digest"
  actual="$(file_sha256 "$tmp/$tarball")"
  [ "$expected" = "$actual" ] ||
    err "checksum mismatch on $tarball (expected $expected, got $actual). Nothing was installed."
  say "checksum ok: $expected"

  tar -xzf "$tmp/$tarball" -C "$tmp"
  [ -f "$tmp/$BIN_NAME" ] || err "$tarball did not contain \`$BIN_NAME\`"

  mkdir -p "$bin_dir" || err "could not create $bin_dir"
  # Copy next to the target and rename: a rename within one directory is
  # atomic, so a limae that is running right now is replaced rather than
  # overwritten halfway through.
  staged="$bin_dir/.$BIN_NAME.$$"
  cp "$tmp/$BIN_NAME" "$staged" || err "could not write to $bin_dir"
  chmod 755 "$staged"
  mv -f "$staged" "$bin_dir/$BIN_NAME"

  # What landed has to run here. A tarball built for another CPU gets all the
  # way to this line and fails it with an exec format error — that is the
  # failure the detection above exists to prevent, so it is the one worth
  # confirming rather than assuming. It says nothing about whether limae's
  # rules are right; the test suite is what covers that.
  "$bin_dir/$BIN_NAME" --help >/dev/null ||
    err "$bin_dir/$BIN_NAME was installed but does not run on this machine"
  say "installed $bin_dir/$BIN_NAME"

  setup_path "$bin_dir"
}

main "$@"
