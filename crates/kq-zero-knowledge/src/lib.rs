//! Knowledge Quest: proofs that show nothing.
//!
//! The reader finds out what it means to prove you know something without showing it, then runs the
//! four systems the field arrived at — an interactive sigma protocol, the same proof made
//! non-interactive with Fiat-Shamir, an argument that is sound only because a number was destroyed,
//! and halo2, which needs no such number — and puts proving time, verification time and proof size
//! side by side. Then they attack all four, and watch the one attacker who is holding a trusted
//! setup's leftover randomness get a false statement past an unchanged verifier.
//!
//! The learning code is `nmtk-zk`, which never says a word: it returns numbers and enums, and every
//! sentence on screen comes from `phrases.rs` in this crate.

#![forbid(unsafe_code)]

mod phrases;
mod session;

use nmtk_core::{Language, MachineProfile};
use nmtk_kq::meta::{Category, Date, Difficulty, KqId, KqMeta, KqVersion, Requirements, StageKind};
use nmtk_kq::session::{Kq, KqSession};

use crate::phrases::Msg;

/// The quest as it sits in the list.
pub struct ZeroKnowledge;

impl Kq for ZeroKnowledge {
    fn meta(&self) -> KqMeta {
        KqMeta {
            id: KqId("cryptography.zero-knowledge"),
            version: KqVersion::new(1, 0),
            released: Date::new(2026, 9, 12),
            updated: Date::new(2026, 9, 12),
            category: Category::Cryptography,
            subcategory: "zero-knowledge",
            difficulty: Difficulty::Steep,
            minutes: 50,
            // One core draws while the other proves: halo2 on the widest circuit takes longer than
            // a frame, so the run is on a thread of its own.
            needs: Requirements::new(2, 0),
            stages: &[
                StageKind::Brief,
                StageKind::Run,
                StageKind::Tune,
                StageKind::Break,
                StageKind::Recap,
            ],
            tags: &[
                "zero-knowledge",
                "sigma",
                "schnorr",
                "fiat-shamir",
                "trusted-setup",
                "halo2",
                "zcash",
                "privacy",
            ],
        }
    }

