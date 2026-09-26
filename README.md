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
mean at Bitcoin's difficulty 1, and beside the hash rate the protocol implies for early 2009, and
the screen works out how many times that whole network your machine is worth.

Then split your cores between miners whose shares you choose, and attack the chain. At 30% the
attacker usually falls behind, gives up, and the payment stands. Type `51` and run the same attack
again: this one is a **51% attack that can actually succeed**, and usually does — the payment
reaches the confirmations you set, the merchant hands over the goods, and the attacker's private
chain erases it. It is a race, so either one can go the other way, and the quest says so when it
does. The attack keeps to the thread count you allowed, and the panel shows the share the attacker
really holds.

### 📒 Ledger models · *Ledgers · 20 min*

Send one coin under three sets of rules at once — **UTXO** (Bitcoin), **account-based** (Ethereum),
**object-based** (Sui) — and watch them disagree about everything except the balance. One whole
coin sent to someone new grows the UTXO state by nothing, the account state by a whole new account,
the object state by nothing again. Send part of a coin and the UTXO and object states grow too: the
coin is broken in two.

Then spend the same coin twice. All three stop it, for three different reasons: the coin is already
spent, the counter has moved on, the object is at a newer version. The UTXO rules notice when they
look the coin up; the other two when they check the transfer is current.

### 🧠 Transformer · *Machine learning · 45 min*

A real transformer, written out by hand — embeddings, causal attention, feed-forward, layer norm,
and the backward pass — with no machine-learning framework underneath. It is sized to your
machine: on eight cores it has about a hundred thousand weights and trains in under a minute, until
asking it `nmtk` gets back `need more truth knowledge`.

Watch the loss fall and the attention grid fill in. Then change the depth, the heads, the width and
the learning rate and train it again — and in **Break**, turn the learning rate up three thousand
times and watch the same model collapse into one character over and over, most often the space.

### 🔒 Zero-knowledge proofs · *Cryptography · 50 min*

Walk the real lineage, each system running here: an interactive **sigma protocol** (stepped through
message by message), the same proof made non-interactive with **Fiat-Shamir**, a system whose
soundness rests on **setup randomness being destroyed**, and **halo2**, which needs no such setup.
Proving time, verification time and proof size side by side — under a millisecond and under a
hundred bytes for the first three, about 1.5 KiB and tens of milliseconds to prove for halo2.

Then attack all four. A guessed response is rejected. A Fiat-Shamir challenge that skips the
commitment is forged and **accepted**. And the holder of a trusted setup's leftover randomness opens
a commitment of its own at a value it never committed to — the unchanged verifier accepts that too,
which is why anyone ever asks whether a ceremony was honest.

Finally, the same shielded payment from four sides at once: sender, receiver, onlooker, attacker.

## Install

Paste this into a terminal on Fedora or Ubuntu. It installs what nmtk needs, builds it and starts
it. It asks you nothing, except that `sudo` may want your password if a C compiler or `curl` has to
be installed.

```sh
git clone https://github.com/needmoretruth/needmoretruthknowledge.git && cd needmoretruthknowledge && ./install.sh
```

After that, `nmtk` starts it. It takes `--help` (`-h`) and `--version` (`-V`) and nothing else;
anything else prints a short usage and exits with status 2. It needs a terminal, and will not start
with its output piped or redirected.

Details, other ways to do it, and what to do when something goes wrong: **[INSTALL.md](INSTALL.md)**.

## Keys

The only key you need to start is `Enter`.

| Key | What it does |
|---|---|
| `Enter` | on the list, open a quest; in a quest, carry the conversation on — and, where the stage has values to set, run it with the ones on screen |
| `Tab` | the next stage of the quest (`Shift+Tab` for the one before) |
| `↑` `↓` | move through a list; in a quest, choose a value to change |
| `←` `→` or `0`–`9` | change it, by arrow or by typing the number you want; on a stage with no values, `1`–`9` go straight to that stage |
| `PgUp` `PgDn` | scroll back through what has been said |
| `Space` | pause and resume a run |
| `r` | start this stage over |
| `o` `f` | on the list: sort and filter |
| `l` | pick a language |
| `s` | settings |
| `?` | every key, grouped by where it works |
| `q` or `Esc` | back, and quit from the shelf |
| `Ctrl+C` | quit, from anywhere |

Held down, the arrows and `PgUp` `PgDn` keep going. `Esc` also throws away a number you are
halfway through typing. However nmtk ends — `q` on the shelf, `Ctrl+C`, or a kill, hangup or
interrupt from outside — it gives the terminal back as it found it and stops every run.

nmtk needs a terminal of at least 80×24. Shrunk below that, it says
`This screen needs 80x24. Make the terminal larger.` and keeps your place; until the terminal is big
enough again, only `q`, `Esc` and `Ctrl+C` do anything.

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

You can change the thread count, the language and colour under `s`. A new thread count reaches the
next quest you open; one already open keeps the count it opened with. Your choices are kept in
`$XDG_CONFIG_HOME/nmtk/settings.toml`, or `~/.config/nmtk/settings.toml` when `XDG_CONFIG_HOME` is
not set. That file also holds the ids of the quests you have finished, so the list can mark them.
It is the only thing nmtk records about what you did, and it never leaves your machine.

If `NO_COLOR` is set, nmtk starts with colour off unless you have turned colour on yourself under
`s`. If part of the file cannot be read, nmtk keeps every setting it can read, puts the rest back to
their defaults, copies the file as it was to `settings.toml.bad` beside it, and says so on the
list.

Quests carry their version, and a release carries one version of each: the newest. Nothing is
kept around from before. If you want the version you learned from, check out that release of this
repository and build it — the whole history is there, and it is what a repository is for.

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
