//! Markdown + LaTeX → flat list of top-level [`Node`]s.

use std::ops::Range;

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};

use crate::model::{BlockType, EnvSpan, MathSpan, Node, NodeKind};
use crate::normalize::{Zone, begin_at, zones};

fn options() -> Options {
    Options::ENABLE_MATH
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
}

/// Splits the source into top-level nodes, keeping for each one its rendered
/// HTML, its prose, and its LaTeX spans (`$…$` and `$$…$$`).
///
/// LaTeX theorem-like environments are cut out first: their `\begin` / `\end`
/// lines produce no node, and the first node of each body carries an
/// [`EnvSpan`]. Raw HTML in the source is shown as text, never passed through.
pub fn parse_markdown(src: &str) -> Vec<Node> {
    let mut nodes = Vec::new();
    for chunk in split_environments(src) {
        let first = nodes.len();
        parse_chunk(src, chunk.range, &mut nodes);
        if let Some((kind, name)) = chunk.env
            && nodes.len() > first
        {
            nodes[first].env = Some(EnvSpan {
                kind,
                name,
                len: nodes.len() - first,
            });
        }
    }
    nodes
}

struct Chunk {
    range: Range<usize>,
    env: Option<(BlockType, Option<String>)>,
}

/// Cuts the source at `\begin{theorem}[Name] … \end{theorem}` (and the other
/// environments [`BlockType::from_env_name`] knows). The `\begin` must start
/// its line; environments inside code or math are left alone.
fn split_environments(src: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    if !src.contains("\\begin{") {
        chunks.push(Chunk {
            range: 0..src.len(),
            env: None,
        });
        return chunks;
    }
    let z = zones(src);
    let (mut i, mut plain_start) = (0, 0);
    while let Some(k) = src[i..].find("\\begin{") {
        let at = i + k;
        i = at + 1;
        let line_start = src[..at].rfind('\n').map_or(0, |n| n + 1);
        if z[at] != Zone::Text || !src[line_start..at].trim().is_empty() {
            continue;
        }
        let Some((name, mut after)) = begin_at(src, at) else {
            continue;
        };
        let Some(kind) = BlockType::from_env_name(name) else {
            continue;
        };
        let end_tag = format!("\\end{{{name}}}");
        let mut search = after;
        let end = loop {
            match src[search..].find(&end_tag) {
                Some(e) if z[search + e] == Zone::Text => break Some(search + e),
                Some(e) => search += e + 1,
                None => break None,
            }
        };
        let Some(end) = end else {
            continue;
        };

        let mut title = None;
        if src[after..end].starts_with('[')
            && let Some(close) = src[after..end].find(']')
        {
            title =
                Some(src[after + 1..after + close].trim().to_string()).filter(|t| !t.is_empty());
            after += close + 1;
        }
        // Text on the `\begin` line must not look like an indented code block.
        after += src[after..end].len() - src[after..end].trim_start_matches([' ', '\t']).len();

        chunks.push(Chunk {
            range: plain_start..line_start,
            env: None,
        });
        chunks.push(Chunk {
            range: after..end,
            env: Some((kind, title)),
        });
        i = end + end_tag.len();
        plain_start = i;
    }
    chunks.push(Chunk {
        range: plain_start..src.len(),
        env: None,
    });
    chunks
}

fn parse_chunk(src: &str, chunk: Range<usize>, nodes: &mut Vec<Node>) {
    let mut buf: Vec<Event> = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let base = chunk.start;

    for (event, range) in Parser::new_ext(&src[chunk], options()).into_offset_iter() {
        let range = base + range.start..base + range.end;
        let event = sanitize(event);
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
}

/// Raw HTML becomes visible text, and link / image targets are limited to
/// schemes that cannot run script.
fn sanitize(event: Event) -> Event {
    match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: safe_url(dest_url, false),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: safe_url(dest_url, true),
            title,
            id,
        }),
        other => other,
    }
}

fn safe_url(url: CowStr, image: bool) -> CowStr {
    // Browsers ignore whitespace and control characters inside a scheme.
    let compact: String = url
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect();
    let scheme = compact
        .split_once(':')
        .map(|(scheme, _)| scheme)
        .filter(|s| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        });
    let allowed = match scheme.map(str::to_ascii_lowercase).as_deref() {
        None => true,
        Some("http" | "https") => true,
        Some("mailto") => !image,
        Some("data") => image && compact.to_ascii_lowercase().starts_with("data:image/"),
        Some(_) => false,
    };
    if allowed {
        url
    } else {
        CowStr::Borrowed(if image { "" } else { "#" })
    }
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
        env: None,
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
    fn latex_environments_mark_their_first_node() {
        let src = "Intro.\n\n\\begin{theorem}[Cauchy] Let $G$ be finite.\n\n$$|G| = n$$\n\\end{theorem}\n\n\\begin{proof}\nTrivial.\n\\end{proof}\nAfter.\n\n`\\begin{lemma}` and \\begin{itemize} stay.";
        let nodes = parse_markdown(src);
        let texts: Vec<_> = nodes.iter().map(|n| n.text.trim()).collect();
        assert_eq!(
            texts,
            [
                "Intro.",
                "Let  be finite.",
                "",
                "Trivial.",
                "After.",
                "\\begin{lemma} and \\begin{itemize} stay."
            ]
        );
        assert_eq!(
            nodes[1].env,
            Some(EnvSpan {
                kind: BlockType::Theorem,
                name: Some("Cauchy".into()),
                len: 2
            })
        );
        assert_eq!(
            nodes[3].env.as_ref().map(|e| (e.kind, e.len)),
            Some((BlockType::Proof, 1))
        );
        assert!(nodes[2].env.is_none() && nodes[4].env.is_none());
        assert_eq!(src[nodes[3].source.clone()].trim(), "Trivial.");
    }

    #[test]
    fn raw_html_and_script_urls_are_neutralised() {
        let nodes = parse_markdown(
            "<script>alert(1)</script>\n\nText <img src=x onerror=alert(1)> here.\n\n\
             [a](javascript:alert(1)) [b](JAVA\tSCRIPT:x) [c](https://example.org) [d](#sec-1) \
             ![e](data:text/html,x) ![f](fig/a.png)",
        );
        let html: String = nodes.iter().map(|n| n.html.as_str()).collect();
        assert!(!html.contains("<script") && !html.contains("<img src=x"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.to_lowercase().contains("javascript:"));
        assert!(html.contains(r##"<a href="#">a</a>"##));
        assert!(html.contains(r#"<a href="https://example.org">c</a>"#));
        assert!(html.contains(r##"<a href="#sec-1">d</a>"##));
        assert!(html.contains(r#"<img src="" alt="e" />"#));
        assert!(html.contains(r#"<img src="fig/a.png" alt="f" />"#));
    }

    #[test]
    fn math_is_not_parsed_as_markdown() {
        let nodes = parse_markdown("برای $a_i * b_j * c_k$ داریم $x_1 _ y_2$.");
        assert_eq!(nodes[0].math[0].latex, "a_i * b_j * c_k");
        assert!(!nodes[0].html.contains("<em>"));
    }
}
