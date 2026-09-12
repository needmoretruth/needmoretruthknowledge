//! The quest a reader has opened: five stages driven by one mining engine.
//!
//! The engine (`nmtk-pow`) owns the threads and hands back a snapshot; this file turns that
//! snapshot into a screen. Nothing here hashes anything, and nothing in the engine says a word.

use std::time::Duration;

use nmtk_core::{Language, MachineProfile, format};
use nmtk_kq::knob::{Knob, KnobValue};
use nmtk_kq::meta::StageKind;
use nmtk_kq::session::{Action, KqSession, Reaction, RunState};
use nmtk_kq::theme::{State, Theme};
use nmtk_kq::widgets;
use nmtk_pow::{
    AttackConfig, AttackHandle, AttackOutcome, AttackPhase, AttackSnapshot,
    BITCOIN_TARGET_BLOCK_SECONDS, BlockSummary, Hash256, MinerSpec, MiningConfig, MiningHandle,
    MiningSnapshot, Share, Target, effective_shares, expected_time_to_block,
    implied_network_hashrate, practice_bits, split_threads, start_attack, start_mining,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::phrases::{self, Msg};

/// Practice difficulties worth trying, in leading zero bits. A reader who wants 23 types 23.
const DIFFICULTY_PRESETS: &[u64] = &[18, 20, 22, 24, 26];
/// Miner counts worth trying.
const MINER_PRESETS: &[u64] = &[1, 2, 3, 4];
/// Thread counts worth trying.
const THREAD_PRESETS: &[u64] = &[1, 2, 4, 8];
/// Shares worth trying, as fractions of the whole machine.
const SHARE_PRESETS: &[f64] = &[0.10, 0.25, 0.50, 0.75];
/// Attacker shares worth trying. 30% loses, 51% wins, and both are the lesson.
const ATTACKER_PRESETS: &[f64] = &[0.20, 0.30, 0.45, 0.51, 0.70];
/// How many blocks a merchant might wait for.
const CONFIRMATION_PRESETS: &[u64] = &[1, 2, 3, 6];

/// More miners than this would not fit the panel, and four is enough to see a share split.
const MAX_MINERS: usize = 4;
/// A thread budget past this is a typo, not a choice.
const MAX_THREADS: u64 = 64;
/// Where the practice difficulty starts on a machine with one thread: about sixteen million
/// hashes a block, which is a few seconds. Every doubling of the threads adds a bit.
const BASE_ZERO_BITS: u64 = 24;
/// Past this a practice block stops being something anyone waits for.
const MAX_ZERO_BITS: u64 = 28;
/// Stable knob ids, one per miner.
const SHARE_IDS: [&str; MAX_MINERS] = ["share-1", "share-2", "share-3", "share-4"];
/// What a miner's share starts at. The first two differ so the split has something to say.
const DEFAULT_SHARES: [f64; MAX_MINERS] = [0.60, 0.40, 0.50, 0.50];
/// How many hash-rate samples the curve keeps. Ten a second, so this is the last twelve seconds.
const RATE_SAMPLES: usize = 120;
/// The panel needs this many rows before a hash-rate curve is worth the space it costs.
const CURVE_NEEDS_ROWS: u16 = 22;

/// Where each knob sits in the Tune stage.
const KNOB_DIFFICULTY: usize = 0;
const KNOB_MINERS: usize = 1;
const KNOB_THREADS: usize = 2;
const KNOB_SHARES_FROM: usize = 3;

/// Where each knob sits in the Break stage.
const KNOB_ATTACKER: usize = 0;
const KNOB_CONFIRMATIONS: usize = 1;

/// The quest a reader has opened.
pub struct Session {
    stage: StageKind,
    tune_knobs: Vec<Knob>,
    break_knobs: Vec<Knob>,
    chosen: usize,
    mining: Option<MiningHandle>,
    mining_snapshot: Option<MiningSnapshot>,
    attack: Option<AttackHandle>,
    attack_snapshot: Option<AttackSnapshot>,
    rates: Vec<f64>,
    state: RunState,
    refused: Option<Msg>,
}

impl Session {
    /// Opens the quest, sized to the machine it was handed.
    pub fn new(machine: &MachineProfile) -> Self {
        let threads = (machine.default_worker_threads() as u64).clamp(1, MAX_THREADS);
        // Twice the threads, twice the hashes a second, so a block stays a few seconds long.
        let doublings = u64::from(u64::BITS - 1 - threads.leading_zeros());
        let zero_bits = (BASE_ZERO_BITS + doublings).min(MAX_ZERO_BITS);
        let mut session = Self {
            stage: StageKind::Brief,
            tune_knobs: vec![
                Knob::new(
                    "difficulty",
                    KnobValue::Count {
                        current: zero_bits,
                        min: 16,
                        max: MAX_ZERO_BITS,
                        step: 1,
                        presets: DIFFICULTY_PRESETS,
                    },
                ),
                Knob::new(
                    "miners",
                    KnobValue::Count {
                        current: 2,
                        min: 1,
                        max: MAX_MINERS as u64,
                        step: 1,
                        presets: MINER_PRESETS,
                    },
                ),
                Knob::new(
                    "threads",
                    KnobValue::Count {
                        current: threads,
                        min: 1,
                        max: MAX_THREADS,
                        step: 1,
                        presets: THREAD_PRESETS,
                    },
                ),
            ],
            break_knobs: vec![
                Knob::new(
                    "attacker-share",
                    KnobValue::Share {
                        current: 0.51,
                        min: 0.01,
                        max: 0.99,
                        step: 0.01,
                        presets: ATTACKER_PRESETS,
                    },
                ),
                Knob::new(
                    "confirmations",
                    KnobValue::Count {
                        current: 2,
                        min: 1,
                        max: 12,
                        step: 1,
                        presets: CONFIRMATION_PRESETS,
                    },
                ),
            ],
            chosen: 0,
            mining: None,
            mining_snapshot: None,
            attack: None,
            attack_snapshot: None,
            rates: Vec::new(),
            state: RunState::Idle,
            refused: None,
        };
        session.sync_share_knobs();
        session
    }

    // ---- What the knobs say ----------------------------------------------------

    fn zero_bits(&self) -> u32 {
        count_of(self.tune_knobs.get(KNOB_DIFFICULTY)).min(u32::MAX as u64) as u32
    }

    fn miner_count(&self) -> usize {
        (count_of(self.tune_knobs.get(KNOB_MINERS)) as usize).clamp(1, MAX_MINERS)
    }

    fn thread_budget(&self) -> usize {
        count_of(self.tune_knobs.get(KNOB_THREADS)).max(1) as usize
    }

    fn miner_share(&self, index: usize) -> f64 {
        share_of(self.tune_knobs.get(KNOB_SHARES_FROM + index)).max(0.0)
    }

    fn attacker_share(&self) -> f64 {
        share_of(self.break_knobs.get(KNOB_ATTACKER)).clamp(0.01, 0.99)
    }

    fn confirmations(&self) -> u64 {
        count_of(self.break_knobs.get(KNOB_CONFIRMATIONS)).max(1)
    }

    /// One share knob per miner, no more and no fewer. Values already set are kept.
    fn sync_share_knobs(&mut self) {
        let wanted = KNOB_SHARES_FROM + self.miner_count();
        while self.tune_knobs.len() > wanted {
            self.tune_knobs.pop();
        }
        while self.tune_knobs.len() < wanted {
            let index = (self.tune_knobs.len() - KNOB_SHARES_FROM).min(MAX_MINERS - 1);
            self.tune_knobs.push(Knob::new(
                SHARE_IDS[index],
                KnobValue::Share {
                    current: DEFAULT_SHARES[index],
                    min: 0.01,
                    max: 1.0,
                    step: 0.05,
                    presets: SHARE_PRESETS,
                },
            ));
        }
        let last = self.knob_count().saturating_sub(1);
        self.chosen = self.chosen.min(last);
    }

    fn knob_count(&self) -> usize {
        match self.stage {
            StageKind::Tune => self.tune_knobs.len(),
            StageKind::Break => self.break_knobs.len(),
            _ => 0,
        }
    }

    fn move_choice(&mut self, step: i32) {
        let count = self.knob_count();
        if count == 0 {
            return;
        }
        let last = count - 1;
        self.chosen = if step > 0 {
            if self.chosen >= last { 0 } else { self.chosen + 1 }
        } else if self.chosen == 0 {
            last
        } else {
            self.chosen - 1
        };
    }

    /// Hands the chosen knob to `change`, then keeps the share knobs in step with the miner count.
    fn change_chosen(&mut self, change: impl FnOnce(&mut Knob)) -> Reaction {
        let index = self.chosen;
        let knobs = match self.stage {
            StageKind::Tune => &mut self.tune_knobs,
            StageKind::Break => &mut self.break_knobs,
            _ => return Reaction::Ignored,
        };
        match knobs.get_mut(index) {
            Some(knob) => change(knob),
            None => return Reaction::Ignored,
        }
        if self.stage == StageKind::Tune && index == KNOB_MINERS {
            self.sync_share_knobs();
        }
        Reaction::Handled
    }

    // ---- Running the engine ----------------------------------------------------

    /// The miners the current knobs describe.
    fn specs(&self) -> Vec<MinerSpec> {
        (0..self.miner_count())
            .map(|index| MinerSpec::percent(index as u32, self.miner_share(index) * 100.0))
            .collect()
    }

    /// Stops whatever is running and starts mining with the current numbers.
    fn start_run(&mut self) {
        self.stop_attack();
        self.stop_mining();
        let bits = match practice_bits(self.zero_bits()) {
            Ok(bits) => bits,
            Err(error) => {
                self.refused = Some(phrases::target_error(error));
                return;
            }
        };
        let config = MiningConfig::new(bits, self.specs(), self.thread_budget());
        // Only one heavy run at a time: mining beside an attack halves both and teaches nothing.
        self.stop_attack();
        match start_mining(config) {
            Ok(handle) => {
                self.mining_snapshot = Some(handle.snapshot());
                self.mining = Some(handle);
                self.rates.clear();
                self.refused = None;
                self.state = RunState::Running;
            }
            Err(error) => {
                self.refused = Some(phrases::config_error(error));
                self.state = RunState::Idle;
            }
        }
    }

    /// Stops whatever is running and starts the 51% attack with the current numbers.
    fn start_the_attack(&mut self) {
        self.stop_mining();
        self.stop_attack();
        let bits = match practice_bits(self.zero_bits()) {
            Ok(bits) => bits,
            Err(error) => {
                self.refused = Some(phrases::target_error(error));
                return;
            }
        };
        let (attacker, honest) = self.attack_threads();
        let config =
            AttackConfig::new(bits, attacker, honest).with_confirmations(self.confirmations());
        self.stop_mining();
        match start_attack(config) {
            Ok(handle) => {
                self.attack_snapshot = Some(handle.snapshot());
                self.attack = Some(handle);
                self.refused = None;
                self.state = RunState::Running;
            }
            Err(error) => {
                self.refused = Some(phrases::config_error(error));
                self.state = RunState::Idle;
            }
        }
    }

    /// The threads each side of the attack gets. Both sides need at least one.
    fn attack_threads(&self) -> (usize, usize) {
        let total = self.thread_budget().max(2);
        let wanted = (total as f64 * self.attacker_share()).round() as usize;
        let attacker = wanted.clamp(1, total - 1);
        (attacker, total - attacker)
    }

    fn stop_mining(&mut self) {
        if let Some(handle) = self.mining.take() {
            handle.stop();
            self.state = RunState::Idle;
        }
    }

    fn stop_attack(&mut self) {
        if let Some(handle) = self.attack.take() {
            handle.stop();
            self.state = RunState::Idle;
        }
    }

    /// Throws the run away. The knobs keep their values: a reader who set a number wants it kept.
    fn reset(&mut self) {
        self.stop_mining();
        self.stop_attack();
        self.mining_snapshot = None;
        self.attack_snapshot = None;
        self.rates.clear();
        self.refused = None;
        self.state = RunState::Idle;
    }

    // ---- Numbers the screens share ---------------------------------------------

    /// What one block costs at the practice difficulty, at difficulty 1, and the ratio between.
    fn cost_rows(&self, language: Language) -> Vec<(&'static str, String)> {
        let real = Target::difficulty_one().expected_hashes();
        let practice = match Target::from_leading_zero_bits(self.zero_bits()) {
            Ok(target) => target.expected_hashes(),
            Err(_) => f64::NAN,
        };
        let hashes = Msg::UnitHashes.text(language);
        let harder = if practice.is_finite() && practice > 0.0 {
            format!("{}x", format::count((real / practice) as u64))
        } else {
            Msg::Unavailable.text(language).to_string()
        };
        vec![
            (Msg::LabelPracticeHere.text(language), whole(practice, hashes, language)),
            (Msg::LabelDifficultyOne.text(language), whole(real, hashes, language)),
            (Msg::LabelHarderBy.text(language), harder),
        ]
    }

    /// This machine measured against difficulty 1 and against the network of early 2009.
    fn comparison_rows(&self, rate: f64, language: Language) -> Vec<(&'static str, String)> {
        let real = Target::difficulty_one();
        let estimate = expected_time_to_block(rate, real);
        let network = implied_network_hashrate(real, BITCOIN_TARGET_BLOCK_SECONDS);
        let share = if network > 0.0 { rate / network } else { f64::NAN };
        vec![
            (Msg::LabelOneBlockTakes.text(language), span(estimate.expected_seconds, language)),
            (Msg::LabelHalfTakeUnder.text(language), span(estimate.median_seconds, language)),
            (Msg::LabelNetwork2009.text(language), format::hashrate(network)),
            (Msg::LabelYouAre.text(language), format::percent(share)),
        ]
    }

    /// One row per miner: threads, the share it asked for, and the share it really holds.
    ///
    /// A live run answers with what it is really doing; with nothing running, the split the
    /// current knobs would produce is worked out without starting anything.
    fn split_rows(&self) -> Result<Vec<SplitRow>, Msg> {
        if let Some(snapshot) = self.mining_snapshot.as_ref().filter(|_| self.mining.is_some()) {
            return Ok(split_of(snapshot));
        }
        let specs = self.specs();
        let threads = split_threads(&specs, self.thread_budget()).map_err(phrases::config_error)?;
        let effective = effective_shares(&threads);
        let asked: f64 = (0..self.miner_count()).map(|index| self.miner_share(index)).sum();
        Ok(threads
            .iter()
            .enumerate()
            .map(|(index, count)| SplitRow {
                index,
                threads: *count,
                requested: if asked > 0.0 { self.miner_share(index) / asked } else { 0.0 },
                effective: effective.get(index).copied().unwrap_or(0.0),
            })
            .collect())
    }

    // ---- Drawing ---------------------------------------------------------------

    fn status_line(&self, elapsed: Duration, language: Language, theme: Theme) -> Line<'static> {
        let (state, message) = match self.state {
            RunState::Idle => (None, Msg::StatusIdle),
            RunState::Running => (Some(State::Working), Msg::StatusMining),
            RunState::Paused => (Some(State::Chosen), Msg::StatusPaused),
            RunState::Done => (Some(State::Good), Msg::StatusDone),
        };
        let style = state.map(|state| theme.state(state)).unwrap_or_else(|| theme.muted());
        let mark = state.map(|state| state.mark()).unwrap_or(" ");
        Line::from(vec![
            Span::styled(format!("{mark} "), style),
            Span::styled(pad(message.text(language), 26), style),
            Span::styled(format::duration(elapsed), theme.muted()),
        ])
    }

    fn brief_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let arrow = |text: &'static str| {
            Line::from(vec![
                Span::styled("  ↓  ", theme.muted()),
                Span::styled(text, theme.plain()),
            ])
        };
        let diagram = vec![
            Line::from(Span::styled(Msg::PuzzleHeader.text(language), theme.plain())),
            arrow(Msg::PuzzleHash.text(language)),
            arrow(Msg::PuzzleCompare.text(language)),
            Line::from(vec![
                Span::styled("     ", theme.plain()),
                Span::styled(Msg::PuzzleNo.text(language), theme.muted()),
            ]),
            Line::from(vec![
                Span::styled("     ", theme.plain()),
                Span::styled(Msg::PuzzleYes.text(language), theme.good()),
            ]),
            Line::from(""),
            Line::from(Span::styled(Msg::PuzzleCost.text(language), theme.heading())),
        ];
        let [top, rows] = Layout::vertical([Constraint::Length(7), Constraint::Min(0)]).areas(area);
        frame.render_widget(Paragraph::new(diagram), top);
        widgets::stats(frame, rows, theme, &self.cost_rows(language));
    }

    fn run_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let Some(snapshot) = &self.mining_snapshot else {
            let mut lines = vec![
                Line::from(Span::styled(Msg::RunIdle.text(language), theme.muted())),
                Line::from(""),
            ];
            if let Some(message) = self.refused {
                lines.push(Line::from(Span::styled(message.text(language), theme.bad())));
                lines.push(Line::from(""));
            }
            let [top, rows] =
                Layout::vertical([Constraint::Length(lines.len() as u16), Constraint::Min(0)])
                    .areas(area);
            frame.render_widget(Paragraph::new(lines), top);
            widgets::stats(frame, rows, theme, &self.cost_rows(language));
            return;
        };

        let rows: Vec<(&str, String)> = vec![
            (Msg::LabelHashRate.text(language), format::hashrate(snapshot.total_hashrate)),
            (Msg::LabelHashes.text(language), format::count(snapshot.total_hashes)),
            (Msg::LabelBlocksFound.text(language), format::count(snapshot.blocks_found)),
            (Msg::LabelChainHeight.text(language), format::count(snapshot.height)),
            (
                Msg::LabelHashesPerBlock.text(language),
                format::count(snapshot.expected_hashes_per_block as u64),
            ),
            (Msg::LabelStale.text(language), format::count(snapshot.stale_blocks)),
        ];
        let comparison = self.comparison_rows(snapshot.total_hashrate, language);

        let [status, gap, stats, gap2, heading, compare, note, gap3, blocks_head, blocks] =
            Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(6),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(4),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .areas(area);
        let _ = (gap, gap2, gap3);

        frame.render_widget(
            Paragraph::new(self.status_line(snapshot.elapsed, language, theme)),
            status,
        );
        widgets::stats(frame, stats, theme, &rows);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                Msg::HeadingComparison.text(language),
                theme.heading(),
            ))),
            heading,
        );
        widgets::stats(frame, compare, theme, &comparison);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                Msg::ComparisonDerived.text(language),
                theme.muted(),
            ))),
            note,
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                Msg::HeadingRecentBlocks.text(language),
                theme.heading(),
            ))),
            blocks_head,
        );
        self.block_rows(frame, blocks, theme, &snapshot.recent_blocks);
    }

    /// The newest blocks, as many as the space left will hold.
    fn block_rows(&self, frame: &mut Frame, area: Rect, theme: Theme, blocks: &[BlockSummary]) {
        if area.height == 0 {
            return;
        }
        let lines: Vec<Line<'static>> = blocks
            .iter()
            .rev()
            .take(area.height as usize)
            .map(|block| {
                let state = if block.in_best_chain { State::Good } else { State::Bad };
                Line::from(vec![
                    Span::styled(format!("{} ", state.mark()), theme.state(state)),
                    Span::styled(pad(&format::count(block.height), 5), theme.plain()),
                    Span::styled(pad(&format::duration(block.since_previous), 8), theme.muted()),
                    Span::styled(short_hash(block.hash), theme.muted()),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn tune_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let knob_lines = self.tune_knob_lines(language, theme);
        let knob_rows = knob_lines.len() as u16;
        let [knobs, gap, heading, table, footer] = Layout::vertical([
            Constraint::Length(knob_rows),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(2),
            Constraint::Length(2),
        ])
        .areas(area);
        let _ = gap;
        frame.render_widget(Paragraph::new(knob_lines), knobs);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                Msg::HeadingSplit.text(language),
                theme.heading(),
            ))),
            heading,
        );

        match self.split_rows() {
            Ok(rows) => {
                let mut lines = vec![Line::from(vec![
                    Span::styled(pad(Msg::ColumnMiner.text(language), 10), theme.muted()),
                    Span::styled(right(Msg::ColumnThreads.text(language), 8), theme.muted()),
                    Span::styled(right(Msg::ColumnAsked.text(language), 8), theme.muted()),
                    Span::styled(right(Msg::ColumnGot.text(language), 8), theme.muted()),
                ])];
                let mut differs = false;
                for row in &rows {
                    differs |= (row.requested - row.effective).abs() > 0.005;
                    lines.push(Line::from(vec![
                        Span::styled(
                            pad(phrases::miner_name(row.index).text(language), 10),
                            theme.plain(),
                        ),
                        Span::styled(right(&row.threads.to_string(), 8), theme.plain()),
                        Span::styled(right(&format::percent(row.requested), 8), theme.muted()),
                        Span::styled(right(&format::percent(row.effective), 8), theme.heading()),
                    ]));
                }
                if differs {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        Msg::TuneMismatch.text(language),
                        theme.muted(),
                    )));
                }
                frame.render_widget(Paragraph::new(lines), table);
            }
            Err(message) => frame.render_widget(
                Paragraph::new(Line::from(Span::styled(message.text(language), theme.bad()))),
                table,
            ),
        }

        let mut lines = Vec::new();
        if let Some(message) = self.refused {
            lines.push(Line::from(Span::styled(message.text(language), theme.bad())));
        }
        lines.push(Line::from(Span::styled(Msg::TuneRestart.text(language), theme.muted())));
        frame.render_widget(Paragraph::new(lines), footer);
    }

    fn tune_knob_lines(&self, language: Language, theme: Theme) -> Vec<Line<'static>> {
        self.tune_knobs
            .iter()
            .enumerate()
            .map(|(index, knob)| {
                let label = match index {
                    KNOB_DIFFICULTY => Msg::KnobDifficulty,
                    KNOB_MINERS => Msg::KnobMiners,
                    KNOB_THREADS => Msg::KnobThreads,
                    other => phrases::share_label(other - KNOB_SHARES_FROM),
                };
                let unit = if index == KNOB_DIFFICULTY {
                    Some(Msg::UnitZeroBits.text(language))
                } else {
                    None
                };
                knob_line(knob, label.text(language), unit, index == self.chosen, theme)
            })
            .collect()
    }

    fn break_knob_lines(&self, language: Language, theme: Theme) -> Vec<Line<'static>> {
        self.break_knobs
            .iter()
            .enumerate()
            .map(|(index, knob)| {
                let label = match index {
                    KNOB_ATTACKER => Msg::KnobAttackerShare,
                    _ => Msg::KnobConfirmations,
                };
                knob_line(knob, label.text(language), None, index == self.chosen, theme)
            })
            .collect()
    }

    fn break_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let knob_lines = self.break_knob_lines(language, theme);
        let [knobs, gap, body] = Layout::vertical([
            Constraint::Length(knob_lines.len() as u16),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(area);
        let _ = gap;
        frame.render_widget(Paragraph::new(knob_lines), knobs);

        let Some(snapshot) = &self.attack_snapshot else {
            let (attacker, honest) = self.attack_threads();
            let mut lines = vec![
                Line::from(Span::styled(Msg::BreakHint.text(language), theme.muted())),
                Line::from(""),
            ];
            if let Some(message) = self.refused {
                lines.push(Line::from(Span::styled(message.text(language), theme.bad())));
                lines.push(Line::from(""));
            }
            let rows = vec![
                (
                    Msg::LabelDifficulty.text(language),
                    format!("{} {}", self.zero_bits(), Msg::UnitZeroBits.text(language)),
                ),
                (Msg::LabelThreadSplit.text(language), format!("{attacker} / {honest}")),
                ("", Msg::ThreadsAttackerHonest.text(language).to_string()),
            ];
            let [top, table] =
                Layout::vertical([Constraint::Length(lines.len() as u16), Constraint::Min(0)])
                    .areas(body);
            frame.render_widget(Paragraph::new(lines), top);
            widgets::stats(frame, table, theme, &rows);
            return;
        };

        // While it runs, the phase says what is happening; once it is over, the ending says it.
        let phase = match snapshot.outcome {
            Some(_) => Line::from(""),
            None => Line::from(Span::styled(
                phrases::phase(snapshot.phase).text(language),
                theme.muted(),
            )),
        };
        let head = vec![self.status_line(snapshot.elapsed, language, theme), phase, Line::from("")];
        let ending = self.ending_lines(snapshot, language, theme);
        let [top, table, tail] = Layout::vertical([
            Constraint::Length(head.len() as u16),
            Constraint::Min(0),
            Constraint::Length(ending.len() as u16),
        ])
        .areas(body);
        frame.render_widget(Paragraph::new(head), top);
        widgets::stats(frame, table, theme, &self.attack_rows(snapshot, language));
        frame.render_widget(Paragraph::new(ending), tail);
    }

    /// How it ended: green when the chain held, red when an attacker rewrote it.
    fn ending_lines(
        &self,
        snapshot: &AttackSnapshot,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let Some(outcome) = snapshot.outcome else { return Vec::new() };
        let state = match outcome {
            AttackOutcome::Succeeded => State::Bad,
            AttackOutcome::GaveUp => State::Good,
        };
        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(format!("{} ", state.mark()), theme.state(state)),
                Span::styled(phrases::outcome(outcome).text(language), theme.state(state)),
            ]),
        ];
        if outcome == AttackOutcome::GaveUp && snapshot.attacker_share < 0.5 {
            lines.push(Line::from(Span::styled(
                Msg::BreakLessonFailed.text(language),
                theme.muted(),
            )));
        }
        lines
    }

    /// What the attack has done so far, or what it did.
    fn attack_rows(
        &self,
        snapshot: &AttackSnapshot,
        language: Language,
    ) -> Vec<(&'static str, String)> {
        let mut rows = vec![
            (Msg::LabelPublicChain.text(language), format::count(snapshot.public_height)),
            (Msg::LabelPrivateChain.text(language), format::count(snapshot.private_height)),
        ];
        let gap = snapshot.lead.unsigned_abs();
        rows.push((
            if snapshot.lead >= 0 {
                Msg::LabelAheadBy.text(language)
            } else {
                Msg::LabelBehindBy.text(language)
            },
            blocks(gap, language),
        ));
        rows.push((
            Msg::LabelFurthestBehind.text(language),
            blocks(snapshot.max_deficit, language),
        ));
        let seen = snapshot
            .confirmations_when_reverted
            .or(snapshot.confirmations_at_release)
            .unwrap_or(snapshot.victim_confirmations);
        rows.push((
            Msg::LabelMerchantSaw.text(language),
            plural(seen, Msg::UnitConfirmation, Msg::UnitConfirmations, language),
        ));
        rows.push((
            Msg::LabelGoods.text(language),
            if snapshot.victim_released {
                Msg::VictimReleased.text(language).to_string()
            } else {
                Msg::VictimWaiting.text(language).to_string()
            },
        ));
        rows.push((Msg::LabelBlocksErased.text(language), format::count(snapshot.blocks_reverted)));
        rows.push((
            Msg::LabelAttackTook.text(language),
            match snapshot.attack_duration {
                Some(took) => format::duration(took),
                None => Msg::Unavailable.text(language).to_string(),
            },
        ));
        rows.push((
            Msg::LabelThreadSplit.text(language),
            format!(
                "{} / {}  ({})",
                snapshot.attacker.threads,
                snapshot.honest.threads,
                format::percent(snapshot.attacker_share)
            ),
        ));
        rows.push(("", Msg::ThreadsAttackerHonest.text(language).to_string()));
        rows
    }

    fn recap_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let unknown = || Msg::NotYet.text(language).to_string();
        let mining = self.mining_snapshot.as_ref();
        let rate = mining.map(|snapshot| snapshot.total_hashrate).unwrap_or(0.0);
        let real = Target::difficulty_one();
        let network = implied_network_hashrate(real, BITCOIN_TARGET_BLOCK_SECONDS);

        // The recap says what happened, so the split it shows is the one the run really used.
        let split = mining.map(split_of).and_then(|rows| rows.first().copied());
        let attack = self.attack_snapshot.as_ref();

        let rows: Vec<(&'static str, String)> = vec![
            (
                Msg::RecapDifficulty.text(language),
                format!("{} {}", self.zero_bits(), Msg::UnitZeroBits.text(language)),
            ),
            (
                Msg::RecapHashRate.text(language),
                match mining {
                    Some(_) => format::hashrate(rate),
                    None => unknown(),
                },
            ),
            (
                Msg::RecapBlocks.text(language),
                match mining {
                    Some(snapshot) => format::count(snapshot.blocks_in_chain),
                    None => unknown(),
                },
            ),
            (
                Msg::RecapAtDifficultyOne.text(language),
                match mining {
                    Some(_) => span(expected_time_to_block(rate, real).expected_seconds, language),
                    None => unknown(),
                },
            ),
            (Msg::RecapNetwork.text(language), format::hashrate(network)),
            (
                Msg::RecapSplit.text(language),
                match split {
                    Some(row) => format!(
                        "{} / {}",
                        format::percent(row.requested),
                        format::percent(row.effective)
                    ),
                    None => unknown(),
                },
            ),
            (
                Msg::RecapAttack.text(language),
                match attack.and_then(|snapshot| snapshot.outcome) {
                    Some(outcome) => phrases::short_outcome(outcome).text(language).to_string(),
                    None => unknown(),
                },
            ),
        ];

        let lines: Vec<Line<'static>> = rows
            .iter()
            .enumerate()
            .map(|(index, (name, value))| {
                Line::from(vec![
                    Span::styled(format!("{} ", index + 1), theme.muted()),
                    Span::styled(pad(name, 21), theme.plain()),
                    Span::styled(value.clone(), theme.heading()),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), area);
    }
}

