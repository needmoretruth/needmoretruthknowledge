//! Knowledge Quest: a transformer trained by hand, on this machine, in about a minute.
//!
//! The reader watches a few tens of thousands of random numbers become a model that answers the
//! four letters `nmtk` with `need more truth knowledge`. Then they change the shape of it and
//! train again, then they set the learning rate far too high and watch the same code produce
//! noise instead.
//!
//! The quest owns no mathematics. Every number on screen comes from `nmtk-transformer`, which
//! never says a word; every word comes from [`phrases`], which never does any arithmetic.

#![forbid(unsafe_code)]

mod phrases;
mod session;

use nmtk_core::{Language, MachineProfile};
use nmtk_kq::meta::{Category, Date, Difficulty, KqId, KqMeta, KqVersion, Requirements, StageKind};
use nmtk_kq::session::{Kq, KqSession};

use crate::phrases::Msg;

/// The quest as it sits in the list.
pub struct Transformer;

impl Kq for Transformer {
    fn meta(&self) -> KqMeta {
        KqMeta {
            id: KqId("machine-learning.transformer"),
            version: KqVersion::new(1, 0),
            released: Date::new(2026, 9, 12),
            updated: Date::new(2026, 9, 12),
            category: Category::MachineLearning,
            subcategory: "architectures",
            difficulty: Difficulty::Steep,
            minutes: 45,
            // Training is real work: two cores so the screen still moves while it happens, and
            // two gibibytes because the optimiser keeps two extra numbers per weight. A smaller
            // machine still opens the quest and gets a smaller model.
            needs: Requirements::new(2, 2 * 1024 * 1024 * 1024),
            stages: &[
                StageKind::Brief,
                StageKind::Run,
                StageKind::Tune,
                StageKind::Break,
                StageKind::Recap,
            ],
            tags: &[
                "transformer",
                "attention",
                "language-model",
                "training",
                "gradient-descent",
                "learning-rate",
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
    use super::*;

    #[test]
    fn the_quest_declares_what_the_list_needs() {
        let meta = Transformer.meta();
        assert_eq!(meta.id.as_str(), "machine-learning.transformer");
        assert_eq!(meta.version, KqVersion::new(1, 0));
        assert_eq!(meta.category, Category::MachineLearning);
        assert_eq!(meta.subcategory, "architectures");
        assert_eq!(meta.difficulty, Difficulty::Steep);
        assert_eq!(meta.minutes, 45);
        assert_eq!(meta.needs, Requirements::new(2, 2 * 1024 * 1024 * 1024));
        assert_eq!(meta.stages, StageKind::ALL);
        assert!(!meta.tags.is_empty());
    }

    #[test]
    fn the_id_names_the_category_it_sits_in() {
        let meta = Transformer.meta();
        assert!(meta.id.as_str().starts_with(meta.category.key()));
    }

    #[test]
    fn it_has_a_name_and_a_subtitle() {
        assert_eq!(Transformer.title(Language::English), "Transformer");
        assert!(!Transformer.summary(Language::English).is_empty());
        assert_eq!(Transformer.subcategory(Language::English), "Architectures");
    }

    /// A quest this heavy still has to open on a machine that does not meet its requirements —
    /// the list says so, and the session sizes itself down.
    #[test]
    fn it_opens_on_a_machine_smaller_than_it_asks_for() {
        let small = MachineProfile {
            logical_cores: 1,
            total_memory_bytes: 512 * 1024 * 1024,
            available_memory_bytes: 256 * 1024 * 1024,
        };
        assert!(!Transformer.meta().needs.met_by(&small));
        let mut session = Transformer.open(&small);
        assert_eq!(session.stage(), StageKind::Brief);
        session.close();
    }
}
