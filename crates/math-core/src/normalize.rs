//! OCR input normalisation, run on the Markdown source before parsing.
//!
//! OCR engines (Mathpix and friends) emit LaTeX conventions that Markdown does
//! not understand: `\( … \)` delimiters, bare `\begin{align}` environments,
//! `\\` line breaks in prose, spaced-out bold markers and unbalanced `$`.
//! Every pass here is a pure text rewrite; code spans and fenced code are
//! never touched, and existing `$ … $` math is left as written.
//!
//! Byte offsets in [`crate::model::Node::source`] refer to the *normalised*
//! text when a document goes through [`crate::analyze`].

use std::collections::BTreeMap;

use crate::model::Direction;

/// Environments KaTeX renders as display math; wrapped in `$$` when found bare.
const MATH_ENVS: [&str; 4] = ["equation", "align", "alignat", "gather"];

/// Runs every pass, in order.
pub fn normalize_markdown(src: &str) -> String {
    let s = convert_delimiters(src);
    let s = wrap_environments(&s);
    let s = number_equations(&s);
    let s = fix_line_breaks(&s);
    let s = fix_bold_markers(&s);
    balance_dollars(&s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Zone {
    Text,
    Code,
    Math,
}

/// Classifies every byte of `src` as prose, code (fenced or inline) or
/// dollar-delimited math, following the rules the Markdown parser applies.
pub(crate) fn zones(src: &str) -> Vec<Zone> {
    let b = src.as_bytes();
    let n = b.len();
    let mut z = vec![Zone::Text; n];
    let mut i = 0;
    let mut line_start = true;
    while i < n {
        if line_start {
            line_start = false;
            if let Some(end) = fence_end(src, i) {
                z[i..end].fill(Zone::Code);
                i = end;
                line_start = true;
                continue;
            }
        }
        match b[i] {
            b'\n' => {
                line_start = true;
                i += 1;
            }
            b'\\' => i += if b.get(i + 1) == Some(&b'\n') { 1 } else { 2 },
            b'`' => {
                let run = b[i..].iter().take_while(|c| **c == b'`').count();
                match find_backticks(b, i + run, run, paragraph_end(b, i)) {
                    Some(end) => {
                        z[i..end].fill(Zone::Code);
                        i = end;
                    }
                    None => i += run,
                }
            }
            b'$' => {
                let stop = paragraph_end(b, i);
                let end = if b.get(i + 1) == Some(&b'$') {
                    find_bytes(b, i + 2, stop, b"$$").map(|j| j + 2)
                } else {
                    inline_math_end(b, i, stop)
                };
                match end {
                    Some(end) => {
                        z[i..end].fill(Zone::Math);
                        i = end;
                    }
                    None => i += if b.get(i + 1) == Some(&b'$') { 2 } else { 1 },
                }
            }
            _ => i += 1,
        }
    }
    z
}

/// If a fenced code block opens at line start `i`, the offset just past it.
fn fence_end(src: &str, i: usize) -> Option<usize> {
    let line_end = |from: usize| src[from..].find('\n').map_or(src.len(), |k| from + k + 1);
    let fence_of = |line: &str| -> Option<(u8, usize)> {
        let indent = line.len() - line.trim_start_matches(' ').len();
        let rest = &line[indent..];
        let c = *rest.as_bytes().first()?;
        let len = rest.bytes().take_while(|x| *x == c).count();
        (indent <= 3 && (c == b'`' || c == b'~') && len >= 3).then_some((c, len))
    };
    let first_end = line_end(i);
    let (c, len) = fence_of(&src[i..first_end])?;
    let mut at = first_end;
    while at < src.len() {
        let end = line_end(at);
        let line = &src[at..end];
        if let Some((c2, len2)) = fence_of(line)
            && c2 == c
            && len2 >= len
            && line.trim().bytes().all(|x| x == c)
        {
            return Some(end);
        }
        at = end;
    }
    Some(src.len())
}

/// Offset of the blank line that ends the paragraph containing `from`.
fn paragraph_end(b: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < b.len() {
        if b[i] == b'\n' {
            let mut j = i + 1;
            while j < b.len() && (b[j] == b' ' || b[j] == b'\t' || b[j] == b'\r') {
                j += 1;
            }
            if j >= b.len() || b[j] == b'\n' {
                return i;
            }
        }
        i += 1;
    }
    b.len()
}

fn find_bytes(b: &[u8], from: usize, stop: usize, needle: &[u8]) -> Option<usize> {
    let stop = stop.min(b.len());
    if from >= stop {
        return None;
    }
    b[from..stop]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|k| from + k)
}

/// End (exclusive) of a closing backtick run of exactly `run` backticks.
fn find_backticks(b: &[u8], from: usize, run: usize, stop: usize) -> Option<usize> {
    let mut i = from;
    while i < stop {
        if b[i] == b'`' {
            let len = b[i..stop].iter().take_while(|c| **c == b'`').count();
            if len == run {
                return Some(i + len);
            }
            i += len;
        } else {
            i += 1;
        }
    }
    None
}

/// End (exclusive) of `$…$` opening at `i`: the opener may not be followed by
/// whitespace, nor the closer preceded by it.
fn inline_math_end(b: &[u8], i: usize, stop: usize) -> Option<usize> {
    if b.get(i + 1).is_none_or(|c| c.is_ascii_whitespace()) {
        return None;
    }
    let mut j = i + 1;
    while j < stop {
        match b[j] {
            b'\\' => j += 2,
            b'$' if !b[j - 1].is_ascii_whitespace() => return Some(j + 1),
            _ => j += 1,
        }
    }
    None
}

fn drop_blank_lines(s: &str) -> String {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `\( … \)` → `$ … $` and `\[ … \]` → `$$ … $$`.
///
/// In Markdown both are also plain escapes (`\[1\]` is a literal "[1]"), so a
/// pair is only converted when what it encloses looks like mathematics.
pub fn convert_delimiters(src: &str) -> String {
    let z = zones(src);
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let (mut i, mut copied) = (0, 0);
    while i + 1 < b.len() {
        if z[i] != Zone::Text || b[i] != b'\\' {
            i += 1;
            continue;
        }
        let (close, display) = match b[i + 1] {
            b'(' => (b')', false),
            b'[' => (b']', true),
            _ => {
                i += 2;
                continue;
            }
        };
        let stop = if display {
            b.len()
        } else {
            paragraph_end(b, i)
        };
        let inner_end = closing_delimiter(b, i + 2, stop, close);
        let inner = inner_end.map(|j| src[i + 2..j].trim());
        match (inner_end, inner) {
            (Some(j), Some(inner)) if looks_like_math(inner, display) => {
                out.push_str(&src[copied..i]);
                if display {
                    out.push_str("$$");
                    out.push_str(&drop_blank_lines(inner));
                    out.push_str("$$");
                } else {
                    out.push('$');
                    out.push_str(inner);
                    out.push('$');
                }
                i = j + 2;
                copied = i;
            }
            _ => i += 2,
        }
    }
    out.push_str(&src[copied..]);
    out
}

fn closing_delimiter(b: &[u8], from: usize, stop: usize, close: u8) -> Option<usize> {
    let mut j = from;
    while j + 1 < stop.min(b.len()) {
        if b[j] == b'\\' {
            if b[j + 1] == close {
                return Some(j);
            }
            j += 2;
        } else {
            j += 1;
        }
    }
    None
}

fn looks_like_math(inner: &str, display: bool) -> bool {
    if inner.is_empty() {
        return false;
    }
    let symbolic = inner.contains(['\\', '^', '_', '=', '+', '<', '>', '|']);
    if display {
        // "\[1\]" and "\[Ser77\]" are citations, not equations.
        symbolic || inner.contains('\n')
    } else {
        // "\(الف\)" is an escaped parenthesis in Persian prose.
        symbolic || Direction::detect(inner) != Some(Direction::Rtl)
    }
}

/// `\begin{…}` at `i`: (environment name, offset after the closing brace).
pub(crate) fn begin_at(src: &str, i: usize) -> Option<(&str, usize)> {
    let rest = src[i..].strip_prefix("\\begin{")?;
    let close = rest.find('}')?;
    Some((&rest[..close], i + "\\begin{".len() + close + 1))
}

/// Wraps bare display environments (`equation`, `align`, `gather`, …) in `$$`.
pub fn wrap_environments(src: &str) -> String {
    let z = zones(src);
    let mut out = String::with_capacity(src.len() + 16);
    let (mut i, mut copied) = (0, 0);
    while let Some(k) = src[i..].find("\\begin{") {
        let at = i + k;
        i = at + 1;
        if z[at] != Zone::Text {
            continue;
        }
        let Some((name, after)) = begin_at(src, at) else {
            continue;
        };
        if !MATH_ENVS.contains(&name.trim_end_matches('*')) {
            continue;
        }
        let end_tag = format!("\\end{{{name}}}");
        let Some(e) = src[after..].find(&end_tag) else {
            continue;
        };
        let end = after + e + end_tag.len();
        out.push_str(&src[copied..at]);
        out.push_str("$$");
        out.push_str(&drop_blank_lines(&src[at..end]));
        out.push_str("$$");
        i = end;
        copied = end;
    }
    out.push_str(&src[copied..]);
    out
}

/// Numbers `equation` environments across the whole document.
///
/// KaTeX restarts its counter for every formula, so every equation would be
/// "(1)". Each unnumbered `equation` becomes `equation*` with an explicit
/// `\tag{n}`; `\label{k}` is removed, and `\eqref{k}` / `\ref{k}` are replaced
/// by the number.
pub fn number_equations(src: &str) -> String {
    const BEGIN: &str = "\\begin{equation}";
    const END: &str = "\\end{equation}";
    let z = zones(src);
    let mut labels: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = String::with_capacity(src.len());
    let (mut i, mut count) = (0, 0usize);
    while let Some(k) = src[i..].find(BEGIN) {
        let at = i + k;
        let body_start = at + BEGIN.len();
        let Some(e) = src[body_start..].find(END) else {
            break;
        };
        let body_end = body_start + e;
        out.push_str(&src[i..at]);
        i = body_end + END.len();
        let body = &src[body_start..body_end];
        if z[at] == Zone::Code
            || ["\\tag", "\\nonumber", "\\notag"]
                .iter()
                .any(|t| body.contains(t))
        {
            out.push_str(&src[at..i]);
            continue;
        }
        count += 1;
        let mut body = body.to_string();
        while let Some(l) = body.find("\\label{") {
            let Some(close) = body[l..].find('}') else {
                break;
            };
            labels.insert(
                body[l + "\\label{".len()..l + close].trim().to_string(),
                count,
            );
            body.replace_range(l..l + close + 1, "");
        }
        out.push_str("\\begin{equation*}");
        out.push_str(body.trim_end());
        out.push_str(&format!(" \\tag{{{count}}}\\end{{equation*}}"));
    }
    out.push_str(&src[i..]);

    for (label, number) in &labels {
        out = out
            .replace(&format!("\\eqref{{{label}}}"), &format!("({number})"))
            .replace(&format!("\\ref{{{label}}}"), &number.to_string());
    }
    out
}

/// A LaTeX `\\` at the end of a prose line: a Markdown hard break when the
/// paragraph goes on, nothing when it does not.
pub fn fix_line_breaks(src: &str) -> String {
    let z = zones(src);
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let (mut i, mut copied) = (0, 0);
    while i + 1 < b.len() {
        if z[i] != Zone::Text || b[i] != b'\\' {
            i += 1;
            continue;
        }
        if b[i + 1] != b'\\' {
            i += 2;
            continue;
        }
        let mut j = i + 2;
        while j < b.len() && (b[j] == b' ' || b[j] == b'\t' || b[j] == b'\r') {
            j += 1;
        }
        if j < b.len() && b[j] != b'\n' {
            i += 2;
            continue;
        }
        out.push_str(src[copied..i].trim_end_matches([' ', '\t']));
        let paragraph_goes_on = j < b.len() && paragraph_end(b, i) != j;
        if paragraph_goes_on && !src[copied..i].trim().is_empty() {
            out.push('\\');
        }
        i = j;
        copied = j;
    }
    out.push_str(&src[copied..]);
    out
}

/// Repairs emphasis markers the OCR spaced out: `* *قضیه* *` → `**قضیه**`,
/// `** قضیه ۱. **متن` → `**قضیه ۱.** متن`.
pub fn fix_bold_markers(src: &str) -> String {
    let z = zones(src);
    let mut out = String::with_capacity(src.len());
    let mut at = 0;
    for line in src.split_inclusive('\n') {
        let zone = &z[at..at + line.len()];
        at += line.len();
        if !line.contains('*') {
            out.push_str(line);
            continue;
        }
        let joined = join_split_stars(line, zone);
        match joined {
            // Zones are unchanged by the join only in shape, so recompute.
            Some(joined) => out.push_str(&tighten_bold(&joined, &zones(&joined))),
            None => out.push_str(&tighten_bold(line, zone)),
        }
    }
    out
}

/// Runs of unescaped `*` in prose: (offset, length).
fn star_runs(line: &str, zone: &[Zone]) -> Vec<(usize, usize)> {
    let b = line.as_bytes();
    let mut runs = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
        } else if b[i] == b'*' && zone[i] == Zone::Text {
            let len = b[i..].iter().take_while(|c| **c == b'*').count();
            runs.push((i, len));
            i += len;
        } else {
            i += 1;
        }
    }
    runs
}

