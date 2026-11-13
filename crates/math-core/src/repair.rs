//! Repairs Markdown whose inline formulas lost their `$ … $`.
//!
//! Some transcriptions (and some copy-paste routes out of a chat window or a
//! note-taking app) keep display math intact but drop the delimiters of inline
//! math, leaving bare LaTeX in the prose: `اگر P(B) \neq 0 آن‌گاه`. KaTeX then
//! never sees those formulas. [`wrap_bare_math`] finds such runs and wraps
//! them; [`repair_markdown`] is the whole "Fix Markdown" feature.
//!
//! A run is a stretch of prose made only of mathematical material (LaTeX
//! commands, single letters, numbers, operators, brackets). It is wrapped only
//! when there is evidence that it is a formula:
//!
//! * a LaTeX command or a sub/superscript (`\emptyset`, `A^c`, `S_X`);
//! * a relation between operands (`x = 5`, `t < 10`);
//! * a function application (`P(E)`, `F(x)`);
//! * in right-to-left text only, a single Latin letter standing alone
//!   (`فضای نمونه S`) or a numeric interval (`[0, 1]`).
//!
//! English words are never part of a run, so parenthesised glosses such as
//! `(Sample Space)` and the article in `(Value of a Function)` are left alone.
//! Code, existing math, links and HTML are never touched, and the pass is
//! idempotent.

use crate::normalize::{
    Zone, balance_dollars, convert_delimiters, fix_bold_markers, fix_line_breaks,
    unwrap_outer_fence, wrap_environments, zones,
};

/// What [`repair_markdown`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repair {
    /// The repaired Markdown.
    pub text: String,
    /// Bare inline formulas that were wrapped in `$ … $`.
    pub formulas: usize,
    /// Whether `text` differs from the input at all.
    pub changed: bool,
}

/// Fixes everything Mathia knows how to fix in a Markdown source, for saving
/// back to disk: a code fence around the whole document, `\( \)` and `\[ \]` delimiters, bare display environments,
/// stray `\\`, spaced-out bold markers, unbalanced `$`, and inline formulas
/// without delimiters. Equation numbering is left to the compiler.
pub fn repair_markdown(src: &str) -> Repair {
    let s = convert_delimiters(unwrap_outer_fence(src));
    let s = wrap_environments(&s);
    let s = fix_line_breaks(&s);
    let s = fix_bold_markers(&s);
    let s = balance_dollars(&s);
    let (text, formulas) = wrap_counted(&s);
    Repair {
        changed: text != src,
        text,
        formulas,
    }
}

/// Wraps bare inline formulas in `$ … $`.
pub fn wrap_bare_math(src: &str) -> String {
    wrap_counted(src).0
}

/// LaTeX commands that are not mathematics.
const NOT_MATH: [&str; 27] = [
    "begin",
    "end",
    "label",
    "ref",
    "eqref",
    "cite",
    "footnote",
    "item",
    "part",
    "chapter",
    "section",
    "subsection",
    "subsubsection",
    "paragraph",
    "textbf",
    "textit",
    "emph",
    "underline",
    "href",
    "url",
    "includegraphics",
    "caption",
    "newline",
    "par",
    "noindent",
    "tag",
    "newpage",
];

/// Commands that take the delimiter after them: `\left(`, `\bigr]`.
const SIZED: [&str; 14] = [
    "left", "right", "middle", "big", "Big", "bigg", "Bigg", "bigl", "bigr", "Bigl", "Bigr",
    "biggl", "biggr", "bigm",
];

