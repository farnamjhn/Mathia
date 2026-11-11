//! Cross-reference links: "طبق قضیه ۳.۳" / "by Theorem 3.3" in running text
//! becomes a link to the block carrying that label.

use std::collections::BTreeMap;

use crate::features::{Lexicon, scan_number};
use crate::model::{BlockType, SemanticBlock};
use crate::text::{fold_digits, lower_char, normalize_char};

type Targets = BTreeMap<(BlockType, String), String>;

/// Rewrites the HTML of every block. A reference resolves to the first block
/// of that type and number; references without a matching block stay text.
pub(crate) fn link_references(blocks: &mut [SemanticBlock], lexicon: &Lexicon) {
    let mut targets = Targets::new();
    for block in blocks.iter().filter(|b| b.kind.is_semantic()) {
        if let Some(number) = &block.number {
            targets
                .entry((block.kind, fold_digits(number)))
                .or_insert_with(|| block.id.clone());
        }
    }
    if targets.is_empty() {
        return;
    }
    for block in blocks.iter_mut().filter(|b| b.kind != BlockType::Heading) {
        // A block's own label is not a reference to itself.
        let own = block
            .number
            .as_deref()
            .map(|n| (block.kind, fold_digits(n)));
        block.html = link_html(&block.html, lexicon, &targets, own.as_ref());
    }
}

fn link_html(
    html: &str,
    lexicon: &Lexicon,
    targets: &Targets,
    own: Option<&(BlockType, String)>,
) -> String {
    let mut out = String::with_capacity(html.len());
    // Open elements whose text is never linked: links, code and math.
    let mut quiet: Vec<&str> = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        if rest.starts_with('<') {
            let end = rest.find('>').map_or(rest.len(), |e| e + 1);
            let tag = &rest[..end];
            let inner = tag.trim_start_matches('<').trim_end_matches('>');
            if let Some(name) = inner.strip_prefix('/') {
                if quiet.last() == Some(&name.trim()) {
                    quiet.pop();
                }
            } else if !inner.ends_with('/') {
                let name = inner.split(char::is_whitespace).next().unwrap_or("");
                if matches!(name, "a" | "code" | "pre")
                    || (name == "span" && inner.contains("class=\"math"))
                {
                    quiet.push(name);
                }
            }
            out.push_str(tag);
            rest = &rest[end..];
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            if quiet.is_empty() {
                link_run(&rest[..end], lexicon, targets, own, &mut out);
            } else {
                out.push_str(&rest[..end]);
            }
            rest = &rest[end..];
        }
    }
    out
}

/// Links the references in one run of text (no tags inside).
fn link_run(
    run: &str,
    lexicon: &Lexicon,
    targets: &Targets,
    own: Option<&(BlockType, String)>,
    out: &mut String,
) {
    // Folded chars, each with the byte range it came from in `run`.
    let mut low = Vec::new();
    let mut bytes = Vec::new();
    for (at, c) in run.char_indices() {
        if let Some(folded) = normalize_char(c) {
            low.push(lower_char(folded));
            bytes.push(at..at + c.len_utf8());
        }
    }

    let (mut p, mut copied) = (0, 0);
    while p < low.len() {
        let at_word_start = p == 0 || !low[p - 1].is_alphanumeric();
        let Some((kind, len)) = at_word_start.then(|| lexicon.keyword_at(&low, p)).flatten() else {
            p += 1;
            continue;
        };
        let mut q = p + len;
        while q < low.len() && low[q].is_whitespace() {
            q += 1;
        }
        let end = scan_number(&low, q);
        if q == p + len || end == q {
            p += len;
            continue;
        }
        let key = (kind, fold_digits(&low[q..end].iter().collect::<String>()));
        match targets.get(&key) {
            Some(id) if own != Some(&key) => {
                let (from, to) = (bytes[p].start, bytes[end - 1].end);
                out.push_str(&run[copied..from]);
                out.push_str(&format!("<a class=\"mx-ref\" href=\"#{id}\">"));
                out.push_str(&run[from..to]);
                out.push_str("</a>");
                copied = to;
            }
            _ => {}
        }
        p = end;
    }
    out.push_str(&run[copied..]);
}