/// `* *X* *` → `**X**`. Needs both halves on the line, so the bullet in
/// `* *emphasised* item` is left alone.
fn join_split_stars(line: &str, zone: &[Zone]) -> Option<String> {
    let runs = star_runs(line, zone);
    let b = line.as_bytes();
    // Pairs of single stars separated by exactly one space.
    let mut split = Vec::new();
    let mut k = 0;
    while k + 1 < runs.len() {
        let ((a, la), (c, lc)) = (runs[k], runs[k + 1]);
        if la == 1 && lc == 1 && c == a + 2 && b[a + 1] == b' ' {
            split.push(a);
            k += 2;
        } else {
            k += 1;
        }
    }
    if split.len() < 2 || !split.len().is_multiple_of(2) {
        return None;
    }
    let mut out = String::with_capacity(line.len());
    let mut copied = 0;
    for a in split {
        out.push_str(&line[copied..a]);
        out.push_str("**");
        copied = a + 3;
    }
    out.push_str(&line[copied..]);
    Some(out)
}

/// Moves whitespace out of `** X **` so the markers flank the text, and adds
/// the space Markdown needs after `**…punctuation**letter`.
fn tighten_bold(line: &str, zone: &[Zone]) -> String {
    let runs: Vec<usize> = star_runs(line, zone)
        .into_iter()
        .filter(|(_, len)| *len == 2)
        .map(|(at, _)| at)
        .collect();
    if runs.is_empty() || !runs.len().is_multiple_of(2) {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len() + 4);
    let mut copied = 0;
    for pair in runs.chunks(2) {
        let (open, close) = (pair[0], pair[1]);
        let inner = &line[open + 2..close];
        let trimmed = inner.trim();
        if trimmed.is_empty() || trimmed.contains('*') {
            continue;
        }
        let lead = &inner[..inner.len() - inner.trim_start().len()];
        let trail = &inner[inner.trim_end().len()..];
        let after = line[close + 2..].chars().next();
        let ends_in_punctuation = trimmed
            .chars()
            .next_back()
            .is_some_and(|c| !c.is_alphanumeric());
        let needs_space =
            trail.is_empty() && ends_in_punctuation && after.is_some_and(char::is_alphanumeric);
        if lead.is_empty() && trail.is_empty() && !needs_space {
            continue;
        }
        out.push_str(&line[copied..open]);
        out.push_str(lead);
        out.push_str("**");
        out.push_str(trimmed);
        out.push_str("**");
        out.push_str(trail);
        if needs_space {
            out.push(' ');
        }
        copied = close + 2;
    }
    out.push_str(&line[copied..]);
    out
}

