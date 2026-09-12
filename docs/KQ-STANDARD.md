# The Knowledge Quest standard

A **Knowledge Quest (KQ)** is one finished piece of learning. A reader opens it, is told one small
thing, presses Enter, and watches that thing happen to the real mechanism on their own machine —
then another, then another, until they can say what it does and why.

nmtk is a program for holding a shelf of them. This document is what every quest is built against.

It exists for two reasons:

1. **One program.** Quests written by different hands, months apart, must feel like the same
   program — the same keys, the same shapes, the same places to look.
2. **Cheaper to build.** When the shape is settled there is nothing to invent. A new quest is
   filling in a form, not designing an interface.

The code in `crates/nmtk-kq` is the authority. Where this document and the code disagree, the code
is right and this document is a bug.

---

## 1. A quest is a conversation, not a document

This is the rule everything else follows from.

The first version of nmtk put five paragraphs on the left of the screen and a running mechanism on
the right. People read the first paragraph, skipped the rest, pressed keys until something moved,
and came away having learned nothing. They were not lazy. Nobody reads a page of prose to get to
the interesting part, and the interesting part was on the other side of the page.

So a quest talks:

> A sentence or two → the reader presses Enter → something real happens → a sentence about what
> just happened → Enter → …

The left panel is that conversation, oldest at the top, rising from the bottom of the panel exactly
like a messenger. The right panel is the thing itself, running. Pressing Enter is the only thing a
reader has to understand to get started.

**Rules for beats** (`nmtk_kq::session::Beat`):

| | |
|---|---|
| One or two sentences | A beat needing three sentences is two beats. |
| Under ~160 characters | Longer than that is a paragraph wearing a beat's clothes. |
| Says one thing | "X is true, and also Y" is two beats. |
| ELI5 | If a ten-year-old would stop reading, rewrite it. |
| No jargon before it is earned | A word may be used once it has been watched happening. |

There are three voices:

| Voice | Drawn | Used for |
|---|---|---|
| `Say` | plain | The quest explaining. |
| `Event(state)` | `+ x ~ >` and the state colour | Something that happened in the run. |
| `Ask` | `▸` | Something the reader has to do first. |

A quest builds its conversation in `transcript(language)`, on demand, from the work it has already
done — never stored as text. Switching language rewrites the whole conversation, not the next line
of it.

**Waiting is part of the conversation.** A stage that starts a real run stops there and lets the
run answer: beats arrive as blocks are mined or the loss falls, and the conversation carries itself
on when the thing it was about to describe has happened. Two methods keep that honest:

| | |
|---|---|
| `can_advance()` | Enter does something right now. False while a run is being waited on. |
| `at_end()` | This stage has said everything it has to say. |

Both are false in the middle of a run, and the shell needs to tell those apart: at the end of a
stage Enter walks into the next one, and in the middle of a run it does nothing, which is what the
reader is being told. A quest never walks itself into the next stage — only the shell knows there
is one.

## 2. What a quest declares

Plain data, available before the quest is opened, so the shelf can sort and filter without running
anything. See `nmtk_kq::meta::KqMeta`.

| Field | Meaning |
|---|---|
| `id` | Permanent name, `area.topic` (e.g. `consensus.proof-of-work`). Never changes. |
| `version` | The nmtk version this quest last shipped in. See §6. |
| `released` | When the quest first shipped, in any version. UTC, to the second. |
| `updated` | When *this* version shipped. UTC, to the second. |
| `category` | The shelf: Consensus, Ledgers, Cryptography, Machine learning, Networking, Systems. |
| `subcategory` | The narrower shelf, as a stable key (`proof-of-work`), worded by the phrase table. |
| `difficulty` | One of five. See §4. |
| `minutes` | Honest estimate of a first pass. |
| `needs` | Minimum **and** recommended. See §5. |
| `stages` | Three or more. See §3. |
| `tags` | Stable search keys (`bitcoin`, `hashing`). |

## 3. Stages

A quest has **at least three stages** and may have dozens. Two stages is a screen with a footnote.

Each stage declares a key, a role and its own difficulty:

```rust
StageSpec::new("twice", StageRole::Break, Difficulty::Medium)
```

| Role | What happens there |
|---|---|
| `Explain` | Why this is worth the time, and what is about to happen. |
| `Run` | The real thing runs. |
| `Tune` | The reader changes values and the run answers. |
| `Break` | The reader attacks it. |
| `Recap` | What just happened, in order, in the reader's own numbers. |

Rules:

- **A stage may be entered directly.** Tab walks them; a reader who jumps to the last stage must
  get a stage that works, so every stage sets up whatever state it needs on entry.
- **Difficulty may vary by stage.** A quest may open very easy and end hard.
- **Requirements may not vary by stage.** They are declared once, for the quest. A reader who was
  told they could run this must be able to finish it.
- **A stage's name is the quest's own words**, looked up by key through its phrase table.

## 4. Five difficulties

`Difficulty::{VeryEasy, Easy, Medium, Hard, VeryHard}`, drawn as `•····` through `•••••` so the
level reads with the colour off.

