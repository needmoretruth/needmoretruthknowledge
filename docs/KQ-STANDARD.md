# The Knowledge Quest standard

A **Knowledge Quest (KQ)** is one finished piece of learning. A reader opens it, finds out why the
subject matters, runs the real thing on their own machine, turns its values, tries to break it, and
leaves knowing what happened.

nmtk is a program for holding a shelf of them. This document is what every quest is built against.

It exists for two reasons:

1. **One program.** Quests written by different hands, months apart, must feel like the same
   program — the same keys, the same shapes, the same places to look.
2. **Cheaper to build.** When the shape is settled there is nothing to invent. A new quest is
   filling in a form, not designing an interface.

The code in `crates/nmtk-kq` is the authority. Where this document and the code disagree, the code
is right and this document is a bug.

---

## 1. What a quest is

| | |
|---|---|
| **Not** | an article with an animation next to it |
| **Not** | a simulation that decides the outcome in advance |
| **Is** | the real mechanism, running on this machine, with its values in the reader's hands |

The test a quest has to pass: **the reader can change something and the result really changes.**
If every reader sees the same ending, it is not a quest yet.

## 2. What a quest declares

Plain data, available before the quest is opened, so the list can sort and filter without running
anything. See `nmtk_kq::meta::KqMeta`.

| Field | Meaning |
|---|---|
| `id` | Permanent name, `area.topic` (e.g. `consensus.proof-of-work`). Never changes. |
| `version` | `major.minor`. Major rises when the lesson itself changes. |
| `released` | When the quest first shipped, in any version. |
| `updated` | When *this* version shipped. |
| `category` | The shelf: Consensus, Ledgers, Cryptography, Machine learning, Networking, Systems. |
| `subcategory` | The narrower shelf, as a stable key (`proof-of-work`). Displayed through the quest's phrase table. |
| `difficulty` | Gentle, Steady or Steep. |
| `minutes` | Honest estimate of a first pass. |
| `needs` | Cores and memory below which the quest will be slow. It still opens; the list says so. |
| `stages` | Which of the five stages this quest has. |
| `tags` | Stable search keys (`bitcoin`, `hashing`). |

## 3. The five stages

Every quest walks the same path. `Brief` and `Run` are required; the rest are offered when the
subject deserves them. A reader jumps straight to any stage with `1`–`5`.

| Stage | Key | What happens |
|---|---|---|
| **Brief** | `1` | Why this is worth an hour. One screen, no jargon, ends with what the reader is about to see. |
| **Run** | `2` | The real thing runs. Numbers move. Nothing is decided in advance. |
| **Tune** | `3` | The reader changes values and the run answers. |
| **Break** | `4` | The reader attacks it and finds out what holds. |
| **Recap** | `5` | What just happened, in the order it happened. |

A quest with no honest attack does not invent one; it leaves `Break` out.

## 4. The screen

The shell draws the frame. A quest draws only inside the right-hand panel.

```
 NMTK  ·  Proof of work  ·  Run                                          EN   ← shell
╭ Proof of work ───────────╮╭──────────────────────────────────────────────╮
│                          ││                                              │
│  the explanation,        ││  the quest's own panel:                      │
│  38% of the width        ││  62% of the width                            │
│                          ││                                              │
╰──────────────────────────╯╰──────────────────────────────────────────────╯
 Enter run  ·  Space pause  ·  r reset  ·  l language  ·  q back           ← shell
```

- Minimum terminal: **80×24**. Draw for that first; use extra room, never require it.
- Repaint at most **ten times a second**, from the drawing thread only.
- The explanation panel is written by the quest, in the reader's language, and changes with the
  stage.

## 5. Colour

nmtk is **black and white**. Structure is carried by weight, spacing and reversal, so the program
reads the same on a light terminal, a dark one, and a monochrome one.

Colour says one thing: **state**. There are four, and no others.

| State | Colour | Mark | Means |
|---|---|---|---|
| Good | green | `+` | It worked, it verified, it is the honest chain |
| Bad | red | `x` | It failed, it was rejected, an attacker did it |
| Working | yellow | `~` | It is running |
| Chosen | blue | `>` | This is what you are pointing at |

