#!/bin/sh
# Install nmtk and start it. Paste one line, answer nothing.
#
# What this does, in order:
#   1. checks you are on Linux
#   2. installs a C linker (cc) through apt-get or dnf if you do not have one
#   3. installs Rust through rustup if you do not have it, and the compiler this repository pins
#   4. builds nmtk from this checkout
#   5. puts it in ~/.local/bin, and tells you if that is not on your PATH
#   6. starts it, if this is a terminal and you did not ask it not to
#
# Options:
#   --no-run       build and install, but do not start nmtk (same as NMTK_NO_RUN=1)
#   -h, --help     print this and stop
#
# Outside ~/.cargo, ~/.rustup and ~/.local/bin it only installs a C linker, and curl, when they
# are missing. Re-running it is safe.

set -eu

BIN_DIR="${HOME}/.local/bin"
MIN_COLS=80
MIN_ROWS=24
NO_RUN="${NMTK_NO_RUN:-0}"
STAGE="start"
REPORTED=0

say() { printf '\033[1m%s\033[0m\n' "$*"; }
note() { printf '  %s\n' "$*"; }
warn() { printf '\033[33m%s\033[0m\n' "$*" >&2; }
die() {
    REPORTED=1
    printf '\033[31m%s\033[0m\n' "$*" >&2
    exit 1
}

on_exit() {
    status=$1
    [ "$status" -eq 0 ] && return 0
    [ "$REPORTED" -eq 1 ] && return 0
    printf '\033[31m%s\033[0m\n' "The installer stopped while it was at: ${STAGE}." >&2
    printf '%s\n' "What to do for the usual causes is in INSTALL.md, under \"When something goes wrong\"." >&2
}
trap 'on_exit $?' EXIT

usage() { sed -n '2,17s/^# \{0,1\}//p' "$0"; }

for arg in "$@"; do
    case "$arg" in
        --no-run) NO_RUN=1 ;;
        -h | --help) usage; exit 0 ;;
        *) die "Unknown option: ${arg}. Try: ./install.sh --help" ;;
    esac
done

[ "$(uname -s)" = "Linux" ] || die "nmtk currently ships for Linux only. You are on $(uname -s)."

cd "$(dirname "$0")"
[ -f Cargo.toml ] || die "Run this from inside the nmtk checkout."

# ---------------------------------------------------------------- System packages
# Which package manager this system uses: prints "apt", "dnf" or nothing.
package_family() {
    [ -r /etc/os-release ] || return 0
    ids=$(
        # shellcheck disable=SC1091
        . /etc/os-release
        printf '%s %s' "${ID:-}" "${ID_LIKE:-}"
    )
    for id in $ids; do
        case "$id" in
            debian | ubuntu) echo apt; return 0 ;;
            fedora | rhel | centos) echo dnf; return 0 ;;
        esac
    done
}

# The command that installs the given packages, as you would type it yourself.
install_command() {
    family=$1
    shift
    case "$family" in
        apt) echo "sudo apt-get update && sudo apt-get install -y $*" ;;
        dnf) echo "sudo dnf install -y $*" ;;
    esac
}

# Runs a command as root: directly when we are root, through sudo otherwise. Without a terminal,
# sudo is told not to ask for a password, so an unattended run fails instead of hanging.
as_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    elif [ -t 0 ]; then
        sudo "$@"
    else
        sudo -n "$@"
    fi
}

# Installs packages without asking. Dies with the exact command to run when it cannot.
install_packages() {
    what=$1
    shift
    family=$(package_family)
    if [ -z "$family" ]; then
        die "nmtk needs ${what}, and this installer only knows apt-get (Ubuntu, Debian) and dnf (Fedora).
Install ${what} with your package manager, then run ./install.sh again."
    fi
    if [ "$(id -u)" -ne 0 ] && ! command -v sudo >/dev/null 2>&1; then
        die "nmtk needs ${what}, and installing it needs root. There is no sudo here.
As root, run:
  $(install_command "$family" "$@" | sed 's/sudo //g')
then run ./install.sh again."
    fi
    say "Installing ${what} with ${family}."
    [ "$(id -u)" -eq 0 ] || note "sudo may ask for your password; this is the only step that needs it."
    ok=1
    case "$family" in
        apt)
            # One broken third-party source makes the update fail while every other source still
            # works, so a failed update is worth a warning, not the end of the install.
            as_root env DEBIAN_FRONTEND=noninteractive apt-get update -q >/dev/null ||
                warn "apt-get update reported a problem; trying the install anyway."
            as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y -q "$@" >/dev/null || ok=0
            ;;
        dnf)
            as_root dnf install -y -q "$@" >/dev/null || ok=0
            ;;
    esac
    [ "$ok" -eq 1 ] || die "Installing ${what} did not work. Run this yourself, then ./install.sh again:
  $(install_command "$family" "$@")"
}

have() { command -v "$1" >/dev/null 2>&1; }

STAGE="installing a C linker"
if ! have cc; then
    case "$(package_family)" in
        apt) install_packages "a C linker (cc)" build-essential ;;
        *) install_packages "a C linker (cc)" gcc ;;
    esac
    have cc || die "A C linker was installed, but there is still no 'cc' on your PATH."
fi

# ---------------------------------------------------------------- Rust
STAGE="installing Rust"

# The compiler this repository pins, e.g. 1.98.1.
# `|| true`, or a missing file ends the script here, under set -e, before the line below can say why.
PINNED=$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' rust-toolchain.toml 2>/dev/null || true)
[ -n "$PINNED" ] || die "rust-toolchain.toml is missing or has no channel. Is this checkout complete?"