The quest's own `difficulty` is what the shelf sorts on. `steepest_stage()` is what the hardest
stage reaches, which may be higher.

## 5. Minimum and recommended

```rust
Requirements::new(
    MachineNeeds::new(2, 1 << 30),   // minimum: it runs
    MachineNeeds::new(8, 4 << 30),   // recommended: it runs the way it was written to
)
```

The shelf prints both and then says where *this* machine sits — `Fit::{Recommended, Minimum,
Below}` — measured against what `MachineProfile::detect()` found. A quest below the minimum still
opens and sizes itself down; it says so rather than refusing.

A quest that runs anywhere declares `Requirements::ANY` and the shelf prints one line instead of
three.

## 6. Versions

- nmtk and every quest carry **one number**, and it starts well below `1.0.0`.
- **A push is a version bump.** Not a date, not a day's work — the push.
- A quest's version is **the nmtk version it shipped in**. They move together.
- Minor or patch is a judgement call; `1.0.0` only on an explicit instruction.
- Timestamps are **UTC, to the second** (`2026-09-12T10:22:31Z`).
- Every version of a quest stays reachable from the shelf with `v`.

## 7. Values the reader turns

`nmtk_kq::knob::Knob`. Every knob offers **both** ways of choosing:

- **presets and arrows**, for the reader who wants a good answer now;
- **typed digits**, for the reader who wants *their* number.

A knob that only offers presets has decided for the reader what is worth trying, which is the
opposite of the point. Out-of-range typing is refused and the old value stays.

Because digits belong to the values, **stages are walked with Tab**, not with number keys. Where a
stage has no values to turn, `1`–`9` reach the first nine stages directly.

## 8. Words

- **English is written first and is never missing.** Every other language is a column beside it
  (`nmtk_i18n::messages!`), and a missing column falls back to English rather than to a blank.
- **Engines never produce words.** `nmtk-pow`, `nmtk-ledger`, `nmtk-transformer` and `nmtk-zk`
  return numbers and enums. Every sentence lives in a quest's phrase table.
- **What the quest says must match what the engine said.** If prose names a reason — "the coin is
  already gone" — a test asserts the engine really returns that reason. Prose that drifts from the
  machine is the worst bug this program can have, because it is invisible.
- **Never make a language agree a plural.** Write `coins 3`, not `3 coins`; `lines 1` reads, `1
  lines` does not.
- **Label first, value after.** `gap 3.2s`, `attacker's share 51.0%`. English tolerates either
  order and Korean does not, so one order has to be the rule, and this is the one that survives.

## 9. Drawing

- Black and white. Red, yellow, blue and green carry **state only**: `State::{Good, Bad, Working,
  Chosen}`, each with a one-column mark (`+ x ~ >`) so the screen reads with colour off.
- **Korean and Japanese glyphs take two columns.** Never use `str::len()` or `format!("{:<10}")` on
  anything a reader will see. Use `nmtk_kq::text::{width, pad, truncate, wrap}`.
- The smallest screen is **80×24**. Anything smaller gets a message, not a broken layout.
- A row that will not fit **drops whole items rather than cutting one in half**. Half a word is
  worse than a missing word.
- Nothing is drawn without a label. A graph with no title and no axis teaches nothing.

## 10. Running the work

- **One heavy run at a time, per machine.** Mining beside an attack halves both, and a 51% attack
  that cannot win because the screen is stealing its cores is a lie about proof of work. Stop the
  other run first.
- Work happens on worker threads; `tick()` is called about ten times a second on the drawing
  thread and only reads their latest state. It never blocks and never does the work.
- `close()` stops every thread and is always called before a session is dropped.
- A quest with no worker threads reports `RunState::Idle` and draws no status mark. A mark nobody
  can explain is clutter.

## 11. The shape of a quest crate

```
crates/kq-<topic>/src/
    lib.rs        the KqMeta, the stage list, the Kq impl — no words, no logic
    phrases.rs    every word, English first
    session.rs    the stage scripts, the deeds, the right-hand panel
```

A stage's script is a list of steps. Four kinds cover every quest written so far:

```rust
Say(Msg)        // one sentence
Ask(Msg)        // something the reader has to do before pressing Enter
Run(Deed)       // real work, started the moment the step is reached
Await(Until)    // the conversation waits; the run's own beats are the answer
```

A `Run` followed by an `Await` is one move: the reader presses Enter once and the waiting begins,
because a key whose only effect is to skip the answer is not worth offering.

The engine it drives is a separate crate and knows nothing about any of this.

## 12. Checklist before a quest ships

- [ ] Three or more stages, each with a name in every language.
- [ ] Every beat is one or two sentences and passes the length test.
- [ ] The first beat of the first stage is understandable by someone who has never heard of the
      subject.
- [ ] Every value a reader can turn accepts typed numbers as well as arrows.
- [ ] Every claim the prose makes about the engine is covered by a test.
- [ ] The quest reads at 80×24, and in every language it declares.
- [ ] `Requirements` are honest on a small machine.
- [ ] No `unsafe`, no network, no file written outside `~/.config/nmtk`.
