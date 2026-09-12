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
use nmtk_kq::meta::{
    Category, Difficulty, KqId, KqMeta, MachineNeeds, Requirements, StageRole, StageSpec, Stamp,
    Version,
};
use nmtk_kq::session::{Kq, KqSession};

use crate::phrases::Msg;

/// The stages, in the order a reader walks them.
const STAGES: [StageSpec; 7] = [
    StageSpec::new("what", StageRole::Explain, Difficulty::VeryEasy),
    StageSpec::new("four", StageRole::Explain, Difficulty::Easy),
    StageSpec::new("run", StageRole::Run, Difficulty::Easy),
    StageSpec::new("messages", StageRole::Run, Difficulty::Medium),
    StageSpec::new("tune", StageRole::Tune, Difficulty::Medium),
    StageSpec::new("break", StageRole::Break, Difficulty::VeryHard),
    StageSpec::new("sides", StageRole::Recap, Difficulty::Medium),
];

/// The quest as it sits in the list.
pub struct ZeroKnowledge;

impl Kq for ZeroKnowledge {
    fn meta(&self) -> KqMeta {
        KqMeta {
            id: KqId("cryptography.zero-knowledge"),
            version: Version::new(0, 4, 0),
            released: Stamp::new(2026, 9, 12, 10, 20, 48),
            updated: Stamp::new(2026, 9, 12, 17, 20, 51),
            category: Category::Cryptography,
            subcategory: "zero-knowledge",
            difficulty: Difficulty::Hard,
            minutes: 50,
            needs: Requirements::new(
                // One core draws while the other proves: halo2 on the widest circuit takes
                // longer than a frame, so the run is on a thread of its own.
                MachineNeeds::new(2, 0),
                // Four cores and two gibibytes is where the widest circuit still proves in a
                // couple of seconds, which is what makes changing the width worth doing twice.
                MachineNeeds::new(4, 2 * 1024 * 1024 * 1024),
            ),
            stages: &STAGES,
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

    fn stage_name(&self, key: &str, language: Language) -> &'static str {
        match key {
            "what" => Msg::StageWhat.text(language),
            "four" => Msg::StageFour.text(language),
            "run" => Msg::StageRun.text(language),
            "messages" => Msg::StageMessages.text(language),
            "tune" => Msg::StageTune.text(language),
            "break" => Msg::StageBreak.text(language),
            _ => Msg::StageSides.text(language),
        }
    }

    fn open(&self, machine: &MachineProfile) -> Box<dyn KqSession> {
        Box::new(session::Session::new(machine))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use nmtk_kq::meta::MIN_STAGES;
    use nmtk_kq::session::{Action, RunState};
    use nmtk_kq::theme::{MIN_HEIGHT, MIN_WIDTH, Theme, split};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::{Constraint, Layout, Rect};

    use super::*;

    /// The stages by position, so a test says which one it means.
    const RUN: usize = 2;
    const MESSAGES: usize = 3;
    const TUNE: usize = 4;
    const BREAK: usize = 5;
    const SIDES: usize = 6;

    fn machine() -> MachineProfile {
        MachineProfile { logical_cores: 4, total_memory_bytes: 0, available_memory_bytes: 0 }
    }

    // ---- What the list needs ---------------------------------------------------

    #[test]
    fn the_quest_declares_what_the_list_needs() {
        let meta = ZeroKnowledge.meta();
        assert_eq!(meta.id.as_str(), "cryptography.zero-knowledge");
        assert!(meta.is_well_formed());
        assert!(meta.stages.len() >= MIN_STAGES);
        assert!(meta.tags.contains(&"halo2"));
    }

    #[test]
    fn the_id_names_the_category_and_the_subcategory_it_sits_in() {
        let meta = ZeroKnowledge.meta();
        assert!(meta.id.as_str().starts_with(meta.category.key()));
        assert!(meta.id.as_str().ends_with(meta.subcategory));
    }

    #[test]
    fn it_has_a_name_and_a_summary_in_both_languages() {
        assert_eq!(ZeroKnowledge.title(Language::ENGLISH), "Zero-knowledge proofs");
        assert_eq!(ZeroKnowledge.title(Language::KOREAN), "영지식 증명");
        assert!(!ZeroKnowledge.summary(Language::ENGLISH).is_empty());
        assert!(!ZeroKnowledge.subcategory(Language::ENGLISH).is_empty());
    }

    #[test]
    fn every_stage_is_named_in_every_language() {
        for stage in ZeroKnowledge.meta().stages {
            for language in Language::ALL {
                assert!(
                    !ZeroKnowledge.stage_name(stage.key, *language).is_empty(),
                    "{} has no name in {language}",
                    stage.key
                );
            }
        }
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

    // ---- The conversation ------------------------------------------------------

    #[test]
    fn it_opens_on_one_sentence_with_nothing_running() {
        let mut opened = ZeroKnowledge.open(&machine());
        assert_eq!(opened.stage(), 0);
        assert_eq!(opened.run_state(), RunState::Idle);
        assert_eq!(opened.transcript(Language::ENGLISH).len(), 1, "a wall of text on opening");
        opened.close();
    }

    #[test]
    fn the_session_moves_through_every_stage_it_declares() {
        let mut session = ZeroKnowledge.open(&machine());
        for stage in 0..ZeroKnowledge.meta().stages.len() {
            session.on(Action::Stage(stage));
            assert_eq!(session.stage(), stage);
            assert!(
                !session.transcript(Language::ENGLISH).is_empty(),
                "stage {stage} says nothing"
            );
        }
        session.close();
    }

    #[test]
    fn every_beat_is_short_enough_to_read_in_one_go() {
        let mut session = ZeroKnowledge.open(&machine());
        for stage in 0..ZeroKnowledge.meta().stages.len() {
            for language in Language::ALL {
                session.on(Action::Stage(stage));
                walk(&mut session);
                for beat in session.transcript(*language) {
                    assert!(
                        beat.text.chars().count() <= 170,
                        "stage {stage} in {language} says too much at once: {:?}",
                        beat.text
                    );
                }
            }
        }
        session.close();
    }

    #[test]
    fn only_the_tune_stage_offers_knobs_and_every_one_takes_a_typed_number() {
        let mut session = ZeroKnowledge.open(&machine());
        assert!(session.knobs().is_empty());
        session.on(Action::Stage(TUNE));
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

        // Out of range lands on the nearest end rather than vanishing, and the conversation says
        // where it landed: putting 37 back without a word reads as a broken key.
        for c in "200".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        let landed = session.knobs()[1].display();
        assert_ne!(landed, "200", "a number past the end should not be taken as typed");
        assert_ne!(landed, "37", "a number past the end should not be silently dropped");
        let said = session.transcript(Language::ENGLISH);
        assert!(
            said.iter().any(|beat| beat.text.contains(&landed)),
            "the reader was not told where the number landed"
        );
        session.close();
    }

    /// Presses Enter until this stage has nothing more to say without being waited on.
    fn walk(session: &mut Box<dyn KqSession>) {
        for _ in 0..40 {
            if !session.can_advance() {
                break;
            }
            session.on(Action::Go);
        }
    }

    /// Runs the stage the way the shell does: press Enter through it, then tick until the worker
    /// is done and the conversation has carried itself on.
    fn run_to_done(session: &mut Box<dyn KqSession>) {
        walk(session);
        let deadline = Instant::now() + Duration::from_secs(120);
        while session.run_state() != RunState::Done {
            assert!(Instant::now() < deadline, "the run never finished");
            session.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        session.tick();
        walk(session);
    }

    #[test]
    fn a_run_finishes_and_the_stage_can_be_reset() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(RUN));
        assert_eq!(session.run_state(), RunState::Idle);
        run_to_done(&mut session);
        assert_eq!(session.run_state(), RunState::Done);
        session.on(Action::Reset);
        assert_eq!(session.run_state(), RunState::Idle);
        assert_eq!(session.transcript(Language::ENGLISH).len(), 1, "reset kept the conversation");
        session.close();
    }

    /// Every measurement the reader is told about has to be one the engine really took.
    #[test]
    fn the_run_reports_all_four_systems_as_they_finish() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(RUN));
        run_to_done(&mut session);
        let beats = session.transcript(Language::ENGLISH);
        for system in ["Sigma", "Fiat-Shamir", "Trusted setup", "halo2"] {
            assert!(
                beats.iter().any(|beat| beat.text.starts_with(system)),
                "{system} was never reported: {beats:?}"
            );
        }
        session.close();
    }