# True when version $1 is at least version $2 (both x.y.z).
version_at_least() {
    [ "$(printf '%s\n%s\n' "$2" "$1" | sort -t. -k1,1n -k2,2n -k3,3n | head -n 1)" = "$2" ]
}

# Where rustup keeps cargo: $CARGO_HOME when it is set, ~/.cargo otherwise. rustup installs into
# $CARGO_HOME, so looking only in ~/.cargo found nothing there after a good install.
CARGO_DIR="${CARGO_HOME:-${HOME}/.cargo}"

# rustup puts cargo in its bin folder, but only a new shell sees it there.
use_cargo_dir() {
    if [ -f "${CARGO_DIR}/env" ]; then
        # shellcheck source=/dev/null
        . "${CARGO_DIR}/env"
    else
        PATH="${CARGO_DIR}/bin:${PATH}"
        export PATH
    fi
}

if ! have cargo && [ -d "${CARGO_DIR}/bin" ]; then
    use_cargo_dir
fi

download() {
    if have curl; then
        curl --proto '=https' --tlsv1.2 -sSf "$1"
    else
        wget -q --https-only -O - "$1"
    fi
}

install_rustup() {
    if ! have curl && ! have wget; then
        install_packages "curl" curl
    fi
    say "Installing Rust ${PINNED} through rustup."
    note "It goes in ~/.rustup and ~/.cargo, and nothing else on your system changes."
    script=$(mktemp)
    download https://sh.rustup.rs >"$script" || {
        rm -f "$script"
        die "Could not download rustup from https://sh.rustup.rs. Check your connection and run ./install.sh again."
    }
    sh "$script" -y --profile minimal --default-toolchain "$PINNED" \
        --component rustfmt,clippy --no-modify-path >/dev/null || {
        rm -f "$script"
        die "rustup did not install. See https://rustup.rs, then run ./install.sh again."
    }
    rm -f "$script"
    use_cargo_dir
}

if have rustup; then
    # rustup reads rust-toolchain.toml; fetch the pinned compiler now so the build does not stall.
    if ! rustup toolchain list 2>/dev/null | grep -q "^${PINNED}-"; then
        say "Installing Rust ${PINNED}, the version this repository is built with."
        rustup toolchain install "$PINNED" --profile minimal --component rustfmt,clippy >/dev/null ||
            die "rustup could not install Rust ${PINNED}. Try: rustup update, then ./install.sh again."
    fi
    say "Rust ${PINNED} is ready (through rustup)."
elif have cargo; then
    # A Rust without rustup ignores rust-toolchain.toml and builds with whatever it is.
    have_version=$(rustc --version 2>/dev/null | cut -d' ' -f2 | cut -d- -f1)
    if [ -n "$have_version" ] && version_at_least "$have_version" "$PINNED"; then
        say "Rust ${have_version} is already here, without rustup. nmtk is built with ${PINNED}; trying yours."
    else
        say "Your Rust (${have_version:-unknown}) is older than ${PINNED}, which nmtk needs."
        install_rustup
    fi
else
    install_rustup
fi

# ---------------------------------------------------------------- Build
STAGE="building nmtk"
say "Building nmtk. This takes about a minute the first time."
note "Every core on this machine is used; later builds take seconds."
if ! cargo build --release --locked --quiet; then
    REPORTED=1
    printf '\033[31m%s\033[0m\n' "The build failed. The compiler's message is above." >&2
    if ! have rustup; then
        printf '%s\n' "This Rust did not come from rustup, so it may not be the one nmtk is written for." \
            "Installing rustup usually fixes that: https://rustup.rs" >&2
    fi
    printf '%s\n' "INSTALL.md, under \"When something goes wrong\", covers the usual causes." >&2
    exit 1
fi

# ---------------------------------------------------------------- Install
STAGE="installing nmtk to ${BIN_DIR}"
mkdir -p "$BIN_DIR"
install -m 755 target/release/nmtk "${BIN_DIR}/nmtk"
say "Installed to ${BIN_DIR}/nmtk"

case ":${PATH}:" in
    *":${BIN_DIR}:"*) ;;
    *)
        case "${SHELL:-}" in
            */zsh) rc="${HOME}/.zshrc" ;;
            */bash) rc="${HOME}/.bashrc" ;;
            *) rc="${HOME}/.profile" ;;
        esac
        note "${BIN_DIR} is not on your PATH, so 'nmtk' alone will not find it yet. To fix that:"
        note "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ${rc}"
        note "then open a new terminal."
        ;;
esac

# ---------------------------------------------------------------- Run
STAGE="starting nmtk"
if [ "$NO_RUN" = "1" ]; then
    say "Done. Start it with: ${BIN_DIR}/nmtk"
    exit 0
fi

if [ ! -t 0 ] || [ ! -t 1 ]; then
    say "Done. This is not a terminal, so nmtk was not started."
    note "Start it from a terminal with: ${BIN_DIR}/nmtk"
    exit 0
fi

size=$(stty size 2>/dev/null || true)
rows=${size% *}
cols=${size#* }
if [ -z "$size" ] && have tput; then
    rows=$(tput lines 2>/dev/null || true)
    cols=$(tput cols 2>/dev/null || true)
fi
case "${rows}${cols}" in
    '' | *[!0-9]*) ;;
    *)
        if [ "$cols" -lt "$MIN_COLS" ] || [ "$rows" -lt "$MIN_ROWS" ]; then
            warn "This terminal is ${cols}x${rows}. nmtk needs ${MIN_COLS}x${MIN_ROWS}; make the window bigger or the font smaller."
        fi
        ;;
esac

say "Starting nmtk. Press q to leave; run it again any time with: nmtk"
sleep 1
trap - EXIT
exec "${BIN_DIR}/nmtk"
