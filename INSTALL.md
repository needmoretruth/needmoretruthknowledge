# Installing nmtk

[한국어](INSTALL.ko.md)

Paste this into a terminal on Fedora or Ubuntu. It installs what nmtk needs and starts it.

```sh
git clone https://github.com/needmoretruth/needmoretruthknowledge.git && cd needmoretruthknowledge && ./install.sh
```

That is the whole thing. You will not be asked anything, with one exception: if your system has no
C compiler, or nothing to download Rust with, `sudo` may ask for your password.

## What that line does

1. Copies this repository into a folder called `needmoretruthknowledge`.
2. Checks for a C linker (`cc`), which Rust needs. If there is none, it installs one:
   `build-essential` through `apt-get` on Ubuntu and Debian, `gcc` through `dnf` on Fedora.
   This and `curl` (step 3) are the only things it may install system-wide, and the only steps
   that need root.
3. Makes sure you have Rust 1.98.1, the version this repository is built with. If you have
   `rustup`, it fetches that version through it. If you have no Rust at all, it installs `rustup`
   and Rust 1.98.1 into `~/.rustup` and `~/.cargo` (or wherever `RUSTUP_HOME` and `CARGO_HOME`
   point, if you set them), and nowhere else. It downloads with `curl`, or `wget` if there is no
   `curl`; with neither, it first installs `curl` the same way as the linker.
4. Builds nmtk. About a minute the first time, using every core you have.
5. Puts the program at `~/.local/bin/nmtk`.
6. Starts it — but only when it runs in a terminal. Piped or in CI, it prints how to start nmtk
   instead.

Afterwards, `nmtk` starts it again. If your shell says `command not found`, `~/.local/bin` is not on
your `PATH`; the installer prints the one line that fixes that.

Running it again is safe. It skips what is already there.

## Options

| | |
|---|---|
| `./install.sh --no-run` | build and install, but do not start nmtk |
| `NMTK_NO_RUN=1 ./install.sh` | the same, as an environment variable |
| `./install.sh --help` | print what the script does, and stop |

## Supported systems

| | |
|---|---|
| **Fedora** | works; installs `gcc`, and `curl`, if they are missing |
| **Ubuntu** | works; installs `build-essential`, and `curl`, if they are missing |
| Other Linux | should work if you already have `cc`, and `curl` or `wget` — nmtk links nothing outside Rust |
| macOS, Windows | not yet |

You need a terminal at least **80 columns by 24 rows**, and a font with Korean glyphs if you want to
read it in Korean (most terminal fonts have them; Fedora and Ubuntu ship them by default).

## If you would rather do it by hand

```sh
git clone https://github.com/needmoretruth/needmoretruthknowledge.git
cd needmoretruthknowledge
cargo build --release --locked
./target/release/nmtk
```

Or to put it on your `PATH` with cargo's own installer:

```sh
cargo install --locked --path crates/nmtk
nmtk
```

## When something goes wrong

**`error: linker 'cc' not found`** — Rust needs a linker. The installer tries to add one; if it
could not, run `sudo dnf install -y gcc` on Fedora, or
`sudo apt-get update && sudo apt-get install -y build-essential` on Ubuntu. Then run `./install.sh`
again.

**`installing it needs root. There is no sudo here`** — log in as root, run the command the
installer printed, then run `./install.sh` again as yourself.

**`Installing a C linker (cc) did not work`** or **`Installing curl did not work`** — `sudo` refused,
it was run without a terminal and could not ask for a password, or the package manager failed. Run
the command the installer printed yourself. On Ubuntu a failed `apt-get update` only warns
(`apt-get update reported a problem; trying the install anyway`); if the install fails after that,
a broken package source is the usual cause.

**`Could not download rustup`** — the download from `https://sh.rustup.rs` failed, usually because
there is no network. Check your connection and run `./install.sh` again.

**The build fails and says it needs a newer `rustc`** — this repository pins Rust 1.98.1 in
`rust-toolchain.toml`. `rustup` reads that file; a Rust from your package manager does not. Install
`rustup` from [rustup.rs](https://rustup.rs), or run `rustup update` if you already have it. Then run
`./install.sh` again.

**`cargo: command not found` after installing** — rustup puts cargo in `~/.cargo/bin` (or
`$CARGO_HOME/bin`), and only a new shell sees it there. Open a new terminal, or run
`. ~/.cargo/env`.

**`nmtk: command not found`** — `~/.local/bin` is not on your `PATH`. Run the `echo 'export PATH=…'`
line the installer printed, then open a new terminal. Or start it by its full name:
`~/.local/bin/nmtk`.

**The build is killed part way through** — the machine ran out of memory. Build with one job at a
time: `cargo build --release --locked -j 1`.

**The screen is a mess of boxes or question marks** — your terminal font has no box-drawing or
Korean glyphs. Any of DejaVu Sans Mono, Noto Sans Mono or JetBrains Mono will do.

**`This terminal is 60x20. nmtk needs 80x24`** or **`This screen needs 80x24. Make the terminal
larger.`** — make the window bigger, or reduce the font size. The installer only warns; nmtk shows
that message until the window is big enough, then carries on from where it was.

## Removing it

```sh
rm ~/.local/bin/nmtk                 # the program
rm -rf ~/.config/nmtk                # your settings
rm -rf /path/to/needmoretruthknowledge   # the source
```

If you set `XDG_CONFIG_HOME`, your settings are in `$XDG_CONFIG_HOME/nmtk` instead.

Rust, if the installer put it there for you, lives in `~/.rustup` and `~/.cargo` (or `RUSTUP_HOME`
and `CARGO_HOME`) and is removed with `rustup self uninstall`. The C linker and `curl`, if they
were installed, are ordinary system packages; leave them, or remove them with `dnf` or `apt-get`
as usual.

nmtk writes nothing else anywhere, and it never touches the network.
