# Installing nmtk

[한국어](INSTALL.ko.md)

Paste this into a terminal. It installs everything nmtk needs and starts it.

```sh
git clone https://github.com/needmoretruth/needmoretruthknowledge.git && cd needmoretruthknowledge && ./install.sh
```

That is the whole thing. You will not be asked anything.

## What that line does

1. Copies this repository into a folder called `needmoretruthknowledge`.
2. Checks whether you have Rust 1.98 or newer. If you do not, it installs it through `rustup` —
   into `~/.rustup` and `~/.cargo`, and nowhere else.
3. Builds nmtk. About a minute the first time, using every core you have.
4. Puts the program at `~/.local/bin/nmtk`.
5. Starts it.

Afterwards, `nmtk` starts it again. If your shell says `command not found`, `~/.local/bin` is not on
your `PATH`; the installer prints the one line that fixes that.

## Supported systems

| | |
|---|---|
| **Fedora** | works, nothing else to install |
| **Ubuntu** | works, nothing else to install |
| Other Linux | should work — nmtk links nothing outside Rust |
| macOS, Windows | not yet |

You need a terminal at least **80 columns by 24 rows**, and a font with Korean glyphs if you want to
read it in Korean (most terminal fonts have them; Fedora and Ubuntu ship them by default).

## If you would rather do it by hand

```sh
git clone https://github.com/needmoretruth/needmoretruthknowledge.git
cd needmoretruthknowledge
cargo build --release
./target/release/nmtk
```

Or to put it on your `PATH` with cargo's own installer:

```sh
cargo install --path crates/nmtk
nmtk
```

## When something goes wrong

**`curl: command not found`** — install it: `sudo dnf install curl` on Fedora,
`sudo apt install curl` on Ubuntu.

**`error: linker 'cc' not found`** — Rust needs a linker:
`sudo dnf install gcc` on Fedora, `sudo apt install build-essential` on Ubuntu.

**The build is killed part way through** — the machine ran out of memory. Build with one job at a
time: `cargo build --release -j 1`.

**The screen is a mess of boxes or question marks** — your terminal font has no box-drawing or
Korean glyphs. Any of DejaVu Sans Mono, Noto Sans Mono or JetBrains Mono will do.

**`This screen needs 80x24`** — make the window bigger, or reduce the font size.

## Removing it

```sh
rm ~/.local/bin/nmtk                 # the program
rm -rf ~/.config/nmtk                # your settings
rm -rf /path/to/needmoretruthknowledge   # the source
```

Rust, if the installer put it there for you, lives in `~/.rustup` and `~/.cargo` and is removed with
`rustup self uninstall`.

nmtk writes nothing else anywhere, and it never touched the network.
