//! Knowledge Quest: one coin, three sets of ledger rules.
//!
//! The reader is told one small thing, presses Enter, and watches it happen to three real ledgers
//! at once. By the end they have sent a coin under Bitcoin's rules, Ethereum's and Sui's, tried to
//! spend it twice, and seen three different reasons why they could not.
//!
//! This quest is the worked example of the standard in `nmtk-kq`: a conversation rather than a
//! wall of text, stages with their own difficulty, knobs that take presets *and* typed numbers,
//! and an engine (`nmtk-ledger`) that never says a word.

mod phrases;
mod session;

use nmtk_core::{Language, MachineProfile};
use nmtk_kq::meta::{
    Category, Difficulty, KqId, KqMeta, Requirements, StageRole, StageSpec, Stamp, Version,
};
use nmtk_kq::session::{Kq, KqSession};

use crate::phrases::Msg;

/// The stages, in the order a reader walks them.
const STAGES: [StageSpec; 6] = [
    StageSpec::new("coins", StageRole::Explain, Difficulty::VeryEasy),
    StageSpec::new("send", StageRole::Run, Difficulty::VeryEasy),
    StageSpec::new("grow", StageRole::Run, Difficulty::Easy),
    StageSpec::new("tune", StageRole::Tune, Difficulty::Easy),
    StageSpec::new("twice", StageRole::Break, Difficulty::Medium),
    StageSpec::new("recap", StageRole::Recap, Difficulty::VeryEasy),
];

/// The quest as it sits in the list.
pub struct Ledgers;

impl Kq for Ledgers {
    fn meta(&self) -> KqMeta {
        KqMeta {
            id: KqId("ledgers.transaction-models"),
            version: Version::new(0, 2, 0),
            released: Stamp::new(2026, 9, 12, 10, 20, 48),
            updated: Stamp::new(2026, 9, 12, 14, 40, 42),
            category: Category::Ledgers,
            subcategory: "transaction-models",
            difficulty: Difficulty::Easy,
            minutes: 20,
            // Three small ledgers in memory. Any machine that runs nmtk runs this.
            needs: Requirements::ANY,
            stages: &STAGES,
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

    fn stage_name(&self, key: &str, language: Language) -> &'static str {
        match key {
            "coins" => Msg::StageCoins.text(language),
            "send" => Msg::StageSend.text(language),
            "grow" => Msg::StageGrow.text(language),
            "tune" => Msg::StageTune.text(language),
            "twice" => Msg::StageTwice.text(language),
            _ => Msg::StageRecap.text(language),
        }
    }

    fn open(&self, _machine: &MachineProfile) -> Box<dyn KqSession> {
        Box::new(session::Session::new())
    }
}

#[cfg(test)]
mod tests {
    use nmtk_kq::meta::MIN_STAGES;

    use super::*;

    #[test]
    fn the_quest_declares_what_the_list_needs() {
        let meta = Ledgers.meta();
        assert_eq!(meta.id.as_str(), "ledgers.transaction-models");
        assert!(meta.is_well_formed());
        assert!(meta.stages.len() >= MIN_STAGES);
    }

    #[test]
    fn it_has_a_name_in_both_languages() {
        assert_eq!(Ledgers.title(Language::ENGLISH), "Ledger models");
        assert_eq!(Ledgers.title(Language::KOREAN), "원장 방식");
    }

    #[test]
    fn every_stage_is_named() {
        for stage in Ledgers.meta().stages {
            for language in Language::ALL {
                assert!(
                    !Ledgers.stage_name(stage.key, *language).is_empty(),
                    "{} has no name in {language}",
                    stage.key
                );
            }
        }
    }

    #[test]
    fn the_quest_opens_gently_and_gets_harder() {
        let stages = Ledgers.meta().stages;
        assert_eq!(stages[0].difficulty, Difficulty::VeryEasy);
        assert_eq!(Ledgers.meta().steepest_stage(), Difficulty::Medium);
    }

    #[test]
    fn the_id_names_the_category_it_sits_in() {
        let meta = Ledgers.meta();
        assert!(meta.id.as_str().starts_with(meta.category.key()));
    }
}