/// Lower-case words that are mathematics when they stand in a formula.
const MATH_WORDS: [&str; 24] = [
    "sin", "cos", "tan", "cot", "sec", "csc", "log", "ln", "exp", "lim", "max", "min", "sup",
    "inf", "det", "dim", "ker", "gcd", "lcm", "mod", "arg", "deg", "sinh", "cosh",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Anything that cannot be in a formula: punctuation, other scripts,
    /// code, existing math, protected text.
    Break,
    /// A Latin word. Ends a run like `Break`, and also tells a neighbouring
    /// single letter that it is part of English prose.
    Word,
    Space,
    /// A LaTeX command with its braced arguments.
    Command,
    /// `^…` or `_…` attached to what precedes it.
    Script,
    Number,
    /// A single letter.
    Letter,
    /// A short all-capitals token (`HH`, `CDF`): may sit inside a formula,
    /// never proves one.
    Capitals,
    /// `sin`, `lim`, `dx`.
    MathWord,
    Open,
    Close,
    /// `=`, `<`, `>`, `+`: evidence when it joins two operands.
    Relation,
    /// `-`, `/`, `,`, `:`, `|`, `\,`, `\quad`.
    Operator,
    /// `!` or `'` directly after an operand.
    Postfix,
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

fn wrap_counted(src: &str) -> (String, usize) {
    let z = zones(src);
    let doc_rtl = mostly_rtl(src);
    let skip = front_matter_end(src);

    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut at = skip;
    for line in src[skip..].split_inclusive('\n') {
        lines.push((at, line));
        at += line.len();
    }
    let table = table_rows(&lines);

    let mut out = String::with_capacity(src.len() + src.len() / 16);
    out.push_str(&src[..skip]);
    let mut count = 0;
    for (index, (offset, line)) in lines.iter().enumerate() {
        let zone = &z[*offset..*offset + line.len()];
        let spans = if is_delimiter_row(line.trim()) || is_link_definition(line) {
            Vec::new()
        } else {
            formula_spans(line, zone, table[index], doc_rtl)
        };
        let mut copied = 0;
        for (start, end) in spans {
            out.push_str(&line[copied..start]);
            out.push('$');
            out.push_str(&line[start..end]);
            out.push('$');
            copied = end;
            count += 1;
        }
        out.push_str(&line[copied..]);
    }
    (out, count)
}

fn is_rtl_letter(c: char) -> bool {
    matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
        && c.is_alphabetic()
}

/// Whether right-to-left letters make up a real share of the document. LaTeX
/// is all Latin, so a Persian text full of formulas is far from 100% RTL.
fn mostly_rtl(src: &str) -> bool {
    let (mut rtl, mut latin) = (0usize, 0usize);
    for c in src.chars() {
        if c.is_ascii_alphabetic() {
            latin += 1;
        } else if is_rtl_letter(c) {
            rtl += 1;
        }
    }
    rtl > 0 && rtl * 4 >= latin
}

/// End of a leading YAML front matter block, or 0.
fn front_matter_end(src: &str) -> usize {
    let Some(rest) = src.strip_prefix("---\n") else {
        return 0;
    };
    let mut at = 4;
    for line in rest.split_inclusive('\n') {
        at += line.len();
        if matches!(line.trim_end(), "---" | "...") {
            return at;
        }
    }
    0
}

fn is_delimiter_row(line: &str) -> bool {
    line.contains('-')
        && line.contains('|')
        && line
            .chars()
            .all(|c| matches!(c, '|' | ':' | '-' | ' ' | '\t'))
}

/// `[label]: https://…`
fn is_link_definition(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('[') && !t.starts_with("[^") && t.contains("]:")
}

/// Which lines are table rows, where `|` separates cells.
fn table_rows(lines: &[(usize, &str)]) -> Vec<bool> {
    let mut table = vec![false; lines.len()];
    let mut inside = false;
    for i in 0..lines.len() {
        let t = lines[i].1.trim();
        if t.is_empty() {
            inside = false;
        } else if is_delimiter_row(t) && i > 0 && lines[i - 1].1.contains('|') {
            table[i - 1] = true;
            table[i] = true;
            inside = true;
        } else if inside && t.contains('|') {
            table[i] = true;
        } else {
            inside = false;
        }
        if t.starts_with('|') {
            table[i] = true;
        }
    }
    table
}

/// Offset of the content of a line, after block quote markers, a list marker,
/// a task box and heading hashes.
fn content_start(b: &[u8]) -> usize {
    let blanks = |mut i: usize| {
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        i
    };
    let mut i = blanks(0);
    while i < b.len() && b[i] == b'>' {
        i = blanks(i + 1);
    }
    let after_space = |i: usize| i < b.len() && (b[i] == b' ' || b[i] == b'\t');
    if i < b.len() && matches!(b[i], b'-' | b'+' | b'*') && after_space(i + 1) {
        i = blanks(i + 1);
    } else {
        let digits = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
        let j = i + digits;
        if (1..=9).contains(&digits)
            && j < b.len()
            && matches!(b[j], b'.' | b')')
            && after_space(j + 1)
        {
            i = blanks(j + 1);
        }
    }
    if i + 2 < b.len() && b[i] == b'[' && matches!(b[i + 1], b' ' | b'x' | b'X') && b[i + 2] == b']'
    {
        i = blanks(i + 3);
    }
    let hashes = b[i..].iter().take_while(|c| **c == b'#').count();
    if (1..=6).contains(&hashes) && after_space(i + hashes) {
        i = blanks(i + hashes);
    }
    i
}

/// Marks the bytes of a line that must not be read as mathematics: URLs,
/// e-mail addresses, link destinations, footnote marks, wiki links and HTML.
fn protect(line: &str, ok: &mut [bool]) {
    let b = line.as_bytes();
    let n = b.len();

    // Whitespace-separated chunks that are addresses.
    let mut i = 0;
    while i < n {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        let chunk = &line[start..i];
        let email = chunk.char_indices().any(|(k, c)| {
            c == '@'
                && k > 0
                && chunk.as_bytes()[k - 1].is_ascii_alphanumeric()
                && chunk
                    .as_bytes()
                    .get(k + 1)
                    .is_some_and(u8::is_ascii_alphanumeric)
        });
        if email || chunk.contains("://") || chunk.starts_with("www.") {
            ok[start..i].fill(false);
        }
    }

    let mut i = 0;
    while i < n {
        match b[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            // `](destination)`
            b']' if b.get(i + 1) == Some(&b'(') => {
                let mut depth = 0;
                let mut j = i + 1;
                while j < n {
                    match b[j] {
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                if j < n {
                    ok[i..=j].fill(false);
                    i = j + 1;
                    continue;
                }
            }
            // `[^1]` and `[[wiki link]]`
            b'[' if matches!(b.get(i + 1), Some(b'^' | b'[')) => {
                let close: &[u8] = if b[i + 1] == b'[' { b"]]" } else { b"]" };
                if let Some(k) = b[i + 2..].windows(close.len()).position(|w| w == close) {
                    let end = i + 2 + k + close.len();
                    ok[i..end].fill(false);
                    i = end;
                    continue;
                }
            }
            // `<tag …>`, `</tag>`, `<!-- … -->`
            b'<' if b
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, b'/' | b'!')) =>
            {
                if let Some(k) = b[i + 1..].iter().position(|c| *c == b'>') {
                    let end = i + 1 + k + 1;
                    ok[i..end].fill(false);
                    i = end;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
}

/// End of the `{ … }` group opening at `i`, if it closes on this line and
/// lies entirely in readable prose.
fn brace_group(b: &[u8], ok: &[bool], i: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut j = i;
    while j < b.len() {
        if !ok[j] || b[j] == b'\n' {
            return None;
        }
        match b[j] {
            b'\\' => j += 1,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// A `\name` at `i` (the backslash): its name and the end of the command with
/// the arguments attached to it.
fn command<'a>(line: &'a str, ok: &[bool], i: usize) -> (&'a str, usize) {
    let b = line.as_bytes();
    let mut j = i + 1;
    while j < b.len() && b[j].is_ascii_alphabetic() {
        j += 1;
    }
    let name = &line[i + 1..j];
    if SIZED.contains(&name) {
        // The delimiter: `(`, `\{`, `\lvert`, `.`
        if j < b.len() && ok[j] {
            if b[j] == b'\\' {
                let mut k = j + 1;
                while k < b.len() && b[k].is_ascii_alphabetic() {
                    k += 1;
                }
                j = if k == j + 1 { (j + 2).min(b.len()) } else { k };
            } else if b[j].is_ascii_punctuation() {
                j += 1;
            }
        }
        return (name, j);
    }
    if name == "sqrt"
        && j < b.len()
        && b[j] == b'['
        && let Some(k) = b[j..].iter().position(|c| *c == b']')
        && ok[j..j + k].iter().all(|x| *x)
    {
        j += k + 1;
    }
    while j < b.len() && b[j] == b'{' {
        match brace_group(b, ok, j) {
            Some(end) => j = end,
            None => break,
        }
    }
    (name, j)
}

fn ends_operand(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Letter
            | Kind::Number
            | Kind::Command
            | Kind::Script
            | Kind::Close
            | Kind::Capitals
            | Kind::MathWord
            | Kind::Postfix
    )
}

fn tokenize(line: &str, ok: &[bool], table: bool) -> Vec<Token> {
    let b = line.as_bytes();
    let n = b.len();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < n {
        let start = i;
        let attached = tokens
            .last()
            .is_some_and(|t| t.end == i && ends_operand(t.kind));
        let kind = if !ok[i] {
            while i < n && !ok[i] {
                i += 1;
            }
            Kind::Break
        } else if !b[i].is_ascii() {
            let c = line[i..].chars().next().expect("a char starts here");
            i += c.len_utf8();
            match c {
                'α'..='ω' | 'Α'..='Ω' => Kind::Letter,
                '∞' | '∅' | 'ℝ' | 'ℕ' | 'ℤ' | 'ℚ' | 'ℂ' => Kind::Number,
                '≤' | '≥' | '≠' | '≈' | '∈' | '∉' | '⊂' | '⊆' | '∪' | '∩' | '→' | '⇒' | '⇔'
                | '×' | '±' => Kind::Relation,
                '·' | '−' => Kind::Operator,
                '′' if attached => Kind::Postfix,
                '\u{00A0}' => Kind::Space,
                _ => Kind::Break,
            }
        } else {
            match b[i] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    i += 1;
                    Kind::Space
                }
                b'\\' => match b.get(i + 1) {
                    Some(c) if c.is_ascii_alphabetic() => {
                        let (name, end) = command(line, ok, i);
                        i = end;
                        if NOT_MATH.contains(&name) {
                            Kind::Break
                        } else if matches!(name, "quad" | "qquad") {
                            Kind::Operator
                        } else {
                            Kind::Command
                        }
                    }
                    Some(b'{' | b'}') => {
                        i += 2;
                        Kind::Command
                    }
                    Some(b'|') if !table => {
                        i += 2;
                        Kind::Command
                    }
                    Some(b',' | b';' | b'!' | b':' | b' ') => {
                        i += 2;
                        Kind::Operator
                    }
                    Some(c) if c.is_ascii() => {
                        i += 2;
                        Kind::Break
                    }
                    _ => {
                        i += 1;
                        Kind::Break
                    }
                },
                b'0'..=b'9' => {
                    while i < n && b[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i + 1 < n && b[i] == b'.' && b[i + 1].is_ascii_digit() {
                        i += 1;
                        while i < n && b[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let letters = b[i..]
                        .iter()
                        .take_while(|c| c.is_ascii_alphabetic())
                        .count();
                    if letters >= 2 {
                        // `10th`, `5kg`
                        while i < n && b[i].is_ascii_alphanumeric() {
                            i += 1;
                        }
                        Kind::Word
                    } else {
                        Kind::Number
                    }
                }
                c if c.is_ascii_alphabetic() => {
                    while i < n && b[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let letters = i - start;
                    let word = &line[start..i];
                    // `e.g.`, `i.e.`, `o.w.`
                    let dotted = letters == 1
                        && i + 1 < n
                        && b[i] == b'.'
                        && b[i + 1].is_ascii_alphabetic()
                        && b.get(i + 2).is_none_or(|c| !c.is_ascii_alphabetic());
                    if dotted {
                        while i + 1 < n
                            && b[i] == b'.'
                            && b[i + 1].is_ascii_alphabetic()
                            && b.get(i + 2).is_none_or(|c| !c.is_ascii_alphabetic())
                        {
                            i += 2;
                        }
                        Kind::Word
                    } else if i < n && b[i].is_ascii_digit() {
                        while i < n && b[i].is_ascii_digit() {
                            i += 1;
                        }
                        if letters == 1 && !(i < n && b[i].is_ascii_alphabetic()) {
                            // `x1`
                            Kind::Letter
                        } else {
                            while i < n && b[i].is_ascii_alphanumeric() {
                                i += 1;
                            }
                            Kind::Word
                        }
                    } else if letters == 1 {
                        Kind::Letter
                    } else if MATH_WORDS.contains(&word)
                        || (letters == 2 && b[start] == b'd' && b[start + 1] != b'o')
                            && b[start + 1].is_ascii_lowercase()
                    {
                        Kind::MathWord
                    } else if letters == 2
                        && b[start].is_ascii_lowercase()
                        && b[start + 1].is_ascii_uppercase()
                    {
                        // `bX`: a product; no English word is spelled so.
                        Kind::MathWord
                    } else if letters <= 4 && word.bytes().all(|c| c.is_ascii_uppercase()) {
                        Kind::Capitals
                    } else {
                        Kind::Word
                    }
                }
                c @ (b'^' | b'_') => {
                    i += 1;
                    let next = b.get(i).copied().filter(|_| ok.get(i) == Some(&true));
                    let script_end = match next {
                        _ if !attached => None,
                        Some(b'{') => brace_group(b, ok, i),
                        Some(b'\\') if b.get(i + 1).is_some_and(u8::is_ascii_alphabetic) => {
                            Some(command(line, ok, i).1)
                        }
                        Some(x) if x.is_ascii_alphanumeric() => {
                            // `S_X` is a subscript; `snake_case` is not.
                            let lone = b.get(i + 1).is_none_or(|y| !y.is_ascii_alphanumeric());
                            (c == b'^' || lone).then_some(i + 1)
                        }
                        Some(b'+' | b'-' | b'*') if c == b'^' => Some(i + 1),
                        _ => None,
                    };
                    match script_end {
                        Some(end) => {
                            i = end;
                            Kind::Script
                        }
                        None if next.is_some_and(|x| x.is_ascii_alphanumeric()) => {
                            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                                i += 1;
                            }
                            Kind::Word
                        }
                        None => Kind::Break,
                    }
                }
                b'(' | b'[' => {
                    i += 1;
                    Kind::Open
                }
                b')' | b']' => {
                    i += 1;
                    Kind::Close
                }
                b'=' | b'<' | b'>' | b'+' => {
                    i += 1;
                    Kind::Relation
                }
                b'-' | b'/' | b',' | b':' => {
                    i += 1;
                    Kind::Operator
                }
                b'|' => {
                    i += 1;
                    if table { Kind::Break } else { Kind::Operator }
                }
                b'!' | b'\'' => {
                    i += 1;
                    if attached { Kind::Postfix } else { Kind::Break }
                }
                _ => {
                    i += 1;
                    Kind::Break
                }
            }
        };
        tokens.push(Token {
            kind,
            start,
            end: i,
        });
    }
    tokens
}

/// Byte ranges of `line` to wrap in `$ … $`, in order.
fn formula_spans(line: &str, zone: &[Zone], table: bool, doc_rtl: bool) -> Vec<(usize, usize)> {
    let b = line.as_bytes();
    let mut ok: Vec<bool> = zone.iter().map(|z| *z == Zone::Text).collect();
    let lead = content_start(b);
    ok[..lead].fill(false);
    if !ok.iter().any(|x| *x) {
        return Vec::new();
    }
    protect(line, &mut ok);

    let tokens = tokenize(line, &ok, table);
    let has_words = tokens.iter().any(|t| t.kind == Kind::Word);
    let rtl = line.chars().any(is_rtl_letter) || (doc_rtl && !has_words);
    let scan = Scan {
        line,
        tokens: &tokens,
        rtl,
    };
    let mut spans = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if matches!(tokens[i].kind, Kind::Break | Kind::Word) {
            i += 1;
            continue;
        }
        let from = i;
        while i < tokens.len() && !matches!(tokens[i].kind, Kind::Break | Kind::Word) {
            i += 1;
        }
        scan.run(from, i, &mut spans);
    }
    spans
}

struct Scan<'a> {
    line: &'a str,
    tokens: &'a [Token],
    rtl: bool,
}

impl Scan<'_> {
    fn kind(&self, i: usize) -> Kind {
        self.tokens[i].kind
    }

    fn text(&self, i: usize) -> &str {
        &self.line[self.tokens[i].start..self.tokens[i].end]
    }

    /// Splits `lo..hi` at brackets that pair with nothing, then judges each
    /// balanced piece.
    fn run(&self, lo: usize, hi: usize, spans: &mut Vec<(usize, usize)>) {
        let mut open: Vec<usize> = Vec::new();
        for i in lo..hi {
            match self.kind(i) {
                Kind::Open => open.push(i),
                Kind::Close if open.pop().is_none() => {
                    self.run(lo, i, spans);
                    self.run(i + 1, hi, spans);
                    return;
                }
                _ => {}
            }
        }
        if let Some(first) = open.first().copied() {
            self.run(lo, first, spans);
            self.run(first + 1, hi, spans);
            return;
        }
        if let Some((lo, hi)) = self.trim(lo, hi)
            && self.is_formula(lo, hi)
        {
            spans.push((self.tokens[lo].start, self.tokens[hi - 1].end));
        }
    }

    /// Drops what does not belong to the formula from both ends: spaces,
    /// dangling operators, and capitals that merely stand next to it.
    fn trim(&self, mut lo: usize, mut hi: usize) -> Option<(usize, usize)> {
        let strong = (lo..hi).any(|i| matches!(self.kind(i), Kind::Command | Kind::Script));
        loop {
            // `|x|` keeps its bars; the bar of a cut-off `a |` goes.
            let bars = (lo..hi).filter(|i| self.text(*i) == "|").count();
            let loose = |i: usize| match self.kind(i) {
                Kind::Space => true,
                Kind::Operator | Kind::Relation => !(self.text(i) == "|" && bars % 2 == 0),
                _ => false,
            };
            while lo < hi && loose(lo) {
                // A sign belongs to the operand it touches: `-\infty`.
                let sign = matches!(self.text(lo), "-" | "+")
                    && lo + 1 < hi
                    && !loose(lo + 1)
                    && self.kind(lo + 1) != Kind::Close;
                if sign {
                    break;
                }
                lo += 1;
            }
            while hi > lo && loose(hi - 1) {
                hi -= 1;
            }
            if lo >= hi {
                return None;
            }
            let weak = |i: usize| match self.kind(i) {
                Kind::Capitals => true,
                Kind::MathWord => !strong,
                _ => false,
            };
            let bound = |k: Kind| matches!(k, Kind::Operator | Kind::Relation | Kind::Postfix);
            if weak(lo) && lo + 1 < hi {
                let next = (lo + 1..hi).find(|i| self.kind(*i) != Kind::Space);
                let applied = self.kind(lo) == Kind::MathWord && self.kind(lo + 1) == Kind::Open;
                if next.is_some_and(|j| !bound(self.kind(j)) && self.kind(j) != Kind::Close)
                    && !applied
                {
                    lo += 1;
                    continue;
                }
            }
            if weak(hi - 1) && hi - 1 > lo {
                let prev = (lo..hi - 1).rev().find(|i| self.kind(*i) != Kind::Space);
                if prev.is_some_and(|j| !bound(self.kind(j)) && self.kind(j) != Kind::Open) {
                    hi -= 1;
                    continue;
                }
            }
            return Some((lo, hi));
        }
    }

    fn is_formula(&self, lo: usize, hi: usize) -> bool {
        let any = |f: &dyn Fn(Kind) -> bool| (lo..hi).any(|i| f(self.kind(i)));
        if any(&|k| matches!(k, Kind::Command | Kind::Script)) {
            return true;
        }
        let operand = any(&|k| matches!(k, Kind::Letter | Kind::Number));
        if operand && (lo + 1..hi).any(|i| self.kind(i) == Kind::Relation) {
            return true;
        }
        // `P(E)`
        let applied = (lo..hi.saturating_sub(1)).any(|i| {
            self.kind(i) == Kind::Letter
                && self.kind(i + 1) == Kind::Open
                && self.kind(i + 2) != Kind::Close
        });
        if applied {
            return true;
        }
        if !self.rtl {
            return false;
        }
        if (lo..hi).any(|i| self.kind(i) == Kind::Letter && self.stands_alone(i)) {
            return true;
        }
        // `[0, 1]`
        self.kind(lo) == Kind::Open
            && self.kind(hi - 1) == Kind::Close
            && (lo..hi).any(|i| self.text(i) == ",")
            && any(&|k| k == Kind::Number)
    }

    /// A single letter that is not part of an English phrase: the nearest
    /// word on either side, across spaces and punctuation, is not Latin.
    fn stands_alone(&self, i: usize) -> bool {
        let wordy = |k: Kind| matches!(k, Kind::Word | Kind::Capitals | Kind::MathWord);
        let left = (0..i).rev().find(|j| {
            !matches!(
                self.kind(*j),
                Kind::Space | Kind::Open | Kind::Operator | Kind::Relation
            )
        });
        let right = (i + 1..self.tokens.len()).find(|j| {
            !matches!(
                self.kind(*j),
                Kind::Space | Kind::Close | Kind::Operator | Kind::Relation | Kind::Postfix
            )
        });
        !left.is_some_and(|j| wordy(self.kind(j))) && !right.is_some_and(|j| wordy(self.kind(j)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(src: &str) {
        assert_eq!(wrap_bare_math(src), src);
    }

    #[test]
    fn latex_in_persian_prose_is_wrapped() {
        assert_eq!(
            wrap_bare_math("اگر E به صورت \\{a\\} باشد و با \\emptyset نشان دهند."),
            "اگر $E$ به صورت $\\{a\\}$ باشد و با $\\emptyset$ نشان دهند."
        );
        assert_eq!(
            wrap_bare_math("احتمال برابر با \\frac{3}{6} = \\frac{1}{2} است."),
            "احتمال برابر با $\\frac{3}{6} = \\frac{1}{2}$ است."
        );
        assert_eq!(
            wrap_bare_math(
                "اگر A_1, A_2, \\dots در \\mathcal{F} باشند، \\bigcup_{i=1}^{\\infty} A_i نیز"
            ),
            "اگر $A_1, A_2, \\dots$ در $\\mathcal{F}$ باشند، $\\bigcup_{i=1}^{\\infty} A_i$ نیز"
        );
        assert_eq!(
            wrap_bare_math("دایره C_1\\left(O, \\frac{R}{2}\\right). فاصله"),
            "دایره $C_1\\left(O, \\frac{R}{2}\\right)$. فاصله"
        );
    }

    #[test]
    fn runs_stop_at_persian_and_at_unmatched_brackets() {
        assert_eq!(
            wrap_bare_math(
                "(یعنی E_i \\cap E_j = \\emptyset به ازای i \\neq j و i, j = 1, 2, \\dots)، آن‌گاه"
            ),
            "(یعنی $E_i \\cap E_j = \\emptyset$ به ازای $i \\neq j$ و $i, j = 1, 2, \\dots$)، آن‌گاه"
        );
        assert_eq!(
            wrap_bare_math("سه‌تایی (S, \\mathcal{F}, P) و بازه (a, b]]_ و S_X = [0, 2)."),
            "سه‌تایی $(S, \\mathcal{F}, P)$ و بازه $(a, b]$]_ و $S_X = [0, 2)$."
        );
        assert_eq!(
            wrap_bare_math("- (الف) 0 \\leq F(x) \\leq 1, \\quad -\\infty < x < +\\infty"),
            "- (الف) $0 \\leq F(x) \\leq 1, \\quad -\\infty < x < +\\infty$"
        );
    }

    #[test]
    fn letters_and_calls_in_persian_prose() {
        assert_eq!(
            wrap_bare_math(
                "پیشامد (Event) E : مساحت دایره C(O, R) و برد (Range) [0, 1] که P(S) = 1"
            ),
            "پیشامد (Event) $E$ : مساحت دایره $C(O, R)$ و برد (Range) $[0, 1]$ که $P(S) = 1$"
        );
        assert_eq!(
            wrap_bare_math("(Cumulative Distribution Function - CDF) X می‌گویند و B_jها را"),
            "(Cumulative Distribution Function - CDF) $X$ می‌گویند و $B_j$ها را"
        );
        assert_eq!(
            wrap_bare_math("داریم X(TTH) = 3. در S = \\{TT, TH\\} است"),
            "داریم $X(TTH) = 3$. در $S = \\{TT, TH\\}$ است"
        );
        assert_eq!(
            wrap_bare_math("تابع خطی (Linear Function) Y = bX + c داریم"),
            "تابع خطی (Linear Function) $Y = bX + c$ داریم"
        );
    }

    #[test]
    fn english_glosses_are_not_math() {
        for src in [
            "احتمال A به شرط B (Conditional Probability of A given B) است",
            "استانداردسازی (Standardization of a Normal Variable) و (Type I Error)",
            "سیگما میدان (Sigma-Field) و (Non-empty Sample Space) و CDF و PDF",
            "برابر با 2 و با احتمال 0.8 و ساعت 10:30 و ۱۴۰۵/۰۸/۲۴",
            "- Email : h_saeedi@sbu.ac.ir",
            "ببینید https://example.com/a_b?x=1 و [متن](http://x.y/a_1) و[^1]",
            "یعنی (i.e. همان) و snake_case و x-axis و Variable(s)",
            "_آزمایش تصادفی_ و **قضیه ۱.** و *[Figure 5.2: شکل]*",
        ] {
            let out = wrap_bare_math(src);
            let expected = src.replace("احتمال A به شرط B (", "احتمال $A$ به شرط $B$ (");
            assert_eq!(out, expected, "{src}");
        }
        assert_eq!(
            wrap_bare_math("(Sigma-Field / \\sigma-algebra)"),
            "(Sigma-Field / $\\sigma$-algebra)"
        );
    }

    #[test]
    fn english_prose_needs_real_evidence() {
        assert_eq!(
            wrap_bare_math("Let x = 5 and f(x) < 3, so A \\subseteq B holds."),
            "Let $x = 5$ and $f(x) < 3$, so $A \\subseteq B$ holds."
        );
        for src in [
            "Let V be a vector space and I think so.",
            "Call +98 912 or see part (a) of Figure 2(b), i.e. the USA > UK case.",
            "Use python >= 3.8 - or C++ - on 2020-01-01 at 3/4 speed.",
            "> quoted text\n\n1. first\n2. second\n\n---\n",
            "<span class=\"a = b\">text</span> and <!-- x = 1 -->",
        ] {
            same(src);
        }
        assert_eq!(
            wrap_bare_math("where n = |V| holds"),
            "where $n = |V|$ holds"
        );
    }

    #[test]
    fn existing_math_code_and_tables() {
        same("اگر $x > 0$ و $$y = 1$$ و `a = b` آن‌گاه\n\n```\nx = 1\n```\n");
        same("---\ntitle: a = b\n---\n");
        assert_eq!(
            wrap_bare_math("|پیشامد|احتمال|\n|---|---|\n|X \\leq a|F(a)|\n|X = a|F(a) - F(a^-)|\n"),
            "|پیشامد|احتمال|\n|---|---|\n|$X \\leq a$|$F(a)$|\n|$X = a$|$F(a) - F(a^-)$|\n"
        );
        same("| $X = x$ | 0 | 1 | جمع |\n| --- | --- | --- | --- |\n");
    }

    #[test]
    fn repair_is_idempotent_and_counts() {
        let src = "فرض کنید \\(f\\) پیوسته و A^c متمم A باشد. آن‌گاه P(A^c) = 1 - P(A) است.\n";
        let once = repair_markdown(src);
        assert!(once.changed);
        assert_eq!(once.formulas, 3);
        assert_eq!(
            once.text,
            "فرض کنید $f$ پیوسته و $A^c$ متمم $A$ باشد. آن‌گاه $P(A^c) = 1 - P(A)$ است.\n"
        );
        let twice = repair_markdown(&once.text);
        assert!(!twice.changed);
        assert_eq!(twice.formulas, 0);
    }
}