/// One miner's line in the Tune table.
#[derive(Debug, Clone, Copy)]
struct SplitRow {
    index: usize,
    threads: usize,
    requested: f64,
    effective: f64,
}

impl KqSession for Session {
    fn stage(&self) -> StageKind {
        self.stage
    }

    fn go_to(&mut self, stage: StageKind) {
        if stage == self.stage {
            return;
        }
        // One run at a time: the attack and the mining run would otherwise fight for the cores.
        if self.stage == StageKind::Break {
            self.stop_attack();
        } else if stage == StageKind::Break {
            self.stop_mining();
        }
        self.stage = stage;
        self.chosen = 0;
    }

    fn knobs(&self) -> &[Knob] {
        match self.stage {
            StageKind::Tune => &self.tune_knobs,
            StageKind::Break => &self.break_knobs,
            _ => &[],
        }
    }

    fn chosen_knob(&self) -> Option<usize> {
        (self.knob_count() > 0).then_some(self.chosen)
    }

    fn on(&mut self, action: Action) -> Reaction {
        match action {
            Action::Stage(stage) => {
                self.go_to(stage);
                Reaction::Handled
            }
            Action::Go => match self.stage {
                StageKind::Run | StageKind::Tune => {
                    self.start_run();
                    Reaction::Handled
                }
                StageKind::Break => {
                    self.start_the_attack();
                    Reaction::Handled
                }
                _ => Reaction::Ignored,
            },
            Action::Reset => {
                self.reset();
                Reaction::Handled
            }
            Action::PauseOrResume => {
                let paused = match (&self.attack, &self.mining) {
                    (Some(handle), _) => {
                        let paused = handle.is_paused();
                        if paused {
                            handle.resume()
                        } else {
                            handle.pause()
                        }
                        Some(!paused)
                    }
                    (None, Some(handle)) => {
                        let paused = handle.is_paused();
                        if paused {
                            handle.resume()
                        } else {
                            handle.pause()
                        }
                        Some(!paused)
                    }
                    (None, None) => None,
                };
                match paused {
                    Some(true) => {
                        self.state = RunState::Paused;
                        Reaction::Handled
                    }
                    Some(false) => {
                        self.state = RunState::Running;
                        Reaction::Handled
                    }
                    None => Reaction::Ignored,
                }
            }
            Action::Next => {
                self.move_choice(1);
                Reaction::Handled
            }
            Action::Previous => {
                self.move_choice(-1);
                Reaction::Handled
            }
            Action::Nudge(direction) => self.change_chosen(|knob| knob.nudge(direction)),
            Action::Type(c) => self.change_chosen(|knob| knob.type_char(c)),
            Action::Backspace => self.change_chosen(Knob::backspace),
            Action::Commit => self.change_chosen(|knob| {
                knob.commit();
            }),
            Action::Cancel => self.change_chosen(Knob::cancel),
        }
    }

