//! The list of quests, and the orders a reader can put it in.
//!
//! One quest per id: a release carries the newest version of each and nothing else. Someone who
//! wants the version they learned from checks out that release of the repository and builds it,
//! which is what a repository is for.

use std::cmp::Reverse;

use nmtk_core::Language;

use crate::meta::{Category, Difficulty, KqId};
use crate::session::Kq;

/// How the list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    /// Alphabetical in the language on screen.
    Title,
    /// Category, then the narrower shelf, then title.
    Category,
    /// Gentle first.
    Difficulty,
    /// Most recently updated first.
    Newest,
    /// Shortest first.
    Shortest,
}

impl SortKey {
    pub const ALL: [SortKey; 5] = [
        SortKey::Category,
        SortKey::Title,
        SortKey::Difficulty,
        SortKey::Newest,
        SortKey::Shortest,
    ];
}

/// What the list is narrowed to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Filter {
    pub category: Option<Category>,
    pub difficulty: Option<Difficulty>,
}

impl Filter {
    fn keeps(&self, quest: &dyn Kq) -> bool {
        let meta = quest.meta();
        self.category.is_none_or(|c| c == meta.category)
            && self.difficulty.is_none_or(|d| d == meta.difficulty)
    }
}

/// Every quest the program knows.
pub struct Catalogue {
    entries: Vec<Box<dyn Kq>>,
}

impl Catalogue {
    pub fn new(entries: Vec<Box<dyn Kq>>) -> Self {
        Self { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every quest, filtered and ordered. Two entries sharing an id would be a packaging mistake,
    /// and the newer of them is the one that is shown.
    pub fn list(&self, filter: &Filter, sort: SortKey, language: Language) -> Vec<&dyn Kq> {
        let mut newest: Vec<&dyn Kq> = Vec::new();
        for quest in self.entries.iter().map(|entry| entry.as_ref()) {
            if !filter.keeps(quest) {
                continue;
            }
            match newest.iter_mut().find(|kept| kept.meta().id == quest.meta().id) {
                Some(kept) if kept.meta().version < quest.meta().version => *kept = quest,
                Some(_) => {}
                None => newest.push(quest),
            }
        }
        sort_in_place(&mut newest, sort, language);
        newest
    }

    /// One quest by its permanent name.
    pub fn find(&self, id: KqId) -> Option<&dyn Kq> {
        self.entries.iter().map(|entry| entry.as_ref()).find(|quest| quest.meta().id == id)
    }

    /// The categories that actually have quests in them.
    pub fn categories(&self) -> Vec<Category> {
        let mut found: Vec<Category> =
            self.entries.iter().map(|quest| quest.meta().category).collect();
        found.sort_unstable();
        found.dedup();
        found
    }
}

fn sort_in_place(quests: &mut [&dyn Kq], sort: SortKey, language: Language) {
    match sort {
        SortKey::Title => quests.sort_by_key(|quest| quest.title(language)),
        SortKey::Category => quests.sort_by(|a, b| {
            let (ma, mb) = (a.meta(), b.meta());
            ma.category
                .cmp(&mb.category)
                .then(ma.subcategory.cmp(mb.subcategory))
                .then(a.title(language).cmp(b.title(language)))
        }),
        SortKey::Difficulty => quests.sort_by(|a, b| {
            a.meta()
                .difficulty
                .cmp(&b.meta().difficulty)
                .then(a.title(language).cmp(b.title(language)))
        }),
        SortKey::Newest => quests.sort_by_key(|quest| Reverse(quest.meta().updated)),
        SortKey::Shortest => quests.sort_by_key(|quest| quest.meta().minutes),
    }
}
