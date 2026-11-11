//! Text normalisation for matching Persian/English keywords in OCR output.

/// Folds the spelling variants OCR engines produce for Persian text:
/// Arabic `ي`/`ك` become Persian `ی`/`ک`; ZWNJ, bidi marks, tatweel and
/// diacritics are dropped; NBSP becomes a space. Case is left untouched.
pub fn normalize(s: &str) -> String {
    s.chars().filter_map(normalize_char).collect()
}

/// [`normalize`] for one char; `None` when the char is dropped.
pub fn normalize_char(c: char) -> Option<char> {
    match c {
        'ي' | 'ى' => Some('ی'),
        'ك' => Some('ک'),
        '\u{200C}' | '\u{200D}' | '\u{200E}' | '\u{200F}' | '\u{0640}' => None,
        '\u{064B}'..='\u{0655}' | '\u{0670}' => None,
        '\u{00A0}' | '\u{202F}' => Some(' '),
        c => Some(c),
    }
}

/// Persian and Arabic-Indic digits as ASCII, so `۳.۲` and `3.2` compare equal.
pub fn fold_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '۰'..='۹' => char::from(b'0' + (c as u32 - '۰' as u32) as u8),
            '٠'..='٩' => char::from(b'0' + (c as u32 - '٠' as u32) as u8),
            '–' | '/' => '-',
            c => c,
        })
        .collect()
}

/// One-to-one lowercase, so char indices stay aligned with the input.
pub fn lower_char(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// [`normalize`] followed by lowercasing: the form all lexicon entries and
/// haystacks are compared in.
pub fn fold(s: &str) -> String {
    normalize(s).chars().map(lower_char).collect()
}

/// Whole-word containment: the match may not touch a letter or digit on
/// either side, so `لم` does not match inside `علم` or `لمس`.
pub fn contains_word(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    hay.match_indices(needle).any(|(i, m)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + m.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// The last `n` chars of `s`.
pub fn tail(s: &str, n: usize) -> &str {
    match s.char_indices().rev().nth(n.saturating_sub(1)) {
        Some((i, _)) => &s[i..],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_arabic_letters_and_zwnj() {
        assert_eq!(normalize("مي‌ناميم"), "مینامیم");
        assert_eq!(normalize("كتاب"), "کتاب");
        assert_eq!(fold("Theorem"), "theorem");
    }

    #[test]
    fn digits_fold_to_ascii() {
        assert_eq!(fold_digits("۳.۲"), "3.2");
        assert_eq!(fold_digits("٤–١"), "4-1");
    }

    #[test]
    fn word_boundaries_are_respected() {
        assert!(contains_word("طبق لم ۲ داریم", "لم"));
        assert!(!contains_word("علم ریاضی", "لم"));
        assert!(!contains_word("لمس کردن", "لم"));
        assert!(contains_word("by the lemma.", "lemma"));
        assert!(!contains_word("dilemmas", "lemma"));
    }

    #[test]
    fn tail_is_char_safe() {
        assert_eq!(tail("ثابت شد", 2), "شد");
        assert_eq!(tail("ab", 10), "ab");
    }
}