    fn tick(&mut self) {
        if let Some(handle) = &self.mining {
            let snapshot = handle.snapshot();
            self.state = if snapshot.paused {
                RunState::Paused
            } else if snapshot.finished {
                RunState::Done
            } else {
                RunState::Running
            };
            if self.rates.len() >= RATE_SAMPLES {
                self.rates.remove(0);
            }
            self.rates.push(snapshot.total_hashrate);
            self.mining_snapshot = Some(snapshot);
        }
        if let Some(handle) = &self.attack {
            let snapshot = handle.snapshot();
            self.state = if snapshot.paused {
                RunState::Paused
            } else if snapshot.finished || snapshot.phase == AttackPhase::Finished {
                RunState::Done
            } else {
                RunState::Running
            };
            self.attack_snapshot = Some(snapshot);
        }
    }

    fn run_state(&self) -> RunState {
        self.state
    }

    fn explain(&self, language: Language) -> Vec<Line<'static>> {
        let paragraphs: &[Msg] = match self.stage {
            StageKind::Brief => &[
                Msg::BriefProblem,
                Msg::BriefCost,
                Msg::BriefNonce,
                Msg::BriefRule,
                Msg::BriefPromise,
            ],
            StageKind::Run => &[
                Msg::RunExplainReal,
                Msg::RunExplainDifficulty,
                Msg::RunExplainTail,
                Msg::RunExplainDerived,
            ],
            StageKind::Tune => &[
                Msg::TuneExplainHint,
                Msg::TuneExplainDifficulty,
                Msg::TuneShares,
                Msg::TuneExplainThreads,
                Msg::TuneExplainOversubscribe,
            ],
            StageKind::Break => &[
                Msg::BreakExplainStory,
                Msg::BreakExplainPrivate,
                Msg::BreakExplainPublish,
                Msg::BreakExplainArithmetic,
                Msg::BreakExplainSettings,
            ],
            StageKind::Recap => &[
                Msg::RecapExplainOrder,
                Msg::RecapExplainMining,
                Msg::RecapExplainTune,
                Msg::RecapExplainBreak,
            ],
        };
        let mut lines = Vec::new();
        for (index, message) in paragraphs.iter().enumerate() {
            if index > 0 {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(message.text(language)));
        }
        lines
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let title = match self.stage {
            StageKind::Brief => Msg::PuzzleTitle,
            StageKind::Run => Msg::RunTitle,
            StageKind::Tune => Msg::TuneTitle,
            StageKind::Break => Msg::BreakTitle,
            StageKind::Recap => Msg::RecapTitle,
        };
        let block = theme.titled_panel(title.text(language));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        match self.stage {
            StageKind::Brief => self.brief_panel(frame, inner, theme, language),
            StageKind::Run => {
                // A taller terminal gets the hash rate over time; 80x24 does not have the room.
                if inner.height >= CURVE_NEEDS_ROWS && self.rates.len() > 1 {
                    let [top, curve] =
                        Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(inner);
                    self.run_panel(frame, top, theme, language);
                    widgets::curve(frame, curve, theme, &self.rates);
                } else {
                    self.run_panel(frame, inner, theme, language);
                }
            }
            StageKind::Tune => self.tune_panel(frame, inner, theme, language),
            StageKind::Break => self.break_panel(frame, inner, theme, language),
            StageKind::Recap => self.recap_panel(frame, inner, theme, language),
        }
    }

    fn keys(&self, language: Language) -> Vec<(&'static str, &'static str)> {
        match self.stage {
            StageKind::Tune | StageKind::Break => vec![
                ("←→", Msg::KeyTurn.text(language)),
                ("0-9", Msg::KeyTypeNumber.text(language)),
            ],
            _ => Vec::new(),
        }
    }

    fn close(&mut self) {
        self.stop_mining();
        self.stop_attack();
    }
}

