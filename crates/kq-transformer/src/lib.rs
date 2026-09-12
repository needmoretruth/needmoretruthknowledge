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
use nmtk_kq::meta::{
    Category, Difficulty, KqId, KqMeta, MachineNeeds, Requirements, StageRole, StageSpec, Stamp,
    Version,
};
use nmtk_kq::session::{Kq, KqSession};

use crate::phrases::Msg;

/// The stages, in the order a reader walks them.
const STAGES: [StageSpec; 6] = [
    StageSpec::new("question", StageRole::Explain, Difficulty::VeryEasy),
    StageSpec::new("pieces", StageRole::Explain, Difficulty::Easy),
    StageSpec::new("train", StageRole::Run, Difficulty::Easy),
    StageSpec::new("tune", StageRole::Tune, Difficulty::Medium),
    StageSpec::new("break", StageRole::Break, Difficulty::VeryHard),
    StageSpec::new("recap", StageRole::Recap, Difficulty::VeryEasy),
];

/// The quest as it sits in the list.
pub struct Transformer;

impl Kq for Transformer {
    fn meta(&self) -> KqMeta {
        KqMeta {
            id: KqId("machine-learning.transformer"),
            version: Version::new(0, 4, 0),
            released: Stamp::new(2026, 9, 12, 10, 20, 48),
            updated: Stamp::new(2026, 9, 12, 17, 20, 51),
            category: Category::MachineLearning,
            subcategory: "architectures",
            difficulty: Difficulty::Hard,
            minutes: 45,
            needs: Requirements::new(
                // Training is real work: two cores so the screen still moves while it happens,
                // and a gibibyte because the optimiser keeps two extra numbers per weight.
                MachineNeeds::new(2, 1024 * 1024 * 1024),
                // Four cores and four gibibytes is where the default model finishes in under a
                // minute, which is what makes changing a setting and running again bearable.
                MachineNeeds::new(4, 4 * 1024 * 1024 * 1024),
            ),
            stages: &STAGES,
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

    fn stage_name(&self, key: &str, language: Language) -> &'static str {
        match key {
            "question" => Msg::StageQuestion.text(language),
            "pieces" => Msg::StagePieces.text(language),
            "train" => Msg::StageTrain.text(language),
            "tune" => Msg::StageTune.text(language),
            "break" => Msg::StageBreak.text(language),
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
        let meta = Transformer.meta();
        assert_eq!(meta.id.as_str(), "machine-learning.transformer");
        assert!(meta.is_well_formed());
        assert!(meta.stages.len() >= MIN_STAGES);
        assert!(meta.tags.contains(&"attention"));
    }

    #[test]
    fn the_id_names_the_category_it_sits_in() {
        let meta = Transformer.meta();
        assert!(meta.id.as_str().starts_with(meta.category.key()));
    }

    #[test]
    fn every_stage_is_named_in_every_language() {
        for stage in Transformer.meta().stages {
            for language in Language::ALL {
                assert!(
                    !Transformer.stage_name(stage.key, *language).is_empty(),
                    "{} has no name in {language}",
                    stage.key
                );
            }
        }
    }

    #[test]
    fn it_opens_on_one_sentence_with_nothing_running() {
        let machine = MachineProfile {
            logical_cores: 4,
            total_memory_bytes: 4 * 1024 * 1024 * 1024,
            available_memory_bytes: 0,
        };
        let mut opened = Transformer.open(&machine);
        assert_eq!(opened.stage(), 0);
        assert_eq!(opened.run_state(), nmtk_kq::session::RunState::Idle);
        assert_eq!(opened.transcript(Language::ENGLISH).len(), 1, "a wall of text on opening");
        opened.close();
    }
}