/// Deals with `$` that pair with nothing: an opening `$$` with a formula after
/// it is closed at the end of its paragraph, a dangling `$$` is dropped, and a
/// lone `$` is escaped so it cannot swallow later text.
pub fn balance_dollars(src: &str) -> String {
    let z = zones(src);
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len() + 8);
    let (mut i, mut copied) = (0, 0);
    while i < b.len() {
        if z[i] != Zone::Text {
            i += 1;
            continue;
        }
        match b[i] {
            b'\\' => i += 2,
            b'$' if b.get(i + 1) == Some(&b'$') => {
                let stop = paragraph_end(b, i);
                out.push_str(&src[copied..i]);
                let body = &src[i + 2..stop];
                if body.trim().is_empty() {
                    copied = i + 2;
                    i = copied;
                } else {
                    out.push_str("$$");
                    out.push_str(body.trim_end());
                    out.push_str("$$");
                    copied = stop;
                    i = stop;
                }
            }
            b'$' => {
                out.push_str(&src[copied..i]);
                out.push_str("\\$");
                i += 1;
                copied = i;
            }
            _ => i += 1,
        }
    }
    out.push_str(&src[copied.min(src.len())..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mathpix_delimiters_become_dollars() {
        assert_eq!(
            convert_delimiters(r"اگر \( x > 0 \) آنگاه \[ x^2 > 0. \]"),
            "اگر $x > 0$ آنگاه $$x^2 > 0.$$"
        );
        assert_eq!(
            convert_delimiters(r"Let \(G\) be a group."),
            "Let $G$ be a group."
        );
        // A `\\` line break inside the formula does not hide the closer.
        assert_eq!(
            convert_delimiters("\\[\na &= b \\\\\nc &= d\n\\]"),
            "$$a &= b \\\\\nc &= d$$"
        );
    }

    #[test]
    fn escapes_that_are_not_math_are_kept() {
        for src in [
            r"see \[1\] and \[Ser77\]",
            r"مورد \(الف\) را ببینید",
            r"`\(x\)` and $a \( b \) c$",
            "```\n\\(x\\)\n```\n",
            r"a double backslash \\(x\\) is not a delimiter",
        ] {
            assert_eq!(convert_delimiters(src), src);
        }
    }

    #[test]
    fn bare_environments_are_wrapped_once() {
        let src = "متن\n\n\\begin{align}\na &= b \\\\\n\nc &= d\n\\end{align}\n\nمتن";
        let out = wrap_environments(src);
        assert_eq!(
            out,
            "متن\n\n$$\\begin{align}\na &= b \\\\\nc &= d\n\\end{align}$$\n\nمتن"
        );
        assert_eq!(wrap_environments(&out), out);
        let inside = "$$\\begin{gather*}x\\end{gather*}$$ and \\begin{itemize}";
        assert_eq!(wrap_environments(inside), inside);
    }

    #[test]
    fn equations_are_numbered_and_references_resolved() {
        let src = "$$\\begin{equation}\\label{eq:a} x = 1\\end{equation}$$\n\n\
                   $$\\begin{equation} y = 2 \\tag{$*$}\\end{equation}$$\n\n\
                   $$\\begin{equation}z = 3\\label{eq:b}\\end{equation}$$\n\n\
                   By \\eqref{eq:a} and equation \\ref{eq:b}.";
        let out = number_equations(src);
        assert!(out.contains("\\begin{equation*} x = 1 \\tag{1}\\end{equation*}"));
        assert!(out.contains("y = 2 \\tag{$*$}\\end{equation}"));
        assert!(out.contains("\\begin{equation*}z = 3 \\tag{2}"));
        assert!(out.ends_with("By (1) and equation 2."));
        assert!(!out.contains("\\label"));
    }

    #[test]
    fn stray_line_breaks_in_prose() {
        assert_eq!(
            fix_line_breaks("خط اول \\\\\nخط دوم \\\\\n\nبعد"),
            "خط اول\\\nخط دوم\n\nبعد"
        );
        let math = "$$a \\\\\nb$$ and `x \\\\`\n";
        assert_eq!(fix_line_breaks(math), math);
        assert_eq!(fix_line_breaks("a \\\\ b"), "a \\\\ b");
    }

    #[test]
    fn spaced_bold_markers_are_repaired() {
        assert_eq!(fix_bold_markers("* *قضیه ۱.* * اگر"), "**قضیه ۱.** اگر");
        assert_eq!(fix_bold_markers("** قضیه ۱. **متن"), " **قضیه ۱.** متن");
        assert_eq!(
            fix_bold_markers("**Theorem 1.**Let $G$"),
            "**Theorem 1.** Let $G$"
        );
        for src in [
            "* *emphasised* item",
            "**Theorem 1.** Let $a * b ** c$.",
            "**a** and **b**",
            "`** x **`",
            "a ** b",
        ] {
            assert_eq!(fix_bold_markers(src), src);
        }
    }

    #[test]
    fn unbalanced_dollars() {
        assert_eq!(balance_dollars("$$\nx = 1\n\nمتن"), "$$\nx = 1$$\n\nمتن");
        assert_eq!(
            balance_dollars("$$x = 1$$\n$$\n\nمتن"),
            "$$x = 1$$\n\n\nمتن"
        );
        assert_eq!(balance_dollars("اگر $x > 0$ و $y"), "اگر $x > 0$ و \\$y");
        let fine = "$a$ و $$b$$ و \\$5 و `$`";
        assert_eq!(balance_dollars(fine), fine);
    }

    #[test]
    fn whole_pipeline_is_idempotent() {
        let src = "* *قضیه ۲.* * اگر \\(f\\) پیوسته باشد آنگاه \\\\\n\\[ \\int_a^b f = 0 \\]\n\n\
                   \\begin{equation}\\label{e} a = b \\end{equation}\n\nطبق \\eqref{e} و $x";
        let once = normalize_markdown(src);
        assert_eq!(
            once,
            "**قضیه ۲.** اگر $f$ پیوسته باشد آنگاه\\\n$$\\int_a^b f = 0$$\n\n\
             $$\\begin{equation*} a = b \\tag{1}\\end{equation*}$$\n\nطبق (1) و \\$x"
        );
        assert_eq!(normalize_markdown(&once), once);
    }
}
