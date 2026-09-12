//! Knowledge Quest: one coin mined, one chain rewritten.
//!
//! The reader is told why a network burns electricity to agree on anything, one sentence at a
//! time, then mines real blocks on their own machine at a practice difficulty, sets the
//! difficulty and the miners and the threads themselves, and finally buys a share of the hash
//! power and tries to erase a payment that was already confirmed. It fails at 30% and works at
//! 51%, and both endings happen for real.
//!
//! Everything on screen came from `nmtk-pow`, which hashes 80-byte headers twice with SHA-256 and
//! compares them against a target unpacked from the same four bytes a real header carries. This
//! crate adds the words and the shapes and nothing else.

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
const STAGES: [StageSpec; 6] = [
    StageSpec::new("why", StageRole::Explain, Difficulty::VeryEasy),
    StageSpec::new("puzzle", StageRole::Explain, Difficulty::Easy),
    StageSpec::new("mine", StageRole::Run, Difficulty::Easy),
    StageSpec::new("tune", StageRole::Tune, Difficulty::Medium),
    StageSpec::new("attack", StageRole::Break, Difficulty::Hard),
    StageSpec::new("recap", StageRole::Recap, Difficulty::VeryEasy),
];

/// The quest as it sits in the list.
pub struct ProofOfWork;

impl Kq for ProofOfWork {
    fn meta(&self) -> KqMeta {
        KqMeta {
            id: KqId("consensus.proof-of-work"),
            version: Version::new(0, 3, 2),
            released: Stamp::new(2026, 9, 12, 10, 20, 48),
            updated: Stamp::new(2026, 9, 12, 15, 26, 53),
            category: Category::Consensus,
            subcategory: "proof-of-work",
            difficulty: Difficulty::Medium,
            minutes: 40,
            needs: Requirements::new(
                // Two cores: one to hash with and one to keep the screen moving. The 51% attack
                // needs a thread on each side of the race or there is no race.
                MachineNeeds::new(2, 0),
                // Four is where a practice block arrives while you are still looking at it, and
                // where an attacker's share is something other than "half the machine or none".
                MachineNeeds::new(4, 0),
            ),
            stages: &STAGES,
            tags: &["bitcoin", "hashing", "sha-256", "mining", "51-percent", "double-spend"],
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
            "why" => Msg::StageWhy.text(language),
            "puzzle" => Msg::StagePuzzle.text(language),
            "mine" => Msg::StageMine.text(language),
            "tune" => Msg::StageTune.text(language),
            "attack" => Msg::StageAttack.text(language),
            _ => Msg::StageRecap.text(language),
        }
    }

    fn open(&self, machine: &MachineProfile) -> Box<dyn KqSession> {
        Box::new(session::Session::new(machine))
    }
}

#[cfg(test)]
mod tests {
    use nmtk_kq::meta::MIN_STAGES;

    use super::*;

    #[test]
    fn the_quest_declares_what_the_list_needs() {
        let meta = ProofOfWork.meta();
        assert_eq!(meta.id.as_str(), "consensus.proof-of-work");
        assert!(meta.is_well_formed());
        assert!(meta.stages.len() >= MIN_STAGES);
        assert!(meta.tags.contains(&"bitcoin"));
    }

    #[test]
    fn the_id_names_the_category_and_the_subcategory_it_sits_in() {
        let meta = ProofOfWork.meta();
        assert!(meta.id.as_str().starts_with(meta.category.key()));
        assert!(meta.id.as_str().ends_with(meta.subcategory));
    }

    #[test]
    fn it_has_a_name_and_a_summary_before_anything_is_opened() {
        assert_eq!(ProofOfWork.title(Language::ENGLISH), "Proof of work");
        assert!(!ProofOfWork.summary(Language::ENGLISH).is_empty());
        assert!(!ProofOfWork.subcategory(Language::ENGLISH).is_empty());
    }

    #[test]
    fn every_stage_is_named_in_every_language() {
        for stage in ProofOfWork.meta().stages {
            for language in Language::ALL {
                assert!(
                    !ProofOfWork.stage_name(stage.key, *language).is_empty(),
                    "{} has no name in {language}",
                    stage.key
                );
            }
        }
    }

    #[test]
    fn the_quest_opens_gently_and_ends_steeply() {
        let stages = ProofOfWork.meta().stages;
        assert_eq!(stages[0].difficulty, Difficulty::VeryEasy);
        assert_eq!(ProofOfWork.meta().steepest_stage(), Difficulty::Hard);
    }

    #[test]
    fn opening_it_starts_on_the_first_stage_with_nothing_running() {
        let machine =
            MachineProfile { logical_cores: 4, total_memory_bytes: 0, available_memory_bytes: 0 };
        let mut opened = ProofOfWork.open(&machine);
        assert_eq!(opened.stage(), 0);
        assert_eq!(opened.run_state(), nmtk_kq::session::RunState::Idle);
        assert_eq!(opened.transcript(Language::ENGLISH).len(), 1, "a wall of text on opening");
        opened.close();
    }
}
