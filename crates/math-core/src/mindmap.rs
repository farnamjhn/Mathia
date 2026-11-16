//! Deterministic, compact navigation tree extracted from the compiler's AST.
//! This module never calls a judge, network service, or language model.
use serde::{Deserialize, Serialize};

use crate::dom::{Dom, NodeData, NodeId};
use crate::model::{BlockType, Document, Node, NodeKind, SemanticBlock};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MindMapNode {
    /// Existing compiled HTML target, shared with the table of contents.
    pub id: String,
    /// Plain text with atomic inline $math$, never trusted HTML.
    pub label: String,
    pub subtitle: Option<String>,
    pub node_type: MindMapNodeType,
    pub level: u8,
    pub children: Vec<MindMapNode>,
}

/// The first H1 is the root; subsequent headings follow their actual levels,
/// including skipped levels. `fallback_title` is normally the output filename.
pub fn extract(doc: &Document, fallback_title: &str) -> MindMapNode {
    let first_h1 = doc
        .blocks
        .iter()
        .position(|b| b.kind == BlockType::Heading && b.heading_level == Some(1));
    let mut root = MindMapNode {
        id: first_h1
            .map_or("mx-main", |i| doc.blocks[i].id.as_str())
            .into(),
        label: first_h1
            .map(|i| {
                let block = &doc.blocks[i];
                doc.nodes
                    .get(block.span.start)
                    .map(|n| compact(&node_text(n), false))
                    .unwrap_or_else(|| compact(&block.title, false))
            })
            .unwrap_or_else(|| compact(fallback_title, false)),
        subtitle: None,
        node_type: MindMapNodeType::Root,
        level: 0,
        children: Vec::new(),
    };
    // (heading level, path from root). Semantic blocks never enter this stack.
    let mut stack: Vec<(u8, Vec<usize>)> = Vec::new();
    for (index, block) in doc.blocks.iter().enumerate() {
        if Some(index) == first_h1 {
            stack.clear();
            stack.push((1, Vec::new()));
            continue;
        }
        let kind = match block.kind {
            BlockType::Heading => MindMapNodeType::Heading,
            BlockType::Definition => MindMapNodeType::Definition,
            BlockType::Theorem => MindMapNodeType::Theorem,
            BlockType::Lemma => MindMapNodeType::Lemma,
            BlockType::Corollary => MindMapNodeType::Corollary,
            BlockType::Proposition => MindMapNodeType::Proposition,
            BlockType::Example => MindMapNodeType::Example,
            BlockType::Remark | BlockType::Notation => MindMapNodeType::Note,
            _ => continue, // In particular, proofs/solutions and ordinary prose.
        };
        let heading = block.kind == BlockType::Heading;
        let level = if heading {
            block.heading_level.unwrap_or(2).clamp(1, 6)
        } else {
            stack.last().map_or(1, |(level, _)| level + 1)
        };
        if heading {
            while stack.last().is_some_and(|(active, _)| *active >= level) {
                stack.pop();
            }
        }
        let mut path = stack.last().map_or_else(Vec::new, |(_, path)| path.clone());
        let mut parent = &mut root;
        for &child in &path {
            parent = &mut parent.children[child];
        }
        let (label, subtitle) = preview(doc, block);
        path.push(parent.children.len());
        parent.children.push(MindMapNode {
            id: block.id.clone(),
            label,
            subtitle,
            node_type: kind,
            level,
            children: Vec::new(),
        });
        if heading {
            stack.push((level, path));
        }
    }
    root
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
    if let Some(title) = explicit
        .as_ref()
        .filter(|s| !s.trim().is_empty() && !generic(s))
    {
        return (compact(title, false), None);
    }
    if block.name.is_some() || block.number.is_some() {
        return (compact(&block.title, false), None);
    }
    let equation = doc.nodes[block.span.clone()]
        .iter()
        .flat_map(|n| &n.math)
        .find(|m| m.display);
    if let Some(eq) = equation {
        return (compact(&block.title, false), Some(formula(&eq.latex)));
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

fn formula(latex: &str) -> String {
    if latex.contains("\\begin{") || latex.contains("\\\\") || latex.chars().count() > 80 {
        "ƒ(x)".into()
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
                "ƒ(x)".into()
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
        let tree = extract(&doc, "fallback");
        assert_eq!(tree.id, "sec-1");
        assert!(tree.label.contains("$V$"));
        assert_eq!(tree.children.len(), 2);
        let spaces = &tree.children[0];
        assert_eq!(spaces.label, "فضاها Spaces");
        assert_eq!(spaces.children.len(), 2);
        assert_eq!(spaces.children[0].node_type, MindMapNodeType::Definition);
        assert!(spaces.children[0].label.contains("$V$"));
        assert_eq!(
            spaces.children[1].children[0].node_type,
            MindMapNodeType::Theorem
        );
        assert_eq!(
            tree.children[1].children[0].node_type,
            MindMapNodeType::Example
        );
        let json = serde_json::to_string(&tree).unwrap();
        assert!(!json.contains("THIS_LONG_PROOF"));
        assert!(!json.contains("aligned"));
        fn targets(node: &MindMapNode, doc: &Document) {
            assert!(doc.blocks.iter().any(|b| b.id == node.id));
            for child in &node.children {
                targets(child, doc);
            }
        }
        targets(&tree, &doc);
        assert_eq!(tree, extract(&doc, "fallback"));
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
        let tree = extract(&doc, "file.md");
        assert_eq!(tree.children[0].label, "A space $V$ has a basis.");
        let imported = crate::analyze_html("<h1>جبر Algebra</h1><p>Text</p>");
        assert_eq!(extract(&imported, "file.html").label, "جبر Algebra");
    }

    #[test]
    fn skipped_levels_multiple_h1_and_untitled_equations() {
        let doc = analyze(
            "# First\n\n#### Deep\n\n\\begin{definition}\n$$x=y$$\n\\end{definition}\n\n# Second\n\n## Child",
            &ScoringConfig::default(),
        );
        let tree = extract(&doc, "file.md");
        assert_eq!(tree.children.len(), 2);
        assert_eq!(
            tree.children[0].children[0].subtitle.as_deref(),
            Some("$x=y$")
        );
        assert_eq!(tree.children[1].children[0].label, "Child");
        let empty = extract(
            &analyze("ordinary text", &ScoringConfig::default()),
            "file.md",
        );
        assert_eq!(empty.label, "file.md");
        assert_eq!(empty.id, "mx-main");
        assert!(empty.children.is_empty());
    }
}
