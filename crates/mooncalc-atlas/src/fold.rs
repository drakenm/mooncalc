//! Search-key normalisation: case and diacritic folding.

use crate::fold_table::{EXT_ADD_START, LATIN, LATIN_EXT_ADDITIONAL, LATIN_START};

/// Normalises text for searching.
///
/// Lower-cases, strips diacritics from Latin letters (`São Paulo` →
/// `sao paulo`, `Hà Nội` → `ha noi`, `Straße` → `strasse`), deletes
/// apostrophes, turns every other non-alphanumeric character into a space, and
/// collapses runs of spaces. Letters of other scripts are lower-cased but
/// otherwise kept.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    // Start as if a space was just written, so leading separators are dropped.
    let mut pending_space = false;
    let push = |out: &mut String, piece: &str, pending: &mut bool| {
        if *pending && !out.is_empty() {
            out.push(' ');
        }
        *pending = false;
        out.push_str(piece);
    };
    let mut buf = [0u8; 4];
    for c in s.chars() {
        let cp = c as u32;
        if c.is_ascii_alphanumeric() {
            let lc = c.to_ascii_lowercase();
            push(&mut out, lc.encode_utf8(&mut buf), &mut pending_space);
        } else if matches!(c, '\'' | '’' | '‘' | 'ʻ' | 'ʼ' | 'ʿ' | 'ʾ' | '`' | '´') {
            // Apostrophes and ʻokina/ayn marks vanish: "N'Djamena" ~ "ndjamena".
        } else if (LATIN_START..LATIN_START + LATIN.len() as u32).contains(&cp) {
            let f = LATIN[(cp - LATIN_START) as usize];
            if f == " " {
                pending_space = true;
            } else {
                push(&mut out, f, &mut pending_space);
            }
        } else if (EXT_ADD_START..EXT_ADD_START + LATIN_EXT_ADDITIONAL.len() as u32).contains(&cp) {
            push(
                &mut out,
                LATIN_EXT_ADDITIONAL[(cp - EXT_ADD_START) as usize],
                &mut pending_space,
            );
        } else if c.is_alphanumeric() {
            // Combining marks are not alphanumeric, so they are skipped below.
            for lc in c.to_lowercase() {
                push(&mut out, lc.encode_utf8(&mut buf), &mut pending_space);
            }
        } else if is_combining_mark(c) {
            // Decomposed input ("a" + U+0301): drop the mark.
        } else {
            pending_space = true;
        }
    }
    out
}

/// The combining diacritical mark blocks.
fn is_combining_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// Whether `needle` occurs in `hay` at the start of a word (position 0 or
/// right after a space). Both are folded.
pub(crate) fn word_prefix(hay: &str, needle: &str) -> bool {
    if hay.starts_with(needle) {
        return true;
    }
    hay.match_indices(' ')
        .any(|(i, _)| hay[i + 1..].starts_with(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_diacritics_and_punctuation() {
        assert_eq!(fold("São Paulo"), "sao paulo");
        assert_eq!(fold("SÃO  PAULO"), "sao paulo");
        assert_eq!(fold("Hà Nội"), "ha noi");
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("Łódź"), "lodz");
        assert_eq!(fold("Reykjavík"), "reykjavik");
        assert_eq!(fold("N'Djamena"), "ndjamena");
        assert_eq!(fold("St. Louis"), "st louis");
        assert_eq!(fold("Winston-Salem"), "winston salem");
        assert_eq!(fold("  Cairo, Egypt "), "cairo egypt");
        assert_eq!(fold("Ağrı"), "agri");
        assert_eq!(fold("Москва"), "москва");
        // Decomposed input.
        assert_eq!(fold("Sa\u{0303}o"), "sao");
    }

    #[test]
    fn word_prefixes() {
        assert!(word_prefix("rio de janeiro", "jan"));
        assert!(word_prefix("rio de janeiro", "rio"));
        assert!(!word_prefix("rio de janeiro", "aneiro"));
    }
}
