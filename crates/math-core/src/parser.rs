//! Markdown + LaTeX → flat list of top-level [`Node`]s.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};

use crate::model::{MathSpan, Node, NodeKind};

fn options() -> Options {
    Options::ENABLE_MATH
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
}

/// Splits the source into top-level nodes, keeping for each one its rendered
/// HTML, its prose, and its LaTeX spans (`$…$` and `$$…$$`).
pub fn parse_markdown(src: &str) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut buf: Vec<Event> = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (event, range) in Parser::new_ext(src, options()).into_offset_iter() {
        match event {
            Event::Start(_) => {
                if depth == 0 {
                    start = range.start;
                }
                depth += 1;
                buf.push(event);
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                buf.push(event);
                if depth == 0 {
                    nodes.push(build(std::mem::take(&mut buf), start..range.end));
                }
            }
            _ if depth == 0 => nodes.push(build(vec![event], range)),
            _ => buf.push(event),
        }
    }
    nodes
}

fn build(events: Vec<Event>, source: Range<usize>) -> Node {
    let mut kind = match events.first() {
        Some(Event::Start(Tag::Heading { level, .. })) => NodeKind::Heading(*level as u8),
        Some(Event::Start(Tag::Paragraph)) => NodeKind::Paragraph,
        Some(Event::Start(Tag::List(_))) => NodeKind::List,
        Some(Event::Start(Tag::BlockQuote(_))) => NodeKind::Quote,
        Some(Event::Start(Tag::CodeBlock(_))) => NodeKind::Code,
        Some(Event::Start(Tag::Table(_))) => NodeKind::Table,
        Some(Event::Rule) => NodeKind::Rule,
        _ => NodeKind::Other,
    };

    // "**Theorem 1.** …" — possibly inside a block quote.
    let leading_emphasis = events
        .iter()
        .find(|e| !matches!(e, Event::Start(Tag::Paragraph | Tag::BlockQuote(_))))
        .is_some_and(|e| matches!(e, Event::Start(Tag::Strong | Tag::Emphasis)));

    let (mut text, mut flat, mut math) = (String::new(), String::new(), Vec::new());
    for event in &events {
        match event {
            Event::Text(t) | Event::Code(t) => {
                text.push_str(t);
                flat.push_str(t);
            }
            Event::InlineMath(latex) | Event::DisplayMath(latex) => {
                flat.push(' ');
                flat.push_str(latex);
                flat.push(' ');
                math.push(MathSpan {
                    latex: latex.to_string(),
                    display: matches!(event, Event::DisplayMath(_)),
                });
            }
            Event::SoftBreak
            | Event::HardBreak
            | Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::TableCell) => {
                text.push(' ');
                flat.push(' ');
            }
            _ => {}
        }
    }

    if kind == NodeKind::Paragraph && text.trim().is_empty() && math.iter().any(|m| m.display) {
        kind = NodeKind::DisplayMath;
    }

    let mut out = String::new();
    if matches!(kind, NodeKind::Heading(_)) && events.len() >= 2 {
        let inner = events[1..events.len() - 1].iter().cloned();
        html::push_html(&mut out, inner);
    } else {
        html::push_html(&mut out, events.into_iter());
    }

    Node {
        kind,
        text,
        flat,
        math,
        leading_emphasis,
        html: out,
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_top_level_nodes_and_extracts_math() {
        let src =
            "# عنوان\n\n**قضیه ۱.** اگر $x > 0$ آنگاه\n$$x^2 > 0.$$\n\n$$a = b$$\n\n- یک\n- دو\n";
        let nodes = parse_markdown(src);
        let kinds: Vec<_> = nodes.iter().map(|n| n.kind).collect();
        assert_eq!(
            kinds,
            [
                NodeKind::Heading(1),
                NodeKind::Paragraph,
                NodeKind::DisplayMath,
                NodeKind::List
            ]
        );

        assert_eq!(nodes[0].html, "عنوان");
        let thm = &nodes[1];
        assert!(thm.leading_emphasis);
        assert!(thm.text.starts_with("قضیه ۱."));
        assert!(!thm.text.contains("x^2"));
        assert!(thm.flat.contains("x^2 > 0."));
        assert_eq!(thm.math.len(), 2);
        assert!(!thm.math[0].display && thm.math[1].display);
        assert!(
            thm.html
                .contains(r#"<span class="math math-inline">x &gt; 0</span>"#)
        );
        assert_eq!(
            &src[thm.source.clone()].chars().take(2).collect::<String>(),
            "**"
        );
    }

    #[test]
    fn math_is_not_parsed_as_markdown() {
        let nodes = parse_markdown("برای $a_i * b_j * c_k$ داریم $x_1 _ y_2$.");
        assert_eq!(nodes[0].math[0].latex, "a_i * b_j * c_k");
        assert!(!nodes[0].html.contains("<em>"));
    }
}