/// A knob's row: the mark, the label, and the value or what is being typed into it.
fn knob_line(
    knob: &Knob,
    label: &str,
    unit: Option<&str>,
    chosen: bool,
    theme: Theme,
) -> Line<'static> {
    let marker = if chosen { State::Chosen.mark() } else { " " };
    let value = match unit {
        Some(unit) if knob.draft().is_none() => format!("{} {unit}", knob.display()),
        _ => knob.display(),
    };
    Line::from(vec![
        Span::styled(format!("{marker} "), theme.state(State::Chosen)),
        Span::styled(pad(label, 20), theme.plain()),
        Span::styled(value, if chosen { theme.heading() } else { theme.muted() }),
    ])
}

fn count_of(knob: Option<&Knob>) -> u64 {
    match knob.map(|knob| &knob.value) {
        Some(KnobValue::Count { current, .. }) => *current,
        _ => 0,
    }
}

fn share_of(knob: Option<&Knob>) -> f64 {
    match knob.map(|knob| &knob.value) {
        Some(KnobValue::Share { current, .. }) => *current,
        _ => 0.0,
    }
}

/// What a run really did with its threads, one row per miner.
fn split_of(snapshot: &MiningSnapshot) -> Vec<SplitRow> {
    let asked: f64 = snapshot.miners.iter().map(|miner| requested_of(miner.requested)).sum();
    snapshot
        .miners
        .iter()
        .enumerate()
        .map(|(index, miner)| SplitRow {
            index,
            threads: miner.threads,
            requested: if asked > 0.0 { requested_of(miner.requested) / asked } else { 0.0 },
            effective: miner.effective_share,
        })
        .collect()
}

