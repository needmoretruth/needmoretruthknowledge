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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn truncation_never_exceeds_the_width_it_was_given() {
        assert!(width(&truncate("Zero-knowledge proofs", 10)) <= 10);
        assert!(width(&truncate("영지식 증명 퀘스트", 9)) <= 9);
        assert_eq!(truncate("short", 10), "short");
    }
}
