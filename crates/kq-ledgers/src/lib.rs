//! Knowledge Quest: one coin, three sets of ledger rules.
//!
//! The reader writes one transfer and watches UTXO, account and object ledgers each take it their
//! own way — different things read, different things written, different state left behind. Then
//! they spend the same coin twice and watch all three refuse it at three different steps.
//!
//! This quest is the worked example of the standard in `nmtk-kq`: metadata that the list can sort
//! without opening anything, its own phrase table, five stages, knobs that take presets *and*
//! typed numbers, and an engine (`nmtk-ledger`) that never says a word.

mod phrases;
mod session;

use nmtk_core::{Language, MachineProfile};
use nmtk_kq::meta::{Category, Date, Difficulty, KqId, KqMeta, KqVersion, Requirements, StageKind};
use nmtk_kq::session::{Kq, KqSession};

use crate::phrases::Msg;

/// The quest as it sits in the list.
pub struct Ledgers;

impl Kq for Ledgers {
    fn meta(&self) -> KqMeta {
        KqMeta {
            id: KqId("ledgers.transaction-models"),
            version: KqVersion::new(1, 0),
            released: Date::new(2026, 9, 12),
            updated: Date::new(2026, 9, 12),
            category: Category::Ledgers,
            subcategory: "transaction-models",
            difficulty: Difficulty::Steady,
            minutes: 25,
            // Three small ledgers in memory. Any machine that runs nmtk runs this.
            needs: Requirements::ANY,
            stages: &[
                StageKind::Brief,
                StageKind::Run,
                StageKind::Tune,
                StageKind::Break,
                StageKind::Recap,
            ],
            tags: &["bitcoin", "ethereum", "sui", "utxo", "double-spend", "parallelism"],
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

    fn open(&self, _machine: &MachineProfile) -> Box<dyn KqSession> {
        Box::new(session::Session::new())
    }
}

#[cfg(test)]
mod tests {
    use nmtk_kq::meta::StageKind;

    use super::*;

    #[test]
    fn the_quest_declares_what_the_list_needs() {
        let meta = Ledgers.meta();
        assert_eq!(meta.id.as_str(), "ledgers.transaction-models");
        assert!(meta.stages.contains(&StageKind::Brief));
        assert!(meta.stages.contains(&StageKind::Run));
        assert!(meta.minutes > 0);
    }

    #[test]
    fn it_has_a_name_in_both_languages() {
        assert_eq!(Ledgers.title(Language::ENGLISH), "Ledger models");
        assert_eq!(Ledgers.title(Language::KOREAN), "원장 방식");
        assert!(!Ledgers.summary(Language::KOREAN).is_empty());
    }

    #[test]
    fn the_id_names_the_category_it_sits_in() {
        let meta = Ledgers.meta();
        assert!(meta.id.as_str().starts_with(meta.category.key()));
    }
}