    // ---- Drawing ---------------------------------------------------------------

    /// The panel a quest is given on the smallest screen nmtk draws on.
    fn panel(area: Rect) -> Rect {
        let [_, body, _] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
                .areas(area);
        let (talk, run) = split(body.width);
        let [_, panel] =
            Layout::horizontal([Constraint::Length(talk), Constraint::Length(run)]).areas(body);
        panel
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
        session.on(Action::Stage(RUN));
        run_to_done(&mut session);
        let screen = draw(session.as_ref());
        println!("\n===== zk run (80x24) =====\n{screen}");

        // At eighty columns the name takes a line of its own, so "Stage" is not a heading there.
        for heading in ["Prove", "Verify", "Size"] {
            assert!(screen.contains(heading), "no {heading} column:\n{screen}");
        }
        for system in ["Sigma", "Fiat-Shamir", "Trusted setup", "halo2"] {
            assert!(screen.contains(system), "no row for {system}:\n{screen}");
        }
        // The numbers are real: halo2's proof is kilobytes where a sigma transcript is 96 bytes.
        assert!(screen.contains("96 B"), "the sigma transcript size is missing:\n{screen}");
        assert!(screen.contains("KiB"), "halo2's proof size is missing:\n{screen}");
        session.close();
    }

    #[test]
    fn the_messages_stage_walks_the_protocol_one_message_at_a_time() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(MESSAGES));
        run_to_done(&mut session);
        let beats = session.transcript(Language::ENGLISH);
        for index in 1..=5 {
            assert!(
                beats.iter().any(|beat| beat.text.starts_with(&format!("message {index}/5"))),
                "message {index} was never reached: {beats:?}"
            );
        }
        let screen = draw(session.as_ref());
        assert!(screen.contains("Verdict"), "the last message is not on the panel:\n{screen}");
        session.close();
    }

    #[test]
    fn the_break_stage_shows_the_toxic_waste_holder_being_accepted() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(BREAK));
        run_to_done(&mut session);
        let screen = draw(session.as_ref());
        println!("\n===== zk break (80x24) =====\n{screen}");

        assert!(screen.contains("guessed the response"), "no sigma attack:\n{screen}");
        assert!(screen.contains("kept the setup randomness"), "no waste attack:\n{screen}");
        // The attack table answers for the system, because that is what its mark is about.
        assert!(screen.contains("broken"), "no system broken on screen:\n{screen}");
        assert!(screen.contains("held"), "no system held on screen:\n{screen}");
        // The verifier's own word stays where the lesson is that a verifier accepted a forgery.
        assert!(screen.contains("accepted"), "no acceptance on screen:\n{screen}");

        // And the conversation itself names the two that got through.
        let beats = session.transcript(Language::ENGLISH);
        let through: Vec<&str> = beats
            .iter()
            .filter(|beat| beat.text.contains("accepted"))
            .map(|beat| beat.text.as_str())
            .collect();
        assert_eq!(through.len(), 2, "two attacks get through, and only two: {through:?}");
        session.close();
    }

    #[test]
    fn the_sides_stage_shows_four_at_once_and_the_onlooker_sees_no_amount() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(SIDES));
        run_to_done(&mut session);
        let screen = draw(session.as_ref());

        for party in ["Sender", "Receiver", "Onlooker", "Attacker"] {
            assert!(screen.contains(party), "no {party} column:\n{screen}");
        }
        assert!(screen.contains("never"), "nothing is marked as never visible:\n{screen}");
        session.close();
    }

    #[test]
    fn every_stage_draws_inside_the_smallest_screen() {
        let mut session = ZeroKnowledge.open(&machine());
        session.on(Action::Stage(RUN));
        run_to_done(&mut session);
        for stage in 0..ZeroKnowledge.meta().stages.len() {
            session.on(Action::Stage(stage));
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
        session.on(Action::Stage(TUNE));
        // Pick halo2 alone, at the narrowest circuit.
        for _ in 0..4 {
            session.on(Action::Nudge(1));
        }
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
