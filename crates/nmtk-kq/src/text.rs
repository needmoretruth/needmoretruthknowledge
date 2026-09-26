//! How wide a piece of text is on a terminal.
//!
//! A Korean or Japanese glyph fills two cells, so `format!("{:<10}")` — which counts characters —
//! lines a table up in English and pulls it apart in Korean. Every quest that aligns a column uses
//! these instead.
//!
//! The unit everything here works in is the **grapheme**: what a reader sees as one character.
//! `é` written as `e` plus a combining accent is two `char`s and one grapheme, a flag is two
//! `char`s and one grapheme, and a family emoji is seven. Cutting between the `char`s of one
//! grapheme leaves a bare accent or half a flag on screen, so nothing here ever does.
//!
//! Widths are the drawing library's own, not a table kept beside it. The first version of this
//! module had its own list of wide ranges, and wherever it disagreed with the library — `⛏`, `🚀`,
//! a combining accent, a zero-width joiner — a column that measured right here was drawn one cell
//! off there. Asking the library the same question it asks when it draws is the only way the two
//! can never disagree.

use ratatui::buffer::CellWidth;
use ratatui::text::Span;

/// Cells a character occupies on its own: two for wide glyphs, none for combining marks,
/// zero-width characters and control characters, one for everything else.
///
/// A combining mark has no width of its own because it sits on the character before it, and a
/// control character is never drawn at all — the drawing library throws it away.
pub fn char_width(c: char) -> usize {
    if c.is_control() {
        return 0;
    }
    let mut buffer = [0u8; 4];
    let text: &str = c.encode_utf8(&mut buffer);
    usize::from(text.cell_width())
}

/// The graphemes of `text` as the drawing library will draw them: each one's byte range in
/// `text` and the cells it takes. Graphemes the library drops — control characters, and anything
/// with no width of its own — are left out, exactly as they are left off the screen.
fn graphemes(text: &str) -> Vec<(usize, usize, usize)> {
    let span = Span::raw(text);
    let base = text.as_ptr() as usize;
    span.styled_graphemes(ratatui::style::Style::default())
        .filter_map(|grapheme| {
            let symbol = grapheme.symbol;
            let cells = usize::from(symbol.cell_width());
            // Byte offset of this grapheme in `text`: the library hands back slices of the text
            // it was given, so the distance between the two starts is the offset.
            let start = symbol.as_ptr() as usize - base;
            (cells > 0).then_some((start, symbol.len(), cells))
        })
        .collect()
}

/// Cells a string occupies.
pub fn width(text: &str) -> usize {
    // The common case — plain ASCII with no control characters — needs no segmentation at all.
    if text.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        return text.len();
    }
    graphemes(text).iter().map(|(_, _, cells)| cells).sum()
}

/// `text` padded with spaces to `columns` cells. Text already that wide is returned unchanged.
pub fn pad(text: &str, columns: usize) -> String {
    let used = width(text);
    let mut out = text.to_string();
    for _ in used..columns {
        out.push(' ');
    }
    out
}

/// `text` pushed to the right of `columns` cells, so numbers line up under their heading.
pub fn rpad(text: &str, columns: usize) -> String {
    let used = width(text);
    let mut out = String::new();
    for _ in used..columns {
        out.push(' ');
    }
    out.push_str(text);
    out
}

/// `text` as one column of a table: exactly `columns` cells, whatever it started as.
///
/// This is what a column needs and [`pad`] is not. `pad` widens but never narrows, so a label
/// longer than its column runs straight into the value beside it — in Korean, where every glyph is
/// two cells, that happens to labels that fit comfortably in English. A column that cannot hold
/// its text loses the tail of the text, never the gap after it.
pub fn column(text: &str, columns: usize) -> String {
    pad(&truncate(text, columns), columns)
}

/// `text` cut to at most `columns` cells, ending in `…` when something was removed.
///
/// The cut falls between graphemes, never inside one, so an accent is never left without its
/// letter and a wide glyph is never drawn half in the column.
pub fn truncate(text: &str, columns: usize) -> String {
    if width(text) <= columns {
        return text.to_string();
    }
    if columns == 0 {
        return String::new();
    }
    // One cell is kept for the mark that says something was cut.
    let room = columns - 1;
    let mut out = String::new();
    let mut used = 0;
    for (start, len, cells) in graphemes(text) {
        if used + cells > room {
            break;
        }
        out.push_str(&text[start..start + len]);
        used += cells;
    }
    out.push('…');
    out
}

/// Breaks `text` into lines no wider than `columns` cells.
///
/// Wrapping is done here rather than by the drawing library because the conversation has to know
/// how many lines it occupies before it draws them — that is what makes scrolling to the newest
/// beat exact rather than approximate.
///
/// Words are kept whole where they fit. A word longer than the whole width is cut rather than
/// allowed to run off the edge, and the cut falls between graphemes. Korean and Japanese have no
/// spaces to break on, so a run of wide glyphs breaks wherever it must. A line break in the text
/// is kept as a line break: the drawing library drops the character, and without this the two
/// lines it separated were drawn run together.
pub fn wrap(text: &str, columns: usize) -> Vec<String> {
    if columns == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        wrap_paragraph(paragraph.trim_end_matches('\r'), columns, &mut lines);
    }
    // A text that ended in a line break does not owe the reader an empty line after it.
    while lines.len() > 1 && lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// One paragraph of [`wrap`]: text with no line breaks in it. Always adds at least one line, so a