    fn title(&self, language: Language) -> &'static str {
        Msg::Title.text(language)
    }

    fn summary(&self, language: Language) -> &'static str {
        Msg::Summary.text(language)
    }

    fn subcategory(&self, language: Language) -> &'static str {
        Msg::Subcategory.text(language)
    }

    fn open(&self, machine: &MachineProfile) -> Box<dyn KqSession> {
        Box::new(session::Session::new(machine))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use nmtk_kq::session::{Action, RunState};
    use nmtk_kq::theme::{EXPLAIN_PERCENT, MIN_HEIGHT, MIN_WIDTH, Theme};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::{Constraint, Layout, Rect};

    use super::*;

    fn machine() -> MachineProfile {
        MachineProfile { logical_cores: 4, total_memory_bytes: 0, available_memory_bytes: 0 }
    }

    // ---- What the list needs ---------------------------------------------------

    #[test]
    fn the_quest_declares_what_the_list_needs() {
        let meta = ZeroKnowledge.meta();
        assert_eq!(meta.id.as_str(), "cryptography.zero-knowledge");
        assert_eq!(meta.version, KqVersion::new(1, 0));
        assert_eq!(meta.category, Category::Cryptography);
        assert_eq!(meta.subcategory, "zero-knowledge");
        assert_eq!(meta.difficulty, Difficulty::Steep);
        assert_eq!(meta.minutes, 50);
        assert_eq!(meta.needs.cores, 2);
        assert_eq!(meta.stages, StageKind::ALL);
        assert!(!meta.tags.is_empty());
    }

    #[test]
    fn the_id_names_the_category_it_sits_in() {
        let meta = ZeroKnowledge.meta();
        assert!(meta.id.as_str().starts_with(meta.category.key()));
        assert!(meta.id.as_str().ends_with(meta.subcategory));
    }

    #[test]
    fn it_has_a_name_and_a_summary() {
        assert_eq!(ZeroKnowledge.title(Language::ENGLISH), "Zero-knowledge proofs");
        assert!(!ZeroKnowledge.summary(Language::ENGLISH).is_empty());
        assert!(!ZeroKnowledge.subcategory(Language::ENGLISH).is_empty());
    }

    // ---- The phrase table ------------------------------------------------------

    #[test]
    fn english_is_never_missing() {
        for msg in Msg::ALL {
            assert!(!msg.text(Language::ENGLISH).trim().is_empty(), "{msg:?} has no English");
        }
    }

    #[test]
    fn korean_is_never_blank_either() {
        // A line with no Korean falls back to the English, which still reads. A blank does not.
        for msg in Msg::ALL {
            assert!(!msg.text(Language::KOREAN).trim().is_empty(), "{msg:?} is blank in Korean");
        }
    }

    #[test]
    fn the_quest_is_named_in_both_languages() {
        assert_eq!(ZeroKnowledge.title(Language::ENGLISH), "Zero-knowledge proofs");
        assert_eq!(ZeroKnowledge.title(Language::KOREAN), "영지식 증명");
    }

    #[test]
    fn every_enum_the_engine_returns_has_a_word_for_it() {
        use nmtk_zk::{Claim, ForgeryKind, Item, SetupKind, Stage};
        for stage in Stage::ALL {
            assert!(!phrases::stage(stage).text(Language::ENGLISH).is_empty());
            assert!(!phrases::stage_brief(stage).text(Language::ENGLISH).is_empty());
        }
        let kinds = [
            ForgeryKind::GuessedResponse,
            ForgeryKind::WeakFiatShamirRebind,
            ForgeryKind::CorrectFiatShamirRebind,
            ForgeryKind::ToxicWasteOpening,
            ForgeryKind::OpeningWithoutToxicWaste,
            ForgeryKind::OverspendOutOfRange,
        ];
        for kind in kinds {
            assert!(!phrases::forgery(kind).text(Language::ENGLISH).is_empty());
        }
        let items = [
            Item::SpendingKey,
            Item::ViewingKey,
            Item::NoteBlinding,
            Item::Amount,
            Item::SenderAddress,
            Item::RecipientAddress,
            Item::NoteCommitment,
            Item::Nullifier,
            Item::Proof,
            Item::PublicStatement,
            Item::SetupParameters,
            Item::ToxicWaste,
        ];
        for item in items {
            assert!(!phrases::item(item).text(Language::ENGLISH).is_empty());
        }
        let claims = [
            Claim::TransactionHappened,
            Claim::ProofVerifies,
            Claim::AmountInRange,
            Claim::AmountMatchesCommitment,
            Claim::NullifierUnseen,
            Claim::SpenderHoldsKey,
        ];
        for claim in claims {
            assert!(!phrases::claim(claim).text(Language::ENGLISH).is_empty());
        }
        for setup in [SetupKind::NoneNeeded, SetupKind::Transparent, SetupKind::ToxicWaste] {
            assert!(!phrases::setup_kind(setup).text(Language::ENGLISH).is_empty());
        }
    }

    // ---- The session -----------------------------------------------------------

    #[test]
    fn the_session_moves_through_every_stage_it_declares() {
        let mut session = ZeroKnowledge.open(&machine());
        assert_eq!(session.stage(), StageKind::Brief);
        for stage in ZeroKnowledge.meta().stages {
            session.on(Action::Stage(*stage));
            assert_eq!(session.stage(), *stage);
            assert!(!session.explain(Language::ENGLISH).is_empty());
        }
        session.close();
    }

    #[test]
    fn only_the_tune_stage_offers_knobs_and_every_one_takes_a_typed_number() {
        let mut session = ZeroKnowledge.open(&machine());
        assert!(session.knobs().is_empty());
        session.on(Action::Stage(StageKind::Tune));
        assert_eq!(session.knobs().len(), 3);
        assert_eq!(session.chosen_knob(), Some(0));

        // The circuit width: presets by the arrow keys, and a number of the reader's own.
        session.on(Action::Next);
        session.on(Action::Nudge(1));
        for c in "37".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.knobs()[1].display(), "37");

        // Out of range is refused and the old value stays.
        for c in "200".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.knobs()[1].display(), "37");
        session.close();
    }

    /// Runs the quest the way the shell does: press Enter, then tick until the worker is done.
    fn run_to_done(session: &mut Box<dyn KqSession>) {
        session.on(Action::Go);
        let deadline = Instant::now() + Duration::from_secs(60);
        while session.run_state() != RunState::Done {
            assert!(Instant::now() < deadline, "the run never finished");
            session.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        session.tick();
    }

    #[test]
    fn a_run_finishes_and_the_stage_can_be_reset() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(StageKind::Run));
        assert_eq!(session.run_state(), RunState::Idle);
        run_to_done(&mut session);
        assert_eq!(session.run_state(), RunState::Done);
        session.on(Action::Reset);
        assert_eq!(session.run_state(), RunState::Idle);
        session.close();
    }

    // ---- Drawing ---------------------------------------------------------------

    /// The panel a quest is given on the smallest screen nmtk draws on: the shell keeps a line top
    /// and bottom, the explanation takes 38% of the width, and the rest is the quest's.
    fn panel(area: Rect) -> Rect {
        let [_, body, _] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
                .areas(area);
        let [_, run] = Layout::horizontal([
            Constraint::Percentage(EXPLAIN_PERCENT),
            Constraint::Percentage(100 - EXPLAIN_PERCENT),
        ])
        .areas(body);
        run
    }

    fn draw(session: &dyn KqSession) -> String {
        let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).expect("backend");
        terminal
            .draw(|frame| {
                let area = panel(frame.area());
                session.render(frame, area, Theme::new(true), Language::ENGLISH);
            })
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_run_stage_puts_the_three_numbers_on_one_screen_at_eighty_by_twenty_four() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(StageKind::Run));
        run_to_done(&mut session);
        let screen = draw(session.as_ref());

        for heading in ["Stage", "Prove", "Verify", "Size"] {
            assert!(screen.contains(heading), "no {heading} column:\n{screen}");
        }
        for system in ["Sigma", "Fiat-Shamir", "Trusted setup", "halo2"] {
            assert!(screen.contains(system), "no row for {system}:\n{screen}");
        }
        // The numbers are real: halo2's proof is kilobytes where a sigma transcript is 96 bytes.
        assert!(screen.contains("96 B"), "the sigma transcript size is missing:\n{screen}");
        assert!(screen.contains("KiB"), "halo2's proof size is missing:\n{screen}");
        // And the interactive protocol is laid out message by message underneath.
        assert!(screen.contains("Message 1/5"), "no walkthrough:\n{screen}");
        assert!(screen.contains("Public statement"), "no first message:\n{screen}");

        session.on(Action::Go);
        let next = draw(session.as_ref());
        assert!(next.contains("Message 2/5"), "Enter did not take the next step:\n{next}");
        assert!(next.contains("Commitment"), "no commitment message:\n{next}");
        session.close();
    }

    #[test]
    fn the_break_stage_shows_the_toxic_waste_holder_being_accepted() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(StageKind::Break));
        run_to_done(&mut session);
        let screen = draw(session.as_ref());

        assert!(screen.contains("guessed the response"), "no sigma attack:\n{screen}");
        assert!(screen.contains("kept the setup randomness"), "no waste attack:\n{screen}");
        assert!(screen.contains("accepted"), "no acceptance on screen:\n{screen}");
        assert!(screen.contains("rejected"), "no rejection on screen:\n{screen}");
        // The false statement, the value it was opened at, and the unchanged verifier's answer.
        assert!(screen.contains("21,845"), "the true value is missing:\n{screen}");
        assert!(screen.contains("21,845,000"), "the claimed value is missing:\n{screen}");
        assert!(
            screen.contains("unchanged verifier"),
            "the acceptance is not shown plainly:\n{screen}"
        );
        session.close();
    }

    #[test]
    fn the_recap_shows_four_sides_at_once_and_the_onlooker_sees_no_amount() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(StageKind::Recap));
        run_to_done(&mut session);
        let screen = draw(session.as_ref());

        for party in ["Sender", "Receiver", "Onlooker", "Attacker"] {
            assert!(screen.contains(party), "no {party} column:\n{screen}");
        }
        assert!(screen.contains("nullifier"), "the onlooker sees no nullifier:\n{screen}");
        assert!(screen.contains("commitment"), "the onlooker sees no commitment:\n{screen}");
        assert!(screen.contains("never"), "nothing is marked as never visible:\n{screen}");
        session.close();
    }

    #[test]
    fn every_stage_draws_inside_the_smallest_screen() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(StageKind::Run));
        run_to_done(&mut session);
        for stage in ZeroKnowledge.meta().stages {
            session.on(Action::Stage(*stage));
            let screen = draw(session.as_ref());
            let widest = screen.lines().map(|line| line.chars().count()).max().unwrap_or(0);
            assert_eq!(widest, MIN_WIDTH as usize, "the panel spilled:\n{screen}");
            assert_eq!(screen.lines().count(), MIN_HEIGHT as usize);
        }
        session.close();
    }

    #[test]
    fn the_tune_stage_answers_the_knobs() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(StageKind::Tune));
        // Pick halo2 alone, at the narrowest circuit.
        session.on(Action::Nudge(1));
        session.on(Action::Nudge(1));
        session.on(Action::Nudge(1));
        session.on(Action::Nudge(1));
        session.on(Action::Next);
        for c in "16".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        run_to_done(&mut session);

        let screen = draw(session.as_ref());
        assert!(screen.contains("halo2"), "the chosen system is missing:\n{screen}");
        assert!(screen.contains("rows used"), "the circuit shape is missing:\n{screen}");
        assert!(screen.contains("65,535"), "the 16-bit range is missing:\n{screen}");
        session.close();
    }
}