Every state carries its mark as well as its colour, because a reader can turn colour off and must
lose nothing. Never paint a background: the reader's terminal owns it.

All values live in `nmtk_kq::theme`. A quest that picks its own colours is a bug.

## 6. Keys

The shell owns these on every screen:

| Key | Does |
|---|---|
| `↑` `↓` / `j` `k` | move between items |
| `←` `→` / `h` `l` | turn the chosen knob |
| `Enter` | go — start the run, take the next step, accept a typed number |
| `Space` | pause and resume |
| `r` | reset the run |
| `1`–`5` | jump to a stage |
| `Tab` | switch panel |
| `l` | English ⇄ 한국어 |
| `s` | settings |
| `?` | keys |
| `q` / `Esc` | back |

A quest receives `nmtk_kq::session::Action`, not key codes. It may add its own keys through
`KqSession::keys`, but never redefines one above.

## 7. Knobs

Everything a reader can change is a `Knob`. Every knob offers **presets and typing**: presets for
the reader who wants a good answer now, typing for the reader who wants *their* number. A knob with
presets only has decided for the reader what is worth trying, which is the opposite of the point.

Kinds: `Count` (whole numbers), `Share` (a percentage), `Decimal`, `Choice`, `Toggle`.

Out-of-range input is refused and the old value stays. Typing shows a cursor; `Esc` discards it.
While a number is being typed, report it through `KqSession::typing` — the shell then hands digits
to the quest instead of using `1`–`5` to jump between stages.

## 8. Words

Every quest carries **its own phrase table**, built with `nmtk_i18n::messages!`. Two quests being
written at the same time never touch the same file.

- **English is the source of truth** and is never missing.
- Korean is optional per line and falls back to English rather than to a blank.
- Engines return numbers and enums. **An engine never builds a sentence** — that is what keeps a
  third language from touching the learning code.

Write for someone who has not read the subject before. Name real things by their real names
(nonce, UTXO, nullifier) and gloss each one the first time.

## 9. Alignment

A Korean or Japanese glyph fills two terminal cells, so `format!("{:<12}")` lines a table up in
English and pulls it apart in Korean. Use `nmtk_kq::text::{width, pad, truncate}` for every column
you align. A table that only looks right in one language is a bug in both.

## 10. Engines

The code that does the work lives apart from the code that draws it (`crates/nmtk-pow`,
`nmtk-ledger`, `nmtk-transformer`, `nmtk-zk`). An engine:

- knows nothing about ratatui, the theme, or the language;
- returns plain data — numbers, enums, structs;
- runs long work on its own threads behind a handle whose `snapshot()` is cheap and holds no lock;
- is deterministic when given a seed, so a test can assert on an outcome.

```rust
pub struct Config { /* what the reader can change */ }
pub struct Handle { /* owns the threads */ }
impl Handle {
    pub fn snapshot(&self) -> Snapshot;
    pub fn pause(&self);
    pub fn resume(&self);
    pub fn stop(self);
}
```

Work that finishes instantly is a plain function instead.

## 11. Versions

A quest's `id` never changes. Its `version` rises, and **every version stays in the program**: a
reader who learned from an older one can open exactly what they saw. The list shows the newest of
each quest; older versions are one keypress away.

Raise `major` when the lesson changes enough that a returning reader would be surprised. Raise
`minor` for everything else. Set `updated` to the day the version ships; leave `released` alone.

## 12. What a quest must never do

- **Touch the network.** Not for updates, not for telemetry, not for anything. nmtk opens no
  sockets.
- **Work on the drawing thread.** `tick()` reads the latest state and returns.
- **Fake a number.** Everything on screen came from something that really ran here.
- **Invent interface.** Colours, borders, keys and shapes come from the standard.

## 13. Adding a quest

1. Write the engine in its own crate if the subject needs one: plain data out, no screen, no words.
2. Write the quest crate: `KqMeta`, a phrase table, the stages, the knobs, a `KqSession`.
3. Register it in the catalogue. Keep the previous version registered.
4. Tests: the engine's behaviour, and the quest drawing at 80×24 in both languages.
