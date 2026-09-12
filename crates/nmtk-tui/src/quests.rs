//! The shelf: every quest, in whatever order the reader asked for.

use nmtk_core::{Language, MachineProfile, format};
use nmtk_i18n::{Msg, t};
use nmtk_kq::meta::{Category, Difficulty, Fit, MachineNeeds, Requirements};
use nmtk_kq::session::Kq;
use nmtk_kq::text::{pad, truncate};
use nmtk_kq::theme::{State, Theme};
use nmtk_kq::{Filter, SortKey};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::App;
use crate::logo;

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let language = app.language();
    let room_for_logo = area.height >= 26 && area.width >= logo::LARGE_WIDTH + 4;

    let [banner, body] = Layout::vertical([
        Constraint::Length(if room_for_logo { logo::LARGE_HEIGHT + 2 } else { 1 }),
        Constraint::Min(6),
    ])
    .areas(area);
    render_banner(frame, banner, theme, language, room_for_logo);

    let [list_area, detail_area] =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).areas(body);

    let quests = app.visible();
    let chosen = app.list_index.min(quests.len().saturating_sub(1));

    let title = match app.versions_of {
        Some(_) => t(Msg::OlderVersions, language),
        None => t(Msg::Quests, language),
    };
    let list_block = theme.titled_panel(title);
    let inner = list_block.inner(list_area);
    frame.render_widget(list_block, list_area);

    if quests.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(t(Msg::QuestsEmpty, language), theme.muted())),
            inner,
        );
    } else {
        frame.render_widget(Paragraph::new(rows(app, &quests, chosen, theme, language)), inner);
        render_detail(frame, detail_area, quests[chosen], app, theme, language);
    }
}

fn render_banner(frame: &mut Frame, area: Rect, theme: Theme, language: Language, large: bool) {
    let lines: Vec<Line> = if large {
        logo::LARGE
            .iter()
            .map(|row| Line::from(Span::styled(*row, theme.heading())))
            .chain([
                Line::from(""),
                Line::from(Span::styled(t(Msg::Motto, language), theme.muted())),
            ])
            .collect()
    } else {
        vec![Line::from(Span::styled(t(Msg::Motto, language), theme.muted()))]
    };
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

/// One line per quest, with a category heading above each group when sorted that way.
fn rows<'a>(
    app: &App,
    quests: &[&'a dyn Kq],
    chosen: usize,
    theme: Theme,
    language: Language,
) -> Vec<Line<'a>> {
    let group = app.sort == SortKey::Category && app.versions_of.is_none();
    let mut lines = Vec::new();
    let mut last: Option<Category> = None;
    for (index, quest) in quests.iter().enumerate() {
        let meta = quest.meta();
        if group && last != Some(meta.category) {
            if last.is_some() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                t(category(meta.category), language).to_string(),
                theme.heading(),
            )));
            last = Some(meta.category);
        }
        let picked = index == chosen;
        let marker = if picked { State::Chosen.mark() } else { " " };
        let name = quest.title(language);
        let style = if picked { theme.selected() } else { theme.plain() };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} "), theme.state(State::Chosen)),
            Span::styled(pad(&truncate(name, 26), 26), style),
            Span::styled(format!(" {} ", meta.difficulty.marks()), theme.muted()),
            Span::styled(
                format!("{:>3}{}  ", meta.minutes, t(Msg::LabelMinutes, language)),
                theme.muted(),
            ),
            Span::styled(format!("v{}", meta.version), theme.muted()),
        ]));
    }
    lines
}

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    quest: &dyn Kq,
    app: &App,
    theme: Theme,
    language: Language,
) {
    let meta = quest.meta();
    let block = theme.panel();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [summary_area, facts_area] =
        Layout::vertical([Constraint::Min(4), Constraint::Length(9)]).areas(inner);

    let summary = Paragraph::new(vec![
        Line::from(Span::styled(quest.title(language), theme.heading())),
        Line::from(""),
        Line::from(Span::styled(quest.summary(language), theme.plain())),
    ])
    .wrap(Wrap { trim: true });
    frame.render_widget(summary, summary_area);

    let mut facts = vec![
        Line::from(vec![
            Span::styled(t(category(meta.category), language), theme.muted()),
            Span::styled(" · ", theme.muted()),
            Span::styled(quest.subcategory(language), theme.muted()),
        ]),
        Line::from(vec![
            Span::styled(format!("{} ", meta.difficulty.marks()), theme.muted()),
            Span::styled(pad(t(difficulty(meta.difficulty), language), 14), theme.plain()),
            Span::styled(
                format!("{} {}", meta.stages.len(), t(Msg::LabelStages, language)),
                theme.muted(),
            ),
        ]),
        Line::from(vec![
            Span::styled(format!("{} ", t(Msg::LabelLength, language)), theme.muted()),
            Span::styled(
                format!("{} {}", meta.minutes, t(Msg::LabelMinutes, language)),
                theme.plain(),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{} ", t(Msg::LabelUpdated, language)), theme.muted()),
            Span::styled(meta.updated.date(), theme.plain()),
            Span::styled(format!("  v{}", meta.version), theme.muted()),
        ]),
    ];
    facts.extend(needs_lines(&meta.needs, &app.machine, language, theme));
    facts.push(Line::from(""));
    facts.push(Line::from(vec![
        Span::styled(format!("{} ", t(Msg::LabelSort, language)), theme.muted()),
        Span::styled(t(sort_name(app.sort), language), theme.plain()),
        Span::styled(format!("   {} ", t(Msg::LabelFilter, language)), theme.muted()),
        Span::styled(t(filter_name(&app.filter), language), theme.plain()),
    ]));
    frame.render_widget(Paragraph::new(facts).wrap(Wrap { trim: true }), facts_area);
}

/// The two bars a quest sets, and where this machine sits against them.
///
/// A quest that runs anywhere says only that. Printing "Minimum 1 core" on a quest that needs
/// nothing is noise the reader has to read past to find the quests that do need something.
fn needs_lines(
    needs: &Requirements,
    machine: &MachineProfile,
    language: Language,
    theme: Theme,
) -> Vec<Line<'static>> {
    if *needs == Requirements::ANY {
        return vec![Line::from(Span::styled(t(Msg::NeedsAny, language), theme.muted()))];
    }
    let fit = needs.fit(machine);
    let (state, word) = match fit {
        Fit::Recommended => (State::Good, Msg::FitRecommended),
        Fit::Minimum => (State::Working, Msg::FitMinimum),
        Fit::Below => (State::Bad, Msg::FitBelow),
    };
    vec![
        bar_line(Msg::LabelMinimum, needs.minimum, language, theme),
        bar_line(Msg::LabelRecommended, needs.recommended, language, theme),
        Line::from(vec![
            Span::styled(format!("{} ", state.mark()), theme.state(state)),
            Span::styled(t(word, language), theme.state(state)),
        ]),
    ]
}