/// blank line in the text stays a blank line on screen.
fn wrap_paragraph(text: &str, columns: usize, lines: &mut Vec<String>) {
    let mut line = String::new();
    let mut used = 0;
    let first = lines.len();

    for word in text.split(' ') {
        if word.is_empty() {
            continue;
        }
        let word_width = width(word);
        // A space before the word, unless the line is empty.
        let gap = usize::from(!line.is_empty());
        if used + gap + word_width <= columns {
            if gap == 1 {
                line.push(' ');
            }
            line.push_str(word);
            used += gap + word_width;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        if word_width <= columns {
            line.push_str(word);
            used = word_width;
            continue;
        }
        // Longer than a whole line: break it wherever the width runs out, between graphemes.
        for (start, len, cells) in graphemes(word) {
            // A glyph wider than the whole line cannot be drawn in it at all. The drawing library
            // would drop it too; dropping it here keeps every line inside its width.
            if cells > columns {
                continue;
            }
            if used + cells > columns {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            line.push_str(&word[start..start + len]);
            used += cells;
        }
    }
    if !line.is_empty() || lines.len() == first {
        lines.push(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this exists for: `pad` widens and never narrows, so a label wider than its column
    /// collides with the value after it. Korean reaches that width on labels English never does.
    #[test]
    fn a_column_is_exactly_as_wide_as_it_was_asked_for() {
        assert_eq!(width(&column("hash that skips the commitment", 20)), 20);
        assert_eq!(width(&column("알게 되는 것", 7)), 7);
        assert_eq!(width(&column("가진 것", 7)), 7);
        assert_eq!(width(&column("ok", 7)), 7);
        // pad, on its own, does not: this is the collision.
        assert!(width(&pad("알게 되는 것", 7)) > 7);
    }

    #[test]
    fn a_column_never_swallows_the_gap_after_it() {
        // Two columns drawn side by side stay two columns, in either language.
        for label in ["hash that skips the commitment", "커밋먼트를 건너뛴 해시"] {
            let row = format!("{}{}", column(label, 22), "rejected");
            assert!(row.contains(' '), "{row:?} has no gap between the two columns");
        }
    }

    #[test]
    fn a_korean_column_is_padded_by_cells_rather_than_characters() {
        // Five characters, ten cells. Padding by characters leaves it five columns short and
        // every value after it in the row lands in the wrong place.
        assert_eq!(width(&pad("영지식증명", 14)), 14);
        assert_eq!(width(&rpad("영지식증명", 14)), 14);
        assert!(rpad("96 B", 8).starts_with("    "));
    }

    #[test]
    fn korean_glyphs_count_as_two_cells() {
        assert_eq!(width("계정"), 4);
        assert_eq!(width("Account"), 7);
    }

    #[test]
    fn padding_lines_both_languages_up_to_the_same_column() {
        assert_eq!(width(&pad("계정", 10)), 10);
        assert_eq!(width(&pad("Account", 10)), 10);
    }

    #[test]
    fn text_already_wide_enough_is_left_alone() {
        assert_eq!(pad("Zero-knowledge", 4), "Zero-knowledge");
    }

    #[test]
    fn wrapping_never_exceeds_the_width_it_was_given() {
        let english =
            "Every chain has to answer one question: where is the money, and who says so?";
        for line in wrap(english, 28) {
            assert!(width(&line) <= 28, "line too wide: {line:?}");
        }
        let korean = "모든 체인은 한 가지 질문에 답해야 합니다. 돈이 어디에 있는가?";
        for line in wrap(korean, 28) {
            assert!(width(&line) <= 28, "line too wide: {line:?}");
        }
    }

    #[test]
    fn wrapping_keeps_words_whole() {
        let lines = wrap("one two three four", 9);
        assert_eq!(lines, vec!["one two", "three", "four"]);
    }

    #[test]
    fn a_word_longer_than_the_line_is_cut_rather_than_lost() {
        let lines = wrap("supercalifragilistic", 8);
        assert!(lines.len() > 1);
        assert_eq!(lines.concat(), "supercalifragilistic");
    }

    #[test]
    fn wrapping_empty_text_still_gives_one_line() {
        assert_eq!(wrap("", 20), vec![String::new()]);
    }

    #[test]
    fn truncation_never_exceeds_the_width_it_was_given() {
        assert!(width(&truncate("Zero-knowledge proofs", 10)) <= 10);
        assert!(width(&truncate("영지식 증명 퀘스트", 9)) <= 9);
        assert_eq!(truncate("short", 10), "short");
    }

    /// Every string here is measured by drawing it and counting the cells the drawing library
    /// actually filled, so the test cannot agree with a mistake in this module.
    fn drawn_width(text: &str) -> usize {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        let mut buffer = Buffer::empty(Rect::new(0, 0, 200, 1));
        let (end, _) = buffer.set_stringn(0, 0, text, 200, ratatui::style::Style::default());
        usize::from(end)
    }

    /// Every byte offset at which the drawing library would break `text` between graphemes.
    fn boundaries(text: &str) -> Vec<usize> {
        let span = Span::raw(text);
        let base = text.as_ptr() as usize;
        let mut ends = vec![0];
        ends.extend(
            span.styled_graphemes(ratatui::style::Style::default())
                .map(|g| g.symbol.as_ptr() as usize - base + g.symbol.len()),
        );
        ends
    }

    const AWKWARD: [&str; 10] = [
        "e\u{301}te\u{301}",        // accents written as separate combining marks
        "\u{26cf} Proof of work",   // the pick on the shelf, which the old table called narrow
        "\u{1f680} launch",         // an emoji the old table did not list
        "\u{1f1f0}\u{1f1f7} Korea", // a flag: two code points, one glyph
        "a\u{200b}b\u{200d}c",      // zero-width space and joiner
        "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} family",
        "tab\there", // a control character the library never draws
        "한국어 English 日本語",
        "ｆｕｌｌｗｉｄｔｈ",
        "plain ascii",
    ];

    #[test]
    fn width_agrees_with_what_the_drawing_library_draws() {
        for text in AWKWARD {
            assert_eq!(width(text), drawn_width(text), "{text:?} is measured wrong");
        }
    }

    #[test]
    fn combining_and_zero_width_characters_take_no_cells() {
        assert_eq!(width("e\u{301}"), 1);
        assert_eq!(char_width('\u{301}'), 0);
        assert_eq!(char_width('\u{200b}'), 0);
        assert_eq!(char_width('\n'), 0);
        assert_eq!(char_width('한'), 2);
        assert_eq!(char_width('a'), 1);
    }

    #[test]
    fn truncation_never_splits_a_grapheme_or_overruns() {
        for text in AWKWARD {
            for columns in 0..=width(text) + 1 {
                let cut = truncate(text, columns);
                assert!(width(&cut) <= columns, "{text:?} at {columns}: {cut:?} is too wide");
                // What was kept is whole graphemes from the front of the text: its length is one
                // of the places the drawing library would have broken the text itself.
                let kept: String =
                    cut.trim_end_matches('…').chars().filter(|c| !c.is_control()).collect();
                let kept = kept.as_str();
                let drawn: String = text.chars().filter(|c| !c.is_control()).collect();
                assert!(drawn.starts_with(kept), "{text:?} at {columns}: {cut:?} is not a prefix");
                assert!(
                    boundaries(&drawn).contains(&kept.len()),
                    "{text:?} at {columns}: cut inside a grapheme: {cut:?}"
                );
            }
        }
        // The accent stays on its letter or goes with it.
        assert_eq!(truncate("e\u{301}e\u{301}e\u{301}", 2), "e\u{301}…");
        // Half a flag is never drawn.
        assert_eq!(truncate("\u{1f1f0}\u{1f1f7}\u{1f1ef}\u{1f1f5}", 3), "\u{1f1f0}\u{1f1f7}…");
    }

    #[test]
    fn wrapping_never_splits_a_grapheme_or_overruns() {
        for text in AWKWARD {
            for columns in 1..=12 {
                let lines = wrap(text, columns);
                for line in &lines {
                    assert!(width(line) <= columns, "{text:?} at {columns}: {line:?} too wide");
                    let whole = boundaries(line);
                    assert!(
                        whole.contains(&line.len())
                            && !line
                                .starts_with(|c| matches!(c, '\u{300}'..='\u{36f}' | '\u{200d}')),
                        "{text:?} at {columns}: a line holds half a grapheme: {line:?}"
                    );
                }
            }
        }
        assert_eq!(wrap("e\u{301}e\u{301}e\u{301}", 2), vec!["e\u{301}e\u{301}", "e\u{301}"]);
    }

    #[test]
    fn a_wide_glyph_never_lands_in_a_one_cell_line() {
        for line in wrap("한국어", 1) {
            assert!(width(&line) <= 1, "{line:?}");
        }
        assert_eq!(wrap("한국어", 3), vec!["한", "국", "어"]);
    }

    #[test]
    fn a_line_break_in_the_text_is_kept() {
        assert_eq!(wrap("first line\nsecond", 40), vec!["first line", "second"]);
        assert_eq!(wrap("a\n\nb", 40), vec!["a", "", "b"]);
        assert_eq!(wrap("trailing\n", 40), vec!["trailing"]);
    }

    #[test]
    fn columns_hold_their_width_for_awkward_text() {
        for text in AWKWARD {
            for columns in 0..20 {
                assert_eq!(width(&column(text, columns)), columns, "{text:?} at {columns}");
            }
        }
    }
}
