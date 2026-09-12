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

## Knowledge Quests

A **Knowledge Quest (KQ)** is one finished piece of learning. Every quest walks the same five
stages, so once you have done one you know your way around all of them:

| | Stage | What happens |
|---|---|---|
| `1` | **Brief** | Why this is worth an hour. One screen, no jargon. |
| `2` | **Run** | The real thing runs. Numbers move. Nothing is decided in advance. |
| `3` | **Tune** | You change values and the run answers. |
| `4` | **Break** | You attack it and find out what holds. |
| `5` | **Recap** | What just happened, in the order it happened. |

Four ship today.

### ⛏ Proof of work · *Consensus · 40 min*

Mine real Bitcoin block headers — 80 bytes, double SHA-256, compared against a real target — at a
practice difficulty, so blocks arrive in seconds. Your measured hash rate sits beside what it would
mean at Bitcoin's difficulty 1, and beside the hash rate the protocol implies for early 2009. A
laptop today is worth several times the whole network of January 2009.

Then split your cores between miners whose shares you choose, and run a **51% attack that actually
succeeds**: a payment reaches the confirmations you set, the merchant hands over the goods, and the
attacker's private chain erases it. Drop the attacker to 30% and watch the same attack fall behind
and give up.

### 📒 Ledger models · *Ledgers · 25 min*

Send one coin under three sets of rules at once — **UTXO** (Bitcoin), **account-based** (Ethereum),
**object-based** (Sui) — and watch them disagree about everything except the balance. One transfer
grows the UTXO state by nothing, the account state by a whole new account, the object state by
nothing again.

Then spend the same coin twice. All three stop it, at three different steps, for three different
reasons: the coin is already spent, the counter has moved on, the object is at a newer version.

### 🧠 Transformer · *Machine learning · 45 min*

A real transformer, written out by hand — embeddings, causal attention, feed-forward, layer norm,
and the backward pass — with no machine-learning framework underneath. Around a hundred thousand
weights, trained here in half a minute, until asking it `nmtk` gets back
`need more truth knowledge`.

Watch the loss fall and the attention grid fill in. Then change the depth, the heads, the width and
the learning rate and train it again — and in **Break**, turn the learning rate up three thousand
times and watch the same model collapse into one repeated letter.

### 🔒 Zero-knowledge proofs · *Cryptography · 50 min*

Walk the real lineage, each system running here: an interactive **sigma protocol** (stepped through
message by message), the same proof made non-interactive with **Fiat-Shamir**, a system whose
soundness rests on **setup randomness being destroyed**, and **halo2**, which needs no such setup.
Proving time, verification time and proof size side by side — 96 bytes and a fraction of a
millisecond at one end, 1.5 KiB and tens of milliseconds at the other.

Then attack all four. A guessed response is rejected. A Fiat-Shamir challenge that skips the
commitment is forged and **accepted**. And the holder of a trusted setup's leftover randomness opens
a commitment at a value it does not hold — the unchanged verifier accepts that too, which is why
anyone ever asks whether a ceremony was honest.

Finally, the same shielded payment from four sides at once: sender, receiver, onlooker, attacker.

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
| `Enter` | open, go, or accept a typed number |
| `←` `→` | turn the chosen value |
| `1`–`5` | jump to a stage |
| `Space` | pause and resume |
| `r` | reset the run |
| `o` `f` `v` | on the shelf: sort, filter, older versions |
| `l` | English ⇄ 한국어 |
| `s` | settings |
| `?` | keys |
| `q` or `Esc` | back, and quit from the shelf |

A terminal of at least 80×24 is required.

## Language

English is the default and always complete. Korean is one key away (`l`), and anything not translated
yet stays in English rather than going blank.

## Settings

nmtk reads your machine at startup and sizes the work to it, so a four-core laptop is not handed a
run that takes an hour. You can override the thread count, the language and colour under `s`, and
your choices are kept in `~/.config/nmtk/settings.toml`.

Quests keep their version. A quest is never rewritten out from under you: when a new version ships
the old one stays in the program, and `v` on the shelf opens exactly the one you learned from.

## What nmtk will not do

- **No network.** Not for updates, not for telemetry, not for anything.
- **No account, no key, no wallet.** Nothing here touches real money or a real chain.
- **No hidden work.** Every number on screen came from something that ran on your machine.

## Writing a quest

Quests are built against one standard so that four of them, written by different hands, feel like
one program: **[docs/KQ-STANDARD.md](docs/KQ-STANDARD.md)**. It covers the metadata a quest declares,
the five stages, the screen, the four state colours, the keys, and the rule that shapes all of it — a
subject is only finished when a reader can change something and watch the result.

Issues and pull requests are welcome.

## Licence

[Apache-2.0](LICENSE).