/// `Minimum      4 cores   2.0 GiB`, with the memory left off when the quest does not care.
fn bar_line(
    label: Msg,
    needs: MachineNeeds,
    language: Language,
    theme: Theme,
) -> Line<'static> {
    let cores = format!(
        "{} {}",
        needs.cores,
        t(if needs.cores == 1 { Msg::LabelCore } else { Msg::LabelCores }, language)
    );
    let mut spans = vec![
        Span::styled(pad(t(label, language), 13), theme.muted()),
        Span::styled(pad(&cores, 10), theme.plain()),
    ];
    if needs.memory_bytes > 0 {
        spans.push(Span::styled(format::bytes(needs.memory_bytes), theme.plain()));
    }
    Line::from(spans)
}

pub fn category(category: Category) -> Msg {
    match category {
        Category::Consensus => Msg::CategoryConsensus,
        Category::Ledgers => Msg::CategoryLedgers,
        Category::Cryptography => Msg::CategoryCryptography,
        Category::MachineLearning => Msg::CategoryMachineLearning,
        Category::Networking => Msg::CategoryNetworking,
        Category::Systems => Msg::CategorySystems,
    }
}

pub fn difficulty(difficulty: Difficulty) -> Msg {
    match difficulty {
        Difficulty::VeryEasy => Msg::DifficultyVeryEasy,
        Difficulty::Easy => Msg::DifficultyEasy,
        Difficulty::Medium => Msg::DifficultyMedium,
        Difficulty::Hard => Msg::DifficultyHard,
        Difficulty::VeryHard => Msg::DifficultyVeryHard,
    }
}

fn sort_name(sort: SortKey) -> Msg {
    match sort {
        SortKey::Category => Msg::SortByCategory,
        SortKey::Title => Msg::SortByTitle,
        SortKey::Difficulty => Msg::SortByDifficulty,
        SortKey::Newest => Msg::SortByNewest,
        SortKey::Shortest => Msg::SortByShortest,
    }
}

fn filter_name(filter: &Filter) -> Msg {
    match filter.category {
        Some(c) => category(c),
        None => Msg::FilterAll,
    }
}
