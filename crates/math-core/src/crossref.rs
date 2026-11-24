//! Cross-reference links: "طبق قضیه ۳.۳" / "by Theorem 3.3" and named
//! results like "Cauchy-Schwarz" / "هاینه–کانتور" in running text become
//! links to the block carrying that label or name.

use std::collections::{BTreeMap, BTreeSet};

use crate::features::{Lexicon, scan_number};
use crate::model::{BlockType, SemanticBlock};
use crate::text::{fold_digits, lower_char, normalize_char};

type Targets = BTreeMap<(BlockType, String), String>;
type NameTargets = Vec<(Vec<char>, String)>;

fn fold_chars(s: &str) -> Vec<char> {
    s.trim()
        .chars()
        .filter_map(normalize_char)
        .map(lower_char)
        .collect()
}

fn is_linkable_name(chars: &[char]) -> bool {
    let non_ws = chars.iter().filter(|c| !c.is_whitespace()).count();
    let has_non_ascii = chars.iter().any(|c| !c.is_ascii());
    if has_non_ascii {
        non_ws >= 3
    } else {
        non_ws >= 4
    }
}

/// Rewrites the HTML of every block. A reference resolves to the first block
/// of that type and number (or matching parenthetical name); references without
/// a matching block stay text.
pub(crate) fn link_references(blocks: &mut [SemanticBlock], lexicon: &Lexicon) {
    let mut targets = Targets::new();
    let mut seen_names = BTreeSet::new();
    let mut name_targets: NameTargets = Vec::new();

    for block in blocks.iter().filter(|b| b.kind.is_semantic()) {
        if let Some(number) = &block.number {
            targets
                .entry((block.kind, fold_digits(number)))
                .or_insert_with(|| block.id.clone());
        }
        if let Some(name) = &block.name {
            let folded = fold_chars(name);
            if is_linkable_name(&folded) && seen_names.insert(folded.clone()) {
                name_targets.push((folded, block.id.clone()));
            }
        }
    }
    if targets.is_empty() && name_targets.is_empty() {
        return;
    }
    // Match longer names first so "Cauchy-Schwarz Inequality" beats "Cauchy-Schwarz".
    name_targets.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    for block in blocks.iter_mut().filter(|b| b.kind != BlockType::Heading) {
        // A block's own label or name is not a reference to itself.
        let own = block
            .number
            .as_deref()
            .map(|n| (block.kind, fold_digits(n)));
        let own_id = block.id.clone();
        block.html = link_html(
            &block.html,
            lexicon,
            &targets,
            &name_targets,
            own.as_ref(),
            &own_id,
        );
    }
}

fn link_html(
    html: &str,
    lexicon: &Lexicon,
    targets: &Targets,
    name_targets: &NameTargets,
    own: Option<&(BlockType, String)>,
    own_id: &str,
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
                link_run(
                    &rest[..end],
                    lexicon,
                    targets,
                    name_targets,
                    own,
                    own_id,
                    &mut out,
                );
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
    name_targets: &NameTargets,
    own: Option<&(BlockType, String)>,
    own_id: &str,
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
        if !at_word_start {
            p += 1;
            continue;
        }

        // 1. Explicit numbered citation: "Theorem 3.2" / "قضیه ۳.۲"
        if let Some((kind, len)) = lexicon.keyword_at(&low, p) {
            let mut q = p + len;
            while q < low.len() && low[q].is_whitespace() {
                q += 1;
            }
            let end = scan_number(&low, q);
            if q > p + len && end > q {
                let key = (kind, fold_digits(&low[q..end].iter().collect::<String>()));
                match targets.get(&key) {
                    Some(id) if own != Some(&key) && id != own_id => {
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
                continue;
            }
        }

        // 2. Named result citation: "Cauchy-Schwarz" / "هاینه–کانتور"
        if let Some((name, id)) = name_targets.iter().find(|(name, id)| {
            id != own_id && low[p..].starts_with(name.as_slice()) && {
                let end = p + name.len();
                end == low.len() || !low[end].is_alphanumeric()
            }
        }) {
            let end = p + name.len();
            let (from, to) = (bytes[p].start, bytes[end - 1].end);
            out.push_str(&run[copied..from]);
            out.push_str(&format!("<a class=\"mx-ref\" href=\"#{id}\">"));
            out.push_str(&run[from..to]);
            out.push_str("</a>");
            copied = to;
            p = end;
            continue;
        }

        p += 1;
    }
    out.push_str(&run[copied..]);
}

#[cfg(test)]
mod tests {
    use crate::{ScoringConfig, analyze};

    #[test]
    fn links_named_results_bilingual_without_self_linking() {
        let src = "\
**Theorem 1 (Cauchy-Schwarz).** For all vectors $u, v$, $|\\langle u, v \\rangle| \\le \\|u\\|\\|v\\|$.

**Proof.** By Cauchy-Schwarz we obtain the bound immediately. $\\blacksquare$

**قضیه ۲ (هاینه–کانتور).** هر تابع پیوسته روی مجموعه فشرده، به‌طور یکنواخت پیوسته است.

**نتیجه ۳.** طبق هاینه–کانتور این تابع یکنواخت پیوسته است.
";
        let doc = analyze(src, &ScoringConfig::default());
        let thm1 = doc
            .blocks
            .iter()
            .find(|b| b.id == "block-theorem-1")
            .unwrap();
        let proof = doc
            .blocks
            .iter()
            .find(|b| b.kind == crate::BlockType::Proof)
            .unwrap();
        let cor = doc
            .blocks
            .iter()
            .find(|b| b.kind == crate::BlockType::Corollary)
            .unwrap();

        assert!(
            !thm1.html.contains("href=\"#block-theorem-1\""),
            "theorem should not self-link"
        );
        assert!(
            proof
                .html
                .contains("<a class=\"mx-ref\" href=\"#block-theorem-1\">Cauchy-Schwarz</a>"),
            "proof should link named English theorem: {}",
            proof.html
        );
        assert!(
            cor.html.contains("class=\"mx-ref\"") && cor.html.contains("هاینه–کانتور"),
            "corollary should link named Persian theorem: {}",
            cor.html
        );
    }
}