/// A share as the engine reported it being asked for.
fn requested_of(share: Share) -> f64 {
    match share {
        Share::Percent(percent) => percent.max(0.0),
        Share::Threads(threads) => threads as f64,
    }
}

/// `text` padded to `width` columns, counting characters rather than bytes.
fn pad(text: &str, width: usize) -> String {
    let used = text.chars().count();
    let mut out = text.to_string();
    for _ in used..width {
        out.push(' ');
    }
    out
}

/// `text` pushed to the right of `width` columns, so a column of numbers lines up.
fn right(text: &str, width: usize) -> String {
    let used = text.chars().count();
    let mut out = String::new();
    for _ in used..width {
        out.push(' ');
    }
    out.push_str(text);
    out
}

/// A count of blocks, written so that one of them is not "1 blocks".
fn blocks(count: u64, language: Language) -> String {
    plural(count, Msg::UnitBlock, Msg::UnitBlocks, language)
}

/// A count with the right one of two words after it.
fn plural(count: u64, one: Msg, many: Msg, language: Language) -> String {
    format!("{count} {}", if count == 1 { one } else { many }.text(language))
}

/// A count of hashes, with its unit.
fn whole(value: f64, unit: &str, language: Language) -> String {
    if value.is_finite() && value >= 0.0 {
        format!("{} {unit}", format::count(value as u64))
    } else {
        Msg::Unavailable.text(language).to_string()
    }
}

