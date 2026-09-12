<div align="center">

```
███╗   ██╗███╗   ███╗████████╗██╗  ██╗
████╗  ██║████╗ ████║╚══██╔══╝██║ ██╔╝
██╔██╗ ██║██╔████╔██║   ██║   █████╔╝
██║╚██╗██║██║╚██╔╝██║   ██║   ██╔═██╗
██║ ╚████║██║ ╚═╝ ██║   ██║   ██║  ██╗
╚═╝  ╚═══╝╚═╝     ╚═╝   ╚═╝   ╚═╝  ╚═╝
```

### If it isn't fun, it doesn't stick.

**[nmtk.me](https://nmtk.me)** · [한국어](README.ko.md)

</div>

---

**nmtk** — *need more truth knowledge* — is a terminal program for learning how a few hard things
actually work, by running them on your own machine and changing them while they run.

Not an animation of mining. Your cores, hashing. Not a diagram of a 51% attack. An attacker you fund
with a share of your own machine, who really rewrites the chain and really takes the coin back.

Everything happens locally. nmtk opens no sockets, sends no telemetry, and needs no account.

## What you can run

### ⛏ Proof of work

Mine against Bitcoin's original difficulty and find out what your machine would have managed in
January 2009 — then set a practice difficulty and watch blocks arrive in seconds. Split your cores
between miners whose shares you choose, and run a **51% attack that actually succeeds**: the attacker
mines a private chain, the payment you already saw confirmed gets reverted, and the number of
confirmations it cost is on screen. Drop the attacker to 30% and watch it fail instead.

### 📒 Ledger models

The same coin, sent under three different sets of rules: **UTXO** (Bitcoin), **account-based**
(Ethereum) and **object-based** (Sui). Try to spend it twice in each and see three different reasons
for rejection. See which pairs of payments could have run at the same time — and where that
possibility disappears.

### 🧠 Transformer

A real transformer, written out by hand — attention, feed-forward, layer norm, and the backward pass
— with no machine-learning framework underneath. Train it here until it answers `nmtk` with
`need more truth knowledge`, watch the loss fall and the attention move, then change the depth, the
heads, the width and the learning rate and train it again.

### 🔒 Zero-knowledge proofs

Prove you know something without showing it. Walk the real lineage: an interactive sigma protocol,
the same proof made non-interactive, a system whose security rests on setup randomness being
destroyed — where an attacker holding that secret really does forge an accepted proof — and finally
**halo2**, which needs no such setup. Every stage is seen from four sides at once: sender, receiver,
onlooker, attacker.

## Install

nmtk builds from source. You need Rust 1.96 or newer; the repository pins the exact compiler, so
`rustup` will fetch the right one for you.

```sh
# Rust, if you do not have it yet
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

git clone https://github.com/needmoretruth/needmoretruthknowledge.git
cd needmoretruthknowledge
cargo build --release
./target/release/nmtk
```

To put it on your `PATH` instead:

```sh
cargo install --path crates/nmtk
nmtk
```

**Fedora** and **Ubuntu** need no other packages — nmtk links nothing outside Rust.

## Keys

| Key | What it does |
|---|---|
| `↑` `↓` or `j` `k` | move |
| `Enter` | open, or start the run |
| `Space` | pause and resume |
| `r` | reset the run |
| `Tab` | switch panel |
| `l` | English ⇄ 한국어 |
| `s` | settings |
| `?` | keys |
| `q` or `Esc` | back, and quit from the home screen |

A terminal of at least 80×24 is required.

## Language

English is the default and always complete. Korean is one key away (`l`), and anything not translated
yet stays in English rather than going blank.

## Settings

nmtk reads your machine at startup and sizes the work to it, so a four-core laptop is not handed a
run that takes an hour. You can override the thread count, the language and colour under `s`, and
your choices are kept in `~/.config/nmtk/settings.toml`.

## What nmtk will not do

- **No network.** Not for updates, not for telemetry, not for anything.
- **No account, no key, no wallet.** Nothing here touches real money or a real chain.
- **No hidden work.** Every number on screen came from something that ran on your machine.

## Contributing

Issues and pull requests are welcome. The rule that shapes the code: a subject is only finished when
a reader can change something and watch the result, so a screen that merely describes a mechanism is
not done yet.

## Licence

[Apache-2.0](LICENSE).
