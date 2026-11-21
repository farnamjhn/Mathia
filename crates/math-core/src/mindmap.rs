//! Deterministic, compact navigation tree extracted from the compiler's AST.
//! This module never calls a judge, network service, or language model.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use crate::dom::{Dom, NodeData, NodeId};
use crate::model::{BlockType, Document, MathSpan, Node, NodeKind, SemanticBlock};
use crate::text::fold_digits;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MindMapNodeType {
    Root,
    Heading,
    Definition,
    Theorem,
    Lemma,
    Corollary,
    Proposition,
    Example,
    Formula,
    Note,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MindMapEdge {
    pub from: String,          // Source node ID (e.g., "thm-3-1")
    pub to: String,            // Referenced node ID (e.g., "def-1-2")
    pub label: Option<String>, // e.g., "uses" or matched ref text
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MindMapData {
    pub root: MindMapNode,
    pub edges: Vec<MindMapEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MindMapNode {
    /// Existing compiled HTML target, shared with the table of contents.
    pub id: String,
    /// Plain text with atomic inline $math$, never trusted HTML.
    pub label: String,
    pub subtitle: Option<String>,
    pub node_type: MindMapNodeType,
    pub level: u8,
    /// Anchor ID of an immediately following Proof or Solution block
    pub proof_id: Option<String>,
    /// Bolded key concepts extracted from prose under this section
    pub key_terms: Vec<String>,
    pub children: Vec<MindMapNode>,
}

/// Always creates a virtual Root node (level: 0) from the document title/filename
/// so documents with multiple # H1 sections or pre-H1 blocks never corrupt the heading stack.
pub fn extract(doc: &Document, fallback_title: &str) -> MindMapData {
    let root_label = if !fallback_title.trim().is_empty() {
        fallback_title
    } else {
        doc.title.as_deref().unwrap_or("Document")
    };
    let mut root = MindMapNode {
        id: "mx-main".into(),
        label: compact(root_label, false),
        subtitle: None,
        node_type: MindMapNodeType::Root,
        level: 0,
        proof_id: None,
        key_terms: Vec::new(),
        children: Vec::new(),
    };

    // Heading stack: (heading_level, path_from_root)
    let mut stack: Vec<(u8, Vec<usize>)> = Vec::new();

    let heading_indices: Vec<usize> = doc
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == BlockType::Heading)
        .map(|(i, _)| i)
        .collect();

    for (index, block) in doc.blocks.iter().enumerate() {
        let is_heading = block.kind == BlockType::Heading;
        let kind = match block.kind {
            BlockType::Heading => MindMapNodeType::Heading,
            BlockType::Definition => MindMapNodeType::Definition,
            BlockType::Theorem => MindMapNodeType::Theorem,
            BlockType::Lemma => MindMapNodeType::Lemma,
            BlockType::Corollary => MindMapNodeType::Corollary,
            BlockType::Proposition => MindMapNodeType::Proposition,
            BlockType::Example => MindMapNodeType::Example,
            BlockType::Remark | BlockType::Notation => MindMapNodeType::Note,
            _ => continue, // Proofs/solutions and ordinary prose
        };

        let level = if is_heading {
            block.heading_level.unwrap_or(1).clamp(1, 6)
        } else {
            stack.last().map_or(1, |(lvl, _)| lvl + 1)
        };

        if is_heading {
            while stack.last().is_some_and(|(active, _)| *active >= level) {
                stack.pop();
            }
        }

        let mut path = stack.last().map_or_else(Vec::new, |(_, p)| p.clone());
        let mut parent = &mut root;
        for &child in &path {
            parent = &mut parent.children[child];
        }

        let (label, subtitle) = preview(doc, block);

        let key_terms = if is_heading {
            let start_node = block.span.start;
            let next_heading_node = heading_indices
                .iter()
                .find(|&&h_idx| h_idx > index)
                .map(|&h_idx| doc.blocks[h_idx].span.start)
                .unwrap_or(doc.nodes.len());
            extract_key_terms(doc, start_node..next_heading_node)
        } else {
            Vec::new()
        };

        let proof_id = if !is_heading {
            match block.kind {
                BlockType::Theorem
                | BlockType::Lemma
                | BlockType::Proposition
                | BlockType::Corollary => doc
                    .blocks
                    .get(index + 1)
                    .filter(|next| next.kind == BlockType::Proof)
                    .map(|b| b.id.clone())
                    .or_else(|| {
                        doc.blocks
                            .iter()
                            .find(|b| {
                                b.kind == BlockType::Proof && b.proves.as_deref() == Some(&block.id)
                            })
                            .map(|b| b.id.clone())
                    }),
                BlockType::Example => doc
                    .blocks
                    .get(index + 1)
                    .filter(|next| next.kind == BlockType::Solution)
                    .map(|b| b.id.clone())
                    .or_else(|| {
                        doc.blocks
                            .iter()
                            .find(|b| {
                                b.kind == BlockType::Solution
                                    && b.proves.as_deref() == Some(&block.id)
                            })
                            .map(|b| b.id.clone())
                    }),
                _ => None,
            }
        } else {
            None
        };

        path.push(parent.children.len());
        parent.children.push(MindMapNode {
            id: block.id.clone(),
            label,
            subtitle,
            node_type: kind,
            level,
            proof_id,
            key_terms,
            children: Vec::new(),
        });

        if is_heading {
            stack.push((level, path));
        }
    }

    let edges = detect_edges(doc, &root);

    MindMapData { root, edges }
}

fn extract_key_terms(doc: &Document, node_range: std::ops::Range<usize>) -> Vec<String> {
    if doc.nodes.is_empty() {
        return Vec::new();
    }
    let start = node_range.start.min(doc.nodes.len());
    let end = node_range.end.min(doc.nodes.len());
    if start >= end {
        return Vec::new();
    }

    let mut candidates: Vec<(u8, usize, String)> = Vec::new();
    let mut order = 0;

    for (node_idx, node) in doc.nodes[start..end].iter().enumerate() {
        let abs_idx = start + node_idx;
        if !matches!(node.kind, NodeKind::Paragraph | NodeKind::ListItem) {
            continue;
        }

        let is_in_text_block = doc
            .blocks
            .iter()
            .any(|b| b.kind == BlockType::Text && b.span.contains(&abs_idx));
        if !is_in_text_block {
            continue;
        }

        let dom = Dom::parse(&node.html);
        let strong_ids = dom.find_all(dom.root(), |d, id| d.is_tag(id, "strong"));
        if strong_ids.is_empty() {
            continue;
        }

        let full_text = &node.flat;
        let has_definitional_cue = full_text.contains(":=")
            || full_text.contains("\\iff")
            || full_text.contains("\\Longleftrightarrow")
            || full_text.to_lowercase().contains("is called")
            || full_text.contains("نامیده می\u{200c}شود")
            || full_text.contains("نامیده میشود")
            || full_text.contains("گفته می\u{200c}شود")
            || full_text.contains("گفته میشود")
            || full_text.contains("تعریف می\u{200c}شود")
            || full_text.contains("تعریف میشود");

        for strong_id in strong_ids {
            let term = rich_text(&dom, strong_id);
            let term = compact(&term, false);
            let cleaned = term
                .trim()
                .trim_end_matches(['.', ':', '：', ',', '،', ';', '؛'])
                .trim()
                .to_string();
            if cleaned.is_empty() || cleaned.chars().count() > 60 {
                continue;
            }
            let priority = if has_definitional_cue { 0 } else { 1 };
            candidates.push((priority, order, cleaned));
            order += 1;
        }
    }

    candidates.sort_by_key(|&(p, o, _)| (p, o));
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for (_, _, term) in candidates {
        let lower = term.to_lowercase();
        if seen.insert(lower) {
            result.push(term);
            if result.len() == 5 {
                break;
            }
        }
    }
    result
}

fn preview(doc: &Document, block: &SemanticBlock) -> (String, Option<String>) {
    let Some(opener) = doc.nodes.get(block.span.start) else {
        return (compact(&block.title, false), None);
    };
    if matches!(opener.kind, NodeKind::Heading(_)) {
        return (compact(&node_text(opener), false), None);
    }
    // Named/numbered statements already have a concise classifier title.
    // A leading bold opener often contains an additional unparenthesised title.
    let dom = Dom::parse(&opener.html);
    let explicit = (0..dom.len())
        .find(|&id| dom.is_tag(id, "strong") || dom.is_tag(id, "em"))
        .filter(|_| opener.leading_emphasis)
        .map(|id| rich_text(&dom, id));
    let generic = |s: &str| {
        s.trim()
            .trim_end_matches(['.', ':', '：'])
            .trim()
            .eq_ignore_ascii_case(block.kind.label(block.direction))
    };
    let subtitle = extract_formula_subtitle(doc, block);

    if let Some(title) = explicit
        .as_ref()
        .filter(|s| !s.trim().is_empty() && !generic(s))
    {
        return (compact(title, false), subtitle);
    }
    if block.name.is_some() || block.number.is_some() {
        return (compact(&block.title, false), subtitle);
    }

    if subtitle.is_some() {
        return (compact(&block.title, false), subtitle);
    }

    let text = node_text(opener);
    let text = explicit
        .as_ref()
        .filter(|s| generic(s))
        .and_then(|title| text.trim_start().strip_prefix(title.trim()))
        .unwrap_or(&text);
    let summary = compact(text, true);
    (
        if summary.is_empty() {
            block.title.clone()
        } else {
            summary
        },
        None,
    )
}

fn extract_formula_subtitle(doc: &Document, block: &SemanticBlock) -> Option<String> {
    if doc.nodes.is_empty() {
        return None;
    }
    let start = block.span.start.min(doc.nodes.len());
    let end = block.span.end.min(doc.nodes.len());
    if start >= end {
        return None;
    }

    let display_maths: Vec<&MathSpan> = doc.nodes[start..end]
        .iter()
        .flat_map(|n| &n.math)
        .filter(|m| m.display)
        .collect();

    if display_maths.is_empty() {
        return None;
    }

    // 1. Prefer \boxed{...} content if present
    for math in &display_maths {
        if let Some(boxed) = extract_boxed(&math.latex) {
            let cleaned = clean_latex(&boxed);
            if cleaned.chars().count() <= 75 && !cleaned.contains("\\begin{") {
                return Some(format!("${cleaned}$"));
            } else {
                return Some(formula_badge(&math.latex));
            }
        }
    }

    // 2. Otherwise take the final display equation in the block (the conclusion)
    let raw_eq = &display_maths.last().unwrap().latex;

    // Indivisible multi-line environments
    if raw_eq.contains("\\begin{bmatrix}")
        || raw_eq.contains("\\begin{pmatrix}")
        || raw_eq.contains("\\det")
    {
        return Some("[Matrix]".into());
    }
    if raw_eq.contains("\\begin{cases}") {
        return Some("[Piecewise / System]".into());
    }

    let conclusion_raw = if raw_eq.contains("\\\\") {
        raw_eq.rsplit("\\\\").next().unwrap_or(raw_eq)
    } else {
        raw_eq.as_str()
    };

    let cleaned = clean_latex(conclusion_raw);

    if cleaned.chars().count() <= 75 && !cleaned.contains("\\begin{") && !cleaned.contains("\\end{")
    {
        Some(format!("${cleaned}$"))
    } else {
        Some(formula_badge(raw_eq))
    }
}

fn extract_boxed(latex: &str) -> Option<String> {
    let idx = latex.find("\\boxed{")?;
    let rest = &latex[idx + 7..];
    let mut depth = 1;
    let mut end = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    if depth == 0 {
        Some(rest[..end].trim().to_string())
    } else {
        None
    }
}

fn clean_latex(latex: &str) -> String {
    let s = latex
        .replace("\\begin{aligned}", "")
        .replace("\\end{aligned}", "")
        .replace("\\begin{equation*}", "")
        .replace("\\end{equation*}", "")
        .replace("\\begin{equation}", "")
        .replace("\\end{equation}", "")
        .replace('&', "");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn formula_badge(latex: &str) -> String {
    if latex.contains("\\begin{bmatrix}")
        || latex.contains("\\begin{pmatrix}")
        || latex.contains("\\det")
    {
        "[Matrix]".into()
    } else if latex.contains("\\begin{cases}") {
        "[Piecewise / System]".into()
    } else if latex.contains("\\int")
        || latex.contains("\\sum")
        || latex.contains("\\prod")
        || latex.contains("\\lim")
    {
        "[Series / Integral]".into()
    } else {
        "[Formula]".into()
    }
}

fn formula(latex: &str) -> String {
    if latex.contains("\\begin{") || latex.contains("\\\\") || latex.chars().count() > 80 {
        formula_badge(latex)
    } else {
        format!(
            "${}$",
            latex.split_whitespace().collect::<Vec<_>>().join(" ")
        )
    }
}

// Read the already parsed inline AST HTML to preserve math at its exact location.
// No markup is retained and entities are decoded by html5ever.
fn node_text(node: &Node) -> String {
    let dom = Dom::parse(&node.html);
    rich_text(&dom, dom.root())
}

fn rich_text(dom: &Dom, id: NodeId) -> String {
    if dom.has_class(id, "math") {
        return formula(&dom.text_content(id));
    }
    match &dom.node(id).data {
        NodeData::Text(text) => text.clone(),
        _ => dom
            .children(id)
            .iter()
            .map(|&child| rich_text(dom, child))
            .collect(),
    }
}

/// Truncate at 90 Unicode characters, treating each $…$ as an indivisible
/// token. Oversized expressions become a formula badge, never a partial TeX.
fn compact(text: &str, first_sentence: bool) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    let mut count = 0;
    while let Some(c) = chars.next() {
        let token = if c == '$' {
            let mut math = String::from("$");
            let mut closed = false;
            let mut escaped = false;
            for m in chars.by_ref() {
                math.push(m);
                if m == '$' && !escaped {
                    closed = true;
                    break;
                }
                escaped = m == '\\' && !escaped;
            }
            if closed && math.chars().count() <= 80 && !math.contains("\\begin{") {
                math
            } else {
                formula_badge(&math)
            }
        } else if c == '\\' && chars.peek() == Some(&'$') {
            chars.next();
            "\\$".into()
        } else {
            c.to_string()
        };
        let size = token.chars().count();
        if count + size > 89 && chars.peek().is_some() || count + size > 90 {
            out = out.trim_end().to_string();
            out.push('…');
            break;
        }
        count += size;
        out.push_str(&token);
        if first_sentence
            && matches!(c, '.' | '!' | '?' | '؟' | '؛')
            && chars.peek().is_none_or(|c| c.is_whitespace())
        {
            break;
        }
    }
    out.trim().to_string()
}

fn detect_edges(doc: &Document, root: &MindMapNode) -> Vec<MindMapEdge> {
    let mut valid_node_ids = HashSet::new();
    fn collect_ids(node: &MindMapNode, out: &mut HashSet<String>) {
        out.insert(node.id.clone());
        for child in &node.children {
            collect_ids(child, out);
        }
    }
    collect_ids(root, &mut valid_node_ids);

    let mut num_targets: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut id_targets: HashSet<String> = HashSet::new();

    let kind_keywords = |kind: BlockType| -> Vec<&'static str> {
        match kind {
            BlockType::Theorem => vec!["theorem", "thm", "قضیه"],
            BlockType::Lemma => vec!["lemma", "lem", "لم"],
            BlockType::Proposition => vec!["proposition", "prop", "گزاره"],
            BlockType::Corollary => vec!["corollary", "cor", "نتیجه"],
            BlockType::Definition => vec!["definition", "defn", "def", "تعریف"],
            BlockType::Example => vec!["example", "exmp", "ex", "مثال"],
            BlockType::Remark => vec!["remark", "rem", "note", "نکته"],
            _ => vec![],
        }
    };

    for block in &doc.blocks {
        if !valid_node_ids.contains(&block.id) {
            continue;
        }
        id_targets.insert(block.id.clone());
        if let Some(ref num) = block.number {
            let folded = fold_digits(num);
            for kw in kind_keywords(block.kind) {
                num_targets.insert((kw.to_string(), folded.clone()), block.id.clone());
            }
        }
    }

    let mut edges: Vec<MindMapEdge> = Vec::new();
    let mut seen_edges: HashSet<(String, String)> = HashSet::new();

    fn scan_node(
        node: &MindMapNode,
        doc: &Document,
        valid_node_ids: &HashSet<String>,
        id_targets: &HashSet<String>,
        num_targets: &BTreeMap<(String, String), String>,
        edges: &mut Vec<MindMapEdge>,
        seen: &mut HashSet<(String, String)>,
    ) {
        if node.id != "mx-main" {
            let Some(block) = doc.blocks.iter().find(|b| b.id == node.id) else {
                for child in &node.children {
                    scan_node(
                        child,
                        doc,
                        valid_node_ids,
                        id_targets,
                        num_targets,
                        edges,
                        seen,
                    );
                }
                return;
            };

            let mut text_corpus = String::new();
            let mut html_corpus = String::new();

            let start = block.span.start.min(doc.nodes.len());
            let end = block.span.end.min(doc.nodes.len());
            if start < end {
                for n in &doc.nodes[start..end] {
                    text_corpus.push(' ');
                    text_corpus.push_str(&n.flat);
                    html_corpus.push(' ');
                    html_corpus.push_str(&n.html);
                }
            } else {
                html_corpus.push_str(&block.html);
            }

            if let Some(proof_block) = node
                .proof_id
                .as_ref()
                .and_then(|pid| doc.blocks.iter().find(|b| b.id == *pid))
            {
                let p_start = proof_block.span.start.min(doc.nodes.len());
                let p_end = proof_block.span.end.min(doc.nodes.len());
                if p_start < p_end {
                    for n in &doc.nodes[p_start..p_end] {
                        text_corpus.push(' ');
                        text_corpus.push_str(&n.flat);
                        html_corpus.push(' ');
                        html_corpus.push_str(&n.html);
                    }
                } else {
                    html_corpus.push_str(&proof_block.html);
                }
            }

            let mut add_edge = |target_id: &str, label: Option<String>| {
                if target_id != node.id && valid_node_ids.contains(target_id) {
                    let key = (node.id.clone(), target_id.to_string());
                    if seen.insert(key) {
                        edges.push(MindMapEdge {
                            from: node.id.clone(),
                            to: target_id.to_string(),
                            label,
                        });
                    }
                }
            };

            // A. Direct href="#..." in HTML
            let mut cursor = 0;
            while let Some(idx) = html_corpus[cursor..].find("href=\"#") {
                let start = cursor + idx + 7;
                if let Some(end_quote) = html_corpus[start..].find('"') {
                    let target_id = &html_corpus[start..start + end_quote];
                    if id_targets.contains(target_id) {
                        add_edge(target_id, Some("uses".into()));
                    }
                    cursor = start + end_quote;
                } else {
                    break;
                }
            }

            // B. Markdown anchor links (#...)
            let mut cursor = 0;
            while let Some(idx) = text_corpus[cursor..].find("(#") {
                let start = cursor + idx + 2;
                if let Some(end_paren) = text_corpus[start..].find(')') {
                    let target_id = &text_corpus[start..start + end_paren];
                    if id_targets.contains(target_id) {
                        add_edge(target_id, Some("uses".into()));
                    }
                    cursor = start + end_paren;
                } else {
                    break;
                }
            }

            // C. Textual citations
            let keywords = [
                "theorem",
                "lemma",
                "proposition",
                "corollary",
                "definition",
                "example",
                "remark",
                "قضیه",
                "لم",
                "گزاره",
                "نتیجه",
                "تعریف",
                "مثال",
                "نکته",
            ];

            for kw in &keywords {
                let mut search_from = 0;
                let text_lower = text_corpus.to_lowercase();
                while let Some(pos) = text_lower[search_from..].find(kw) {
                    let abs_pos = search_from + pos;
                    let after_kw = &text_corpus[abs_pos + kw.len()..];
                    let before_ok = abs_pos == 0 || {
                        let prev_char = text_corpus[..abs_pos].chars().next_back().unwrap();
                        !prev_char.is_alphanumeric()
                    };
                    search_from = abs_pos + kw.len();
                    if !before_ok {
                        continue;
                    }

                    let trimmed = after_kw.trim_start();
                    let whitespace_len = after_kw.len() - trimmed.len();
                    if whitespace_len == 0 && !trimmed.is_empty() {
                        continue;
                    }

                    let mut num_end = 0;
                    for (i, c) in trimmed.char_indices() {
                        if c.is_ascii_digit()
                            || ('۰'..='۹').contains(&c)
                            || ('٠'..='٩').contains(&c)
                            || c == '.'
                            || c == '-'
                            || c == '–'
                        {
                            num_end = i + c.len_utf8();
                        } else {
                            break;
                        }
                    }

                    if num_end > 0 {
                        let raw_num = trimmed[..num_end].trim_end_matches(['.', '-', '–']);
                        if !raw_num.is_empty() {
                            let folded = fold_digits(raw_num);
                            if let Some(target_id) = num_targets.get(&(kw.to_string(), folded)) {
                                let matched_text = format!("{} {}", kw, raw_num);
                                add_edge(target_id, Some(matched_text));
                            }
                        }
                    }
                }
            }
        }

        for child in &node.children {
            scan_node(
                child,
                doc,
                valid_node_ids,
                id_targets,
                num_targets,
                edges,
                seen,
            );
        }
    }

    scan_node(
        root,
        doc,
        &valid_node_ids,
        &id_targets,
        &num_targets,
        &mut edges,
        &mut seen_edges,
    );
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ScoringConfig, analyze};

    #[test]
    fn bilingual_hierarchy_and_proof_exclusion() {
        let source = "# جبر Linear Algebra $V$\n\n## فضاها Spaces\n\n**تعریف ۱ (فضای برداری $V$).** مجموعهٔ $V$ یک فضای برداری است.\n\n### Maps\n\n**Theorem 2 (Rank Nullity).** A linear map satisfies:\n\n$$\\begin{aligned} a&=b\\\\ c&=d \\end{aligned}$$\n\n**Proof.** THIS_LONG_PROOF_MUST_NOT_APPEAR.\n\nThis argument continues for many lines. QED\n\n## Examples\n\n**Example 3.** Take $V=R^2$.\n";
        let source = source.replace(
            "THIS_LONG_PROOF_MUST_NOT_APPEAR.",
            &"THIS_LONG_PROOF_MUST_NOT_APPEAR. ".repeat(200),
        );
        let doc = analyze(&source, &ScoringConfig::default());
        let data = extract(&doc, "fallback");
        let tree = &data.root;
        assert_eq!(tree.id, "mx-main");
        assert_eq!(tree.level, 0);
        assert_eq!(tree.label, "fallback");
        assert_eq!(tree.children.len(), 1);

        let h1 = &tree.children[0];
        assert_eq!(h1.id, "sec-1");
        assert!(h1.label.contains("$V$"));
        assert_eq!(h1.level, 1);

        let spaces = &h1.children[0];
        assert_eq!(spaces.label, "فضاها Spaces");
        assert_eq!(spaces.children.len(), 2);
        assert_eq!(spaces.children[0].node_type, MindMapNodeType::Definition);
        assert!(spaces.children[0].label.contains("$V$"));
        let theorem = &spaces.children[1].children[0];
        assert_eq!(theorem.node_type, MindMapNodeType::Theorem);
        assert!(theorem.proof_id.is_some());
        assert_eq!(theorem.subtitle.as_deref(), Some("$c=d$"));

        assert_eq!(
            h1.children[1].children[0].node_type,
            MindMapNodeType::Example
        );

        let json = serde_json::to_string(&data).unwrap();
        assert!(!json.contains("THIS_LONG_PROOF"));
        assert!(!json.contains("aligned"));

        fn targets(node: &MindMapNode, doc: &Document) {
            if node.id != "mx-main" {
                assert!(doc.blocks.iter().any(|b| b.id == node.id));
            }
            for child in &node.children {
                targets(child, doc);
            }
        }
        targets(tree, &doc);
        assert_eq!(data, extract(&doc, "fallback"));
    }

    #[test]
    fn truncation_is_unicode_and_math_safe() {
        let escaped = r"Price $\text{cost \$5}$ is exact.";
        assert_eq!(compact(escaped, false), escaped);
        for prefix in 0..110 {
            let input = format!("{} $x^2 + y^2 = z^2$ ادامه", "ف".repeat(prefix));
            let label = compact(&input, false);
            assert!(label.chars().count() <= 90);
            assert_eq!(label.matches('$').count() % 2, 0);
            assert!(!label.contains('$') || label.contains("$x^2 + y^2 = z^2$"));
        }
        assert_eq!(
            compact("First sentence. Long body follows.", true),
            "First sentence."
        );
    }

    #[test]
    fn generic_opener_uses_a_short_sentence_and_imported_h1_keeps_title() {
        let doc = analyze(
            "# Title\n\n**Definition.** A space $V$ has a basis. A very long discussion follows.",
            &ScoringConfig::default(),
        );
        let data = extract(&doc, "file.md");
        assert_eq!(
            data.root.children[0].children[0].label,
            "A space $V$ has a basis."
        );
        let imported = crate::analyze_html("<h1>جبر Algebra</h1><p>Text</p>");
        assert_eq!(
            extract(&imported, "file.html").root.children[0].label,
            "جبر Algebra"
        );
    }

    #[test]
    fn skipped_levels_multiple_h1_and_untitled_equations() {
        let doc = analyze(
            "# First\n\n#### Deep\n\n\\begin{definition}\n$$x=y$$\n\\end{definition}\n\n# Second\n\n## Child",
            &ScoringConfig::default(),
        );
        let data = extract(&doc, "file.md");
        assert_eq!(data.root.children.len(), 2);
        assert_eq!(data.root.children[0].label, "First");
        assert_eq!(data.root.children[1].label, "Second");
        assert_eq!(
            data.root.children[0].children[0].children[0]
                .subtitle
                .as_deref(),
            Some("$x=y$")
        );
        assert_eq!(data.root.children[1].children[0].label, "Child");
        let empty = extract(
            &analyze("ordinary text", &ScoringConfig::default()),
            "file.md",
        );
        assert_eq!(empty.root.label, "file.md");
        assert_eq!(empty.root.id, "mx-main");
        assert!(empty.root.children.is_empty());
    }

    #[test]
    fn proof_and_solution_linking() {
        let source = "# Statements\n\n**Theorem 1.** Statement of theorem.\n\n**Proof.** Proof of theorem.\n\n**Example 1.** Sample example.\n\n**Solution.** Solution to example.";
        let doc = analyze(source, &ScoringConfig::default());
        let data = extract(&doc, "file.md");
        let thm = &data.root.children[0].children[0];
        assert_eq!(thm.node_type, MindMapNodeType::Theorem);
        assert!(thm.proof_id.is_some());

        let ex = &data.root.children[0].children[1];
        assert_eq!(ex.node_type, MindMapNodeType::Example);
        assert!(ex.proof_id.is_some());
    }

    #[test]
    fn key_term_extraction_from_prose() {
        let source = "# Section 1\n\nIn mathematics, a **vector space** is defined over a field. Furthermore, a subset is called a **basis** if it spans.\n\nدر جبر، **پایه** نامیده می‌شود و همچنین **بعد** := ۲ است.";
        let doc = analyze(source, &ScoringConfig::default());
        let data = extract(&doc, "file.md");
        let section = &data.root.children[0];
        assert_eq!(section.node_type, MindMapNodeType::Heading);
        assert!(
            section
                .key_terms
                .iter()
                .any(|t| t == "vector space" || t == "basis")
        );
        assert!(
            section
                .key_terms
                .iter()
                .any(|t| t.contains("پایه") || t.contains("بعد"))
        );
        assert!(section.key_terms.len() <= 5);
    }

    #[test]
    fn semantic_formula_badges_and_boxed() {
        let source = "## Formulas\n\n**Theorem 1.**\n$$\\boxed{E = mc^2}$$\n\n**Theorem 2.**\n$$\\begin{bmatrix} 1 & 0 & 0 \\\\ 0 & 1 & 0 \\\\ 0 & 0 & 1 \\end{bmatrix}$$\n\n**Theorem 3.**\n$$\\begin{cases} x & x > 0 \\\\ -x & x \\le 0 \\end{cases}$$\n\n**Theorem 4.**\n$$\\int_{-\\infty}^\\infty e^{-x^2} dx = \\sqrt{\\pi}$$\n";
        let doc = analyze(source, &ScoringConfig::default());
        let data = extract(&doc, "formulas.md");
        let sec = &data.root.children[0];
        assert_eq!(sec.children[0].subtitle.as_deref(), Some("$E = mc^2$"));
        assert_eq!(sec.children[1].subtitle.as_deref(), Some("[Matrix]"));
        assert_eq!(
            sec.children[2].subtitle.as_deref(),
            Some("[Piecewise / System]")
        );
        assert_eq!(
            sec.children[3].subtitle.as_deref(),
            Some(r"$\int_{-\infty}^\infty e^{-x^2} dx = \sqrt{\pi}$")
        );
    }

    #[test]
    fn cross_reference_edge_detection_bilingual() {
        let source = "# Content\n\n**Definition 1.** A vector space $V$.\n\n**Theorem 1.** By Definition 1, $V$ is non-empty.\n\n**تعریف ۲.** میدان $\\mathbb{R}$.\n\n**قضیه ۲.** طبق تعریف ۲ میدان پیوسته است. همچنین (#block-theorem-1) استفاده شده است.\n";
        let doc = analyze(source, &ScoringConfig::default());
        let data = extract(&doc, "crossref.md");

        // Edge from Theorem 1 -> Definition 1
        assert!(data.edges.iter().any(|e| e.to == "block-definition-1"));
        // Edge from قضیه ۲ -> تعریف ۲
        assert!(data.edges.iter().any(|e| e.to == "block-definition-2"));
        // Edge from قضیه ۲ -> Theorem 1 via explicit anchor (#block-theorem-1)
        assert!(data.edges.iter().any(|e| e.to == "block-theorem-1"));
    }
}
