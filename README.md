<div align="center">

```
███╗   ██╗███╗   ███╗████████╗██╗  ██╗
████╗  ██║████╗ ████║╚══██╔══╝██║ ██╔╝
██╔██╗ ██║██╔████╔██║   ██║   █████╔╝
██║╚██╗██║██║╚██╔╝██║   ██║   ██╔═██╗
██║ ╚████║██║ ╚═╝ ██║   ██║   ██║  ██╗
╚═╝  ╚═══╝╚═╝     ╚═╝   ╚═╝   ╚═╝  ╚═╝
```

### Study that isn't fun is labour. I hate labour.

**[nmtk.me](https://nmtk.me)** · [한국어](README.ko.md)

</div>

---

**nmtk** — *need more truth knowledge* — is a terminal program for learning how a few hard things
actually work, by running them on your own machine and changing them while they run.

Not an animation of mining. Your cores, hashing. Not a diagram of a 51% attack. An attacker you fund
with a share of your own machine, who really rewrites the chain and really takes the coin back.

Everything happens locally. nmtk opens no sockets, sends no telemetry, and needs no account.

## Knowledge Quests

A **Knowledge Quest (KQ)** is one finished piece of learning, and it talks rather than lectures.

You are told one thing, in one or two sentences. You press Enter. Something real happens on your
machine. Then a sentence about what just happened, and Enter again — the way a chat goes, not the
way a textbook does. Where the answer takes time, the conversation stops and waits: a block arrives,
a loss falls past a milestone, an attack gives up, and each of those says so as it happens.

Every quest has at least three stages and some have seven. Each declares its own difficulty, so a
quest can open very easy and end hard, and `Tab` walks between them.

| Stage kind | What happens |
|---|---|
| **Explain** | Why this is worth the time, one sentence at a time. |
| **Run** | The real thing runs. Nothing is decided in advance. |
| **Tune** | You change values — by arrow or by typing a number — and the run answers. |
| **Break** | You attack it and find out what holds. |
| **Recap** | What just happened, in your own numbers. |

Four quests ship today.

### ⛏ Proof of work · *Consensus · 40 min*

Mine real Bitcoin block headers — 80 bytes, double SHA-256, compared against a real target — at a
practice difficulty, so blocks arrive in seconds. Your measured hash rate sits beside what it would
mean at Bitcoin's difficulty 1, and beside the hash rate the protocol implies for early 2009. A
laptop today is worth several times the whole network of January 2009.

Then split your cores between miners whose shares you choose, and attack the chain. At 30% the
attacker falls behind, gives up, and the payment stands. Type `51` and run the same attack again:
this one is a **51% attack that actually succeeds** — the payment reaches the confirmations you set,
the merchant hands over the goods, and the attacker's private chain erases it.

### 📒 Ledger models · *Ledgers · 20 min*

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

Paste this into a terminal on Fedora or Ubuntu. It installs what nmtk needs and starts it, and asks
you nothing.

```sh
git clone https://github.com/needmoretruth/needmoretruthknowledge.git && cd needmoretruthknowledge && ./install.sh
```

Details, other ways to do it, and what to do when something goes wrong: **[INSTALL.md](INSTALL.md)**.

## Keys

The only key you need to start is `Enter`.

| Key | What it does |
|---|---|
| `Enter` | carry the conversation on, or accept a typed number |
| `Tab` | the next stage of the quest (`Shift+Tab` for the one before) |
| `↑` `↓` | choose a value to change |
| `←` `→` or `0`–`9` | change it, by arrow or by typing the number you want |
| `PgUp` `PgDn` | scroll back through what has been said |
| `Space` | pause and resume a run |
| `r` | start this stage over |
| `o` `f` `v` | on the shelf: sort, filter, older versions |
| `l` | pick a language |
| `s` | settings |
| `?` | every key, grouped by where it works |
| `q` or `Esc` | back, and quit from the shelf |

A terminal of at least 80×24 is required.

## Language

The first time you run nmtk it says what it is in four lines and asks you three things, the first of
which is the language. After that it never asks again, and `l` opens the list from anywhere.

English is the default and always complete. Anything not translated yet stays in English rather than
going blank, and no screen loses text in one language that it keeps in another.

## Settings

nmtk reads your machine at startup and sizes the work to it, so a four-core laptop is not handed a
run that takes an hour. Every quest states a **minimum** and a **recommended** machine, and the
shelf says where yours sits against both. A quest under the minimum still opens and sizes itself
down; it says so rather than refusing.

You can change the thread count, the language and colour under `s`, and your choices are kept in
`~/.config/nmtk/settings.toml`. That file also holds the ids of the quests you have finished, so
the list can mark them. It is the only thing nmtk records about what you did, and it never leaves
your machine.

Quests keep their version. From 0.1.1 on, a quest is never rewritten out from under you: when a
new version ships the old one stays in the program, and `v` on the shelf opens exactly the one you
learned from. The 0.1.0 quests were replaced rather than kept, because the format itself changed.

## What nmtk will not do

- **No network.** Not for updates, not for telemetry, not for anything.
- **No account, no key, no wallet.** Nothing here touches real money or a real chain.
- **No hidden work.** Every number on screen came from something that ran on your machine.

## Writing a quest

Quests are built against one standard so that four of them, written by different hands, feel like
one program: **[docs/KQ-STANDARD.md](docs/KQ-STANDARD.md)**. It covers the metadata a quest declares,
how a conversation is written, stages and difficulties, minimum and recommended machines, the screen,
the four state colours, the keys, and the rule that shapes all of it — a subject is only finished
when a reader can change something and watch the result.

Issues and pull requests are welcome.

## Licence

[Apache-2.0](LICENSE).
