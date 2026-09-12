//! How wide a piece of text is on a terminal.
//!
//! A Korean or Japanese glyph fills two cells, so `format!("{:<10}")` — which counts characters —
//! lines a table up in English and pulls it apart in Korean. Every quest that aligns a column uses
//! these instead.

/// Cells a character occupies: two for the wide ranges, one for everything else.
///
/// This covers the ranges nmtk actually shows — Hangul, CJK ideographs, kana, fullwidth forms and
/// emoji. It is not a full implementation of the Unicode width tables, and it does not try to be:
/// a quest that needs one should say so rather than widen this quietly.
pub fn char_width(c: char) -> usize {
    match c as u32 {
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// Cells a string occupies.
pub fn width(text: &str) -> usize {
    text.chars().map(char_width).sum()
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
pub fn truncate(text: &str, columns: usize) -> String {
    if width(text) <= columns {
        return text.to_string();
    }
    if columns == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let next = used + char_width(c);
        if next > columns.saturating_sub(1) {
            break;
        }
        out.push(c);
        used = next;
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
/// allowed to run off the edge. Korean and Japanese have no spaces to break on, so a run of wide
/// glyphs breaks wherever it must.
pub fn wrap(text: &str, columns: usize) -> Vec<String> {
    if columns == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;

    let mut flush = |line: &mut String, used: &mut usize| {
        lines.push(std::mem::take(line));
        *used = 0;
    };

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
            flush(&mut line, &mut used);
        }
        if word_width <= columns {
            line.push_str(word);
            used = word_width;
            continue;
        }
        // Longer than a whole line: break it wherever the width runs out.
        for c in word.chars() {
            let w = char_width(c);
            if used + w > columns {
                flush(&mut line, &mut used);
            }
            line.push(c);
            used += w;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
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
        let english = "Every chain has to answer one question: where is the money, and who says so?";
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
}