/// A stretch of seconds as a span a reader can read. Nothing hashing means no answer, not a panic.
fn span(seconds: f64, language: Language) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return Msg::Unavailable.text(language).to_string();
    }
    format::duration(Duration::from_secs_f64(seconds.min(1e15)))
}

/// The first few characters of a hash, which is how a block is named in a list.
fn short_hash(hash: Hash256) -> String {
    let full = format!("{hash:?}");
    let mut out: String = full.chars().take(10).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use nmtk_kq::theme::{EXPLAIN_PERCENT, MIN_HEIGHT, MIN_WIDTH};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn session() -> Session {
        Session::new(&MachineProfile {
            logical_cores: 4,
            total_memory_bytes: 0,
            available_memory_bytes: 0,
        })
    }

    /// Draws the quest's panel exactly where the shell puts it on the smallest screen nmtk allows.
    fn draw(session: &Session) -> String {
        let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).expect("backend");
        terminal
            .draw(|frame| {
                let [_, body, _] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .areas(frame.area());
                let [_, panel] = Layout::horizontal([
                    Constraint::Percentage(EXPLAIN_PERCENT),
                    Constraint::Percentage(100 - EXPLAIN_PERCENT),
                ])
                .areas(body);
                session.render(frame, panel, Theme::new(true), Language::English);
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
    fn the_brief_draws_the_puzzle_and_what_a_block_costs_at_eighty_by_twenty_four() {
        let text = draw(&session());
        assert!(text.contains("The puzzle"), "no panel title:\n{text}");
        assert!(text.contains("SHA-256"), "the hash is not named:\n{text}");
        // A block at difficulty 1 costs 2^256/(target+1) hashes, and that number — the one the
        // engine works out from the target itself — is the whole point of the screen.
        assert!(text.contains("4,295,032,833"), "difficulty 1 is not costed:\n{text}");
    }

    #[test]
    fn the_run_screen_offers_to_start_and_names_the_figure_it_compares_against() {
        let mut session = session();
        session.go_to(StageKind::Run);
        let text = draw(&session);
        assert!(text.contains("Press Enter"), "nothing invites a run:\n{text}");
        assert!(text.contains("Bitcoin, Jan 2009"), "no comparison offered:\n{text}");
    }

    #[test]
    fn tune_shows_the_share_that_was_asked_for_beside_the_share_that_was_got() {
        let mut session = session();
        session.go_to(StageKind::Tune);
        let text = draw(&session);
        assert!(text.contains("asked") && text.contains("got"), "no split table:\n{text}");
        // Three threads cannot be split 60/40, so the two numbers have to differ on screen.
        assert!(text.contains("60.0%"), "what was asked for is missing:\n{text}");
        assert!(text.contains("66.7%"), "what was got is missing:\n{text}");
        assert!(text.contains("threads are whole things"), "the reason is missing:\n{text}");
    }

    #[test]
    fn break_says_how_the_threads_are_split_before_anything_starts() {
        let mut session = session();
        session.go_to(StageKind::Break);
        assert_eq!(session.attack_threads(), (2, 1));
        let text = draw(&session);
        assert!(text.contains("Attacker's share"), "the knob is missing:\n{text}");
        assert!(text.contains("51.0%"), "the share is missing:\n{text}");
        assert!(text.contains("2 / 1"), "the thread split is missing:\n{text}");
    }

    #[test]
    fn the_recap_admits_what_has_not_been_run_instead_of_inventing_it() {
        let mut session = session();
        session.go_to(StageKind::Recap);
        let text = draw(&session);
        assert!(text.contains("What just happened"), "no panel title:\n{text}");
        assert!(text.contains("not run yet"), "an unrun stage was filled in:\n{text}");
        assert!(text.contains("7.16 MH/s"), "the 2009 figure is missing:\n{text}");
    }

    #[test]
    fn a_reader_can_jump_to_every_stage_and_each_one_draws_something() {
        let mut session = session();
        for stage in StageKind::ALL {
            session.on(Action::Stage(stage));
            assert_eq!(session.stage(), stage);
            let text = draw(&session);
            assert!(text.contains('╭'), "{stage:?} drew no panel:\n{text}");
            assert!(
                text.chars().filter(|c| c.is_alphanumeric()).count() > 40,
                "{stage:?} drew an empty panel:\n{text}"
            );
        }
    }

    #[test]
    fn every_stage_with_knobs_offers_them_and_the_others_offer_none() {
        let mut session = session();
        for stage in StageKind::ALL {
            session.go_to(stage);
            let expected = matches!(stage, StageKind::Tune | StageKind::Break);
            assert_eq!(!session.knobs().is_empty(), expected, "{stage:?}");
            assert_eq!(session.chosen_knob().is_some(), expected, "{stage:?}");
        }
    }

    #[test]
    fn a_typed_number_beats_the_presets_and_a_refused_one_changes_nothing() {
        let mut session = session();
        session.go_to(StageKind::Tune);
        for c in "24".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.zero_bits(), 24);
        for c in "99".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.zero_bits(), 24, "an out-of-range number was taken");
    }

    #[test]
    fn changing_the_miner_count_changes_how_many_shares_there_are_to_set() {
        let mut session = session();
        session.go_to(StageKind::Tune);
        assert_eq!(session.knobs().len(), KNOB_SHARES_FROM + 2);
        session.chosen = KNOB_MINERS;
        session.on(Action::Nudge(1));
        assert_eq!(session.miner_count(), 3);
        assert_eq!(session.knobs().len(), KNOB_SHARES_FROM + 3);
        session.on(Action::Nudge(-1));
        session.on(Action::Nudge(-1));
        assert_eq!(session.miner_count(), 1);
        assert_eq!(session.knobs().len(), KNOB_SHARES_FROM + 1);
    }

    #[test]
    fn the_split_is_worked_out_before_anything_is_started() {
        let session = session();
        let rows = session.split_rows().expect("three threads split between two miners");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.iter().map(|row| row.threads).sum::<usize>(), 3);
        assert!((rows[0].requested - 0.6).abs() < 1e-9);
        assert!(rows[0].effective > rows[0].requested, "the rounding went the other way");
    }

    #[test]
    fn asking_for_fewer_threads_than_miners_is_refused_in_words() {
        let mut session = session();
        session.go_to(StageKind::Tune);
        session.chosen = KNOB_THREADS;
        for c in "1".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert!(session.split_rows().is_err(), "one thread cannot carry two miners");
        let text = draw(&session);
        assert!(text.contains("Fewer threads than miners"), "no refusal on screen:\n{text}");
    }

    #[test]
    fn a_run_starts_real_threads_and_closing_brings_them_home() {
        let mut session = session();
        session.go_to(StageKind::Run);
        session.on(Action::Go);
        assert_eq!(session.run_state(), RunState::Running);
        session.tick();
        assert!(session.mining_snapshot.is_some(), "no snapshot to draw from");
        session.close();
        assert!(session.mining.is_none(), "a worker was left running");
        assert_eq!(session.run_state(), RunState::Idle);
    }
}
