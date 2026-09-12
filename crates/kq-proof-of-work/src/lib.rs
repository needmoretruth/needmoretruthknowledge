//! Knowledge Quest: one coin mined, one chain rewritten.
//!
//! The reader watches why a network burns electricity to agree on anything, then mines real
//! blocks on their own machine at a practice difficulty, sets the difficulty and the miners and
//! the threads themselves, and finally buys a share of the hash power and tries to erase a
//! payment that was already confirmed.
//!
//! Everything on screen came from `nmtk-pow`, which hashes 80-byte headers twice with SHA-256 and
//! compares them against a target unpacked from the same four bytes a real header carries. This
//! crate adds the words and the shapes and nothing else.

#![forbid(unsafe_code)]

mod phrases;
mod session;

use nmtk_core::{Language, MachineProfile};
use nmtk_kq::meta::{Category, Date, Difficulty, KqId, KqMeta, KqVersion, Requirements, StageKind};
use nmtk_kq::session::{Kq, KqSession};

use crate::phrases::Msg;

/// The quest as it sits in the list.
pub struct ProofOfWork;

impl Kq for ProofOfWork {
    fn meta(&self) -> KqMeta {
        KqMeta {
            id: KqId("consensus.proof-of-work"),
            version: KqVersion::new(1, 0),
            released: Date::new(2026, 9, 12),
            updated: Date::new(2026, 9, 12),
            category: Category::Consensus,
            subcategory: "proof-of-work",
            difficulty: Difficulty::Steady,
            minutes: 40,
            // Two cores: one to hash with and one to keep the screen moving. The 51% attack needs
            // a thread on each side of the race.
            needs: Requirements::new(2, 0),
            stages: &[
                StageKind::Brief,
                StageKind::Run,
                StageKind::Tune,
                StageKind::Break,
                StageKind::Recap,
            ],
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

    fn open(&self, machine: &MachineProfile) -> Box<dyn KqSession> {
        Box::new(session::Session::new(machine))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quest_declares_what_the_list_needs() {
        let meta = ProofOfWork.meta();
        assert_eq!(meta.id.as_str(), "consensus.proof-of-work");
        assert_eq!(meta.version, KqVersion::new(1, 0));
        assert_eq!(meta.category, Category::Consensus);
        assert_eq!(meta.subcategory, "proof-of-work");
        assert_eq!(meta.difficulty, Difficulty::Steady);
        assert_eq!(meta.minutes, 40);
        assert_eq!(meta.needs, Requirements::new(2, 0));
        assert_eq!(meta.stages, StageKind::ALL);
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
        assert_eq!(ProofOfWork.title(Language::English), "Proof of work");
        assert!(!ProofOfWork.summary(Language::English).is_empty());
        assert!(!ProofOfWork.subcategory(Language::English).is_empty());
    }

    #[test]
    fn opening_it_starts_on_the_brief_and_nothing_is_running() {
        let machine =
            MachineProfile { logical_cores: 4, total_memory_bytes: 0, available_memory_bytes: 0 };
        let mut opened = ProofOfWork.open(&machine);
        assert_eq!(opened.stage(), StageKind::Brief);
        assert_eq!(opened.run_state(), nmtk_kq::session::RunState::Idle);
        opened.close();
    }
}
