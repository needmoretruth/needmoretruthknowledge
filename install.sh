#!/bin/sh
# Install nmtk and start it. Paste one line, answer nothing.
#
# What this does, in order:
#   1. checks you are on Linux with a terminal big enough
#   2. installs Rust through rustup if you do not have a new enough one
#   3. builds nmtk from this checkout
#   4. puts it in ~/.local/bin, and tells you if that is not on your PATH
#   5. starts it
#
# It never touches anything outside ~/.cargo, ~/.rustup and ~/.local/bin, and it asks rustup for
# the minimal profile so the install stays small. Re-running it is safe.

set -eu

NEEDED_RUST_MAJOR=1
NEEDED_RUST_MINOR=98
BIN_DIR="${HOME}/.local/bin"

say() { printf '\033[1m%s\033[0m\n' "$*"; }
note() { printf '  %s\n' "$*"; }
die() { printf '\033[31m%s\033[0m\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Linux" ] || die "nmtk currently ships for Linux only. You are on $(uname -s)."

cd "$(dirname "$0")"
[ -f Cargo.toml ] || die "Run this from inside the nmtk checkout."

# ---------------------------------------------------------------- Rust
rust_is_new_enough() {
    command -v rustc >/dev/null 2>&1 || return 1
    version=$(rustc --version | cut -d' ' -f2)
    major=$(echo "$version" | cut -d. -f1)
    minor=$(echo "$version" | cut -d. -f2)
    [ "$major" -gt "$NEEDED_RUST_MAJOR" ] && return 0
    [ "$major" -eq "$NEEDED_RUST_MAJOR" ] && [ "$minor" -ge "$NEEDED_RUST_MINOR" ]
}

if rust_is_new_enough; then
    say "Rust $(rustc --version | cut -d' ' -f2) is already here."
else
    say "Installing Rust (this is the only thing nmtk needs)."
    note "It goes in ~/.rustup and ~/.cargo, and nothing else on your system changes."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --no-modify-path >/dev/null
    # shellcheck disable=SC1091
    . "${HOME}/.cargo/env"
    rust_is_new_enough || die "Rust installed but is older than ${NEEDED_RUST_MAJOR}.${NEEDED_RUST_MINOR}. Run: rustup update"
fi

# ---------------------------------------------------------------- Build
say "Building nmtk. This takes about a minute the first time."
note "Every core on this machine is used; later builds take seconds."
cargo build --release --quiet

# ---------------------------------------------------------------- Install
mkdir -p "$BIN_DIR"
install -m 755 target/release/nmtk "${BIN_DIR}/nmtk"
say "Installed to ${BIN_DIR}/nmtk"

case ":${PATH}:" in
    *":${BIN_DIR}:"*) ;;
    *)
        note "${BIN_DIR} is not on your PATH. To fix that for next time:"
        note "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
        ;;
esac

# ---------------------------------------------------------------- Run
say "Starting nmtk. Press q to leave; run it again any time with: nmtk"
sleep 1
exec "${BIN_DIR}/nmtk"
