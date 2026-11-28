//! A compiled [`Document`] rendered back to Markdown.
//!
//! The output follows the conventions Mathia detects (the same ones the
//! transcription prompt asks for): a semantic block starts with its bold label
//! — `**Theorem 1.**` — and math keeps its `$ … $` delimiters, so the exported
//! file compiles back to the document it came from.

use crate::dom::{Dom, NodeData, NodeId};
use crate::model::{BlockType, Direction, Document, EnvSpan, SemanticBlock};

/// Renders `doc` as Markdown.
///
/// `source` is the normalised Markdown the document was parsed from. With it,
/// every node is copied verbatim, so emphasis, links, tables and code survive
/// untouched. A document imported from HTML has no Markdown source: pass
/// `None` and its block HTML is converted instead.
pub fn render(doc: &Document, source: Option<&str>) -> String {
    let mut out = String::new();
    for block in &doc.blocks {
        let md = block_markdown(doc, block, source);
        if md.is_empty() {
            continue;
        }
        out.push_str(&md);
        out.push_str("\n\n");
    }
    tidy(out)
}

/// Renders a single [`SemanticBlock`] as standardized Markdown (adding bold
/// labels or `#` heading markers when needed).
pub fn block_markdown(doc: &Document, block: &SemanticBlock, source: Option<&str>) -> String {
    let body = match source {
        Some(source) => from_source(doc, block, source),
        None => html_to_markdown(&block.html),
    };
    let env = source
        .and_then(|_| doc.nodes.get(block.span.start))
        .and_then(|node| node.env.as_ref());
    let mut out = String::new();
    push_block(&mut out, block, &body, env);
    out.trim().to_string()
}

/// Returns a single [`SemanticBlock`]'s verbatim Markdown for in-place editing.
///
/// When `source` is `Some`, preserves the exact authored text (restoring any
/// surrounding `\begin{...}...\end{...}` lines) rather than synthesizing new
/// labels on unlabelled blocks. When `source` is `None` (HTML import), falls
/// back to [`block_markdown`].
pub fn raw_block_markdown(doc: &Document, block: &SemanticBlock, source: Option<&str>) -> String {
    let Some(source) = source else {
        return block_markdown(doc, block, None);
    };
    if let Some(range) = block_source_range(doc, block, source)
        && let Some(slice) = source.get(range)
    {
        return slice.trim().to_string();
    }
    block_markdown(doc, block, Some(source))
}

/// Byte range of `block` inside `source` (the normalised Markdown), including
/// any outer `\begin{env}...\end{env}` wrapper lines and trimmed of trailing
/// line endings.
pub fn block_source_range(
    doc: &Document,
    block: &SemanticBlock,
    source: &str,
) -> Option<std::ops::Range<usize>> {
    let first = doc.nodes.get(block.span.start)?;
    let last_idx = block.span.end.checked_sub(1)?;
    let last = doc.nodes.get(last_idx)?;
    let mut start = first.source.start;
    let mut end = last.source.end;
    if start > source.len() || end > source.len() || start > end {
        return None;
    }

    if first.env.is_some() {
        // Walk backwards across the `\begin{...}` line that split_environments cut out.
        let before = &source[..start];
        if let Some(begin_pos) = before.rfind("\\begin{") {
            let line_start = before[..begin_pos].rfind('\n').map_or(0, |n| n + 1);
            if before[line_start..begin_pos].trim().is_empty()
                && let Some((env_name, _)) = crate::normalize::begin_at(before, begin_pos)
            {
                start = line_start;
                let end_tag = format!("\\end{{{env_name}}}");
                let after = &source[end..];
                if let Some(end_rel) = after.find(&end_tag) {
                    end += end_rel + end_tag.len();
                }
            }
        }
    }

    while end > start && matches!(source.as_bytes()[end - 1], b'\n' | b'\r') {
        end -= 1;
    }
    Some(start..end)
}

/// The block's own Markdown, copied out of the normalised source.
fn from_source(doc: &Document, block: &SemanticBlock, source: &str) -> String {
    let mut out = String::new();
    let mut previous_list: Option<bool> = None;
    for index in block.span.clone() {
        let Some(node) = doc.nodes.get(index) else {
            continue;
        };
        let Some(text) = source.get(node.source.clone()) else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let ordered = node.list.map(|list| list.ordered);
        if !out.is_empty() {
            // Items of one list stay on consecutive lines; everything else is
            // its own paragraph.
            if ordered.is_some() && ordered == previous_list {
                out.push('\n');
            } else {
                out.push_str("\n\n");
            }
        }
        out.push_str(text);
        previous_list = ordered;
    }
    out
}

fn push_block(out: &mut String, block: &SemanticBlock, body: &str, env: Option<&EnvSpan>) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    // A LaTeX environment keeps its own form; the parser cuts the
    // `\begin` / `\end` lines out, so put them back.
    if let Some(env) = env {
        let name = block.kind.slug();
        out.push_str(&format!("\\begin{{{name}}}"));
        if let Some(label) = &env.name {
            out.push('[');
            out.push_str(label);
            out.push(']');
        }
        out.push('\n');
        out.push_str(body);
        out.push_str(&format!("\n\\end{{{name}}}\n\n"));
        return;
    }
    if block.kind == BlockType::Heading {
        if body.starts_with('#') {
            out.push_str(body);
        } else {
            let level = block.heading_level.unwrap_or(1).clamp(1, 6) as usize;
            out.push_str(&"#".repeat(level));
            out.push(' ');
            out.push_str(body);
        }
        out.push_str("\n\n");
        return;
    }
    let mut body = body.to_string();
    // A heading that opened the block is its title line already; any other
    // semantic block gets a label, unless its opener carries one.
    if block.kind.is_semantic() && !body.starts_with('#') {
        body = if let Some(relabelled) = relabel(&body, &block.title, block.kind) {
            // The opener names another type: the user retyped the block.
            relabelled
        } else if has_label(&body) {
            body
        } else {
            match bold_label(&body, &block.title) {
                Some(labelled) => labelled,
                None if starts_paragraph(&body) => format!("**{}.** {body}", block.title),
                None => format!("**{}.**\n\n{body}", block.title),
            }
        };
    }
    out.push_str(&body);
    out.push_str("\n\n");
}

/// Whether the body already opens with a bold label.
fn has_label(body: &str) -> bool {
    let first = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim_start();
    ["**", "__", "*", "_"]
        .iter()
        .any(|marker| first.starts_with(marker) && !first.starts_with(&format!("{marker} ")))
}

/// Rewrites a bold label that names another type — `**Theorem 1.**` on a block
/// the user corrected to a lemma — to the block's own label.
fn relabel(body: &str, title: &str, kind: BlockType) -> Option<String> {
    let first_end = body.find('\n').unwrap_or(body.len());
    let first = body[..first_end].trim_start();
    let marker = ["**", "__"]
        .into_iter()
        .find(|marker| first.starts_with(marker))?;
    let close = first[marker.len()..].find(marker)? + marker.len();
    let inner = first[marker.len()..close].trim();
    // Only a label that names another type is rewritten.
    if !matches!(label_kind(inner), Some(existing) if existing != kind) {
        return None;
    }
    let after = first[close + marker.len()..].trim_start();
    let separator = if after.is_empty() || after.starts_with([':', ',', ';', '.', '،']) {
        ""
    } else {
        " "
    };
    let tail = &body[first_end..];
    Some(format!("**{title}.**{separator}{after}{tail}"))
}

/// The type a bold run names (`Theorem 1.` → [`BlockType::Theorem`]).
fn label_kind(text: &str) -> Option<BlockType> {
    BlockType::SEMANTIC.into_iter().find(|kind| {
        [Direction::Ltr, Direction::Rtl]
            .into_iter()
            .any(|direction| {
                let label = kind.label(direction);
                text.get(..label.len())
                    .is_some_and(|head| head.eq_ignore_ascii_case(label))
            })
    })
}

/// Bolds the label the opener already spells out in plain text
/// (`Theorem 1. …` → `**Theorem 1.** …`), so no label is written twice.
fn bold_label(body: &str, title: &str) -> Option<String> {
    let first_end = body.find('\n').unwrap_or(body.len());
    let first = body[..first_end].trim_start();
    let head: String = first.chars().take(title.chars().count()).collect();
    if !head.eq_ignore_ascii_case(title) {
        return None;
    }
    let after_title: String = first.chars().skip(title.chars().count()).collect();
    // `Theorem 1` must not eat the `0` of `Theorem 10`.
    if after_title
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric())
    {
        return None;
    }
    // A label may run past the keyword: `Proof of Claim 2.3.` or
    // `Theorem 2.4 (Lagrange).` — bold the whole sentence, not just the word.
    let label_end = first
        .char_indices()
        .filter(|(_, c)| *c == '.')
        .map(|(index, _)| index + 1)
        .find(|&end| first[end..].chars().next().is_none_or(char::is_whitespace))
        .filter(|&end| end <= 60);
    let (label, rest) = match label_end {
        Some(end) => (&first[..end], first[end..].trim_start()),
        None => {
            let rest = after_title.trim_start();
            let rest = rest.strip_prefix('.').unwrap_or(rest).trim_start();
            (title, rest)
        }
    };
    let separator = if rest.is_empty() || rest.starts_with([':', ',', ';', '.', '،']) {
        ""
    } else {
        " "
    };
    let tail = &body[first_end..];
    let bold = if label.ends_with('.') {
        format!("**{label}**")
    } else {
        format!("**{label}.**")
    };
    Some(format!("{bold}{separator}{rest}{tail}"))
}

/// Whether a label can sit at the start of the body's first line.
fn starts_paragraph(body: &str) -> bool {
    let first = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim_start();
    if first.starts_with('#')
        || first.starts_with("$$")
        || first.starts_with("```")
        || first.starts_with('|')
        || first.starts_with('>')
        || first.starts_with("---")
        || first.starts_with("\\begin{")
    {
        return false;
    }
    if first.starts_with("- ") || first.starts_with("* ") || first.starts_with("+ ") {
        return false;
    }
    let digits = first.chars().take_while(|c| c.is_ascii_digit()).count();
    !(digits > 0 && first[digits..].starts_with(". "))
}

/// Collapses runs of blank lines and trims trailing whitespace.
fn tidy(text: String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Converts a block's HTML (as the HTML importer stores it) to Markdown.
fn html_to_markdown(html: &str) -> String {
    if html.trim().is_empty() {
        return String::new();
    }
    let dom = Dom::parse(html);
    let root = dom
        .find_all(dom.root(), |dom, id| dom.is_tag(id, "body"))
        .first()
        .copied()
        .unwrap_or_else(|| dom.root());
    let mut writer = Writer {
        dom: &dom,
        out: String::new(),
    };
    writer.blocks(root);
    writer.out
}

struct Writer<'a> {
    dom: &'a Dom,
    out: String,
}

impl Writer<'_> {
    fn blocks(&mut self, id: NodeId) {
        for child in self.dom.children(id).to_vec() {
            self.block(child);
        }
    }

    fn block(&mut self, id: NodeId) {
        match &self.dom.node(id).data {
            NodeData::Text(text) => {
                let text = collapse_ws(text);
                self.push_block_text(&text);
            }
            NodeData::Element(element) => {
                let tag = element.tag().to_ascii_lowercase();
                match tag.as_str() {
                    "p" => {
                        let inline = self.inline_children(id);
                        self.push_block_text(&inline);
                    }
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        let level = tag[1..].parse::<usize>().unwrap_or(1);
                        let inline = self.inline_children(id);
                        let inline = inline.trim();
                        if inline.is_empty() {
                            return;
                        }
                        self.blank();
                        self.out.push_str(&"#".repeat(level));
                        self.out.push(' ');
                        self.out.push_str(inline);
                        self.out.push_str("\n\n");
                    }
                    "ul" | "ol" => self.list(id, tag == "ol"),
                    "blockquote" => {
                        let mut inner = Writer {
                            dom: self.dom,
                            out: String::new(),
                        };
                        inner.blocks(id);
                        let quoted: Vec<String> = inner
                            .out
                            .trim()
                            .lines()
                            .map(|line| {
                                if line.trim().is_empty() {
                                    ">".to_string()
                                } else {
                                    format!("> {line}")
                                }
                            })
                            .collect();
                        self.push_block_text(&quoted.join("\n"));
                    }
                    "pre" => self.code_block(id),
                    "hr" => {
                        self.blank();
                        self.out.push_str("---\n\n");
                    }
                    "table" => self.table(id),
                    "svg" => {}
                    "br" => self.out.push('\n'),
                    _ => {
                        if self.has_block_children(id) {
                            self.blocks(id);
                        } else {
                            let inline = self.inline_children(id);
                            self.push_block_text(&inline);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn inline_children(&mut self, id: NodeId) -> String {
        let mut inner = Writer {
            dom: self.dom,
            out: String::new(),
        };
        for child in self.dom.children(id).to_vec() {
            inner.inline(child);
        }
        inner.out
    }

    fn inline(&mut self, id: NodeId) {
        match &self.dom.node(id).data {
            NodeData::Text(text) => self.out.push_str(&normalize_inline(text)),
            NodeData::Element(element) => {
                let tag = element.tag().to_ascii_lowercase();
                match tag.as_str() {
                    "strong" | "b" => self.wrap(id, "**"),
                    "em" | "i" => self.wrap(id, "*"),
                    "del" | "s" | "strike" => self.wrap(id, "~~"),
                    "code" => {
                        let text = self.dom.text_content(id);
                        let text = text.trim();
                        if text.is_empty() {
                            return;
                        }
                        self.out.push('`');
                        self.out.push_str(text);
                        self.out.push('`');
                    }
                    "a" => self.link(id),
                    "img" => self.image(id),
                    "br" => self.out.push_str("  \n"),
                    "span" | "div" if element.has_class("math") => {
                        let tex = self.dom.text_content(id);
                        let tex = tex.trim();
                        if tex.is_empty() {
                            return;
                        }
                        if element.has_class("math-display") {
                            self.out.push_str("$$ ");
                            self.out.push_str(tex);
                            self.out.push_str(" $$");
                        } else {
                            self.out.push('$');
                            self.out.push_str(tex);
                            self.out.push('$');
                        }
                    }
                    "svg" => {}
                    _ => {
                        for child in self.dom.children(id).to_vec() {
                            self.inline(child);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn wrap(&mut self, id: NodeId, marker: &str) {
        let inner = self.inline_children(id);
        let inner = inner.trim();
        if inner.is_empty() {
            return;
        }
        self.out.push_str(marker);
        self.out.push_str(inner);
        self.out.push_str(marker);
    }

    fn link(&mut self, id: NodeId) {
        let href = self.dom.attr(id, "href").unwrap_or("").trim().to_string();
        let text = self.inline_children(id);
        let text = text.trim();
        if text.is_empty() {
            self.out.push_str(&href);
        } else if href.is_empty() || href == text {
            self.out.push_str(text);
        } else {
            self.out.push('[');
            self.out.push_str(text);
            self.out.push_str("](");
            self.out.push_str(&href);
            self.out.push(')');
        }
    }

    fn image(&mut self, id: NodeId) {
        let src = self.dom.attr(id, "src").unwrap_or("").trim().to_string();
        if src.is_empty() {
            return;
        }
        let alt = self.dom.attr(id, "alt").unwrap_or("").trim().to_string();
        self.out.push_str("![");
        self.out.push_str(&alt);
        self.out.push_str("](");
        self.out.push_str(&src);
        self.out.push(')');
    }

    fn list(&mut self, id: NodeId, ordered: bool) {
        self.blank();
        let mut number = 1;
        for child in self.dom.children(id).to_vec() {
            if !self.dom.is_tag(child, "li") {
                continue;
            }
            let marker = if ordered {
                let marker = format!("{number}. ");
                number += 1;
                marker
            } else {
                "- ".to_string()
            };
            self.list_item(child, &marker);
        }
        self.out.push('\n');
    }

    fn list_item(&mut self, id: NodeId, marker: &str) {
        let mut text = String::new();
        let mut nested: Vec<NodeId> = Vec::new();
        for child in self.dom.children(id).to_vec() {
            if self.dom.is_tag(child, "ul") || self.dom.is_tag(child, "ol") {
                nested.push(child);
                continue;
            }
            let part = if self.has_block_children(child) {
                let mut inner = Writer {
                    dom: self.dom,
                    out: String::new(),
                };
                inner.blocks(child);
                inner.out
            } else {
                let mut inner = Writer {
                    dom: self.dom,
                    out: String::new(),
                };
                inner.inline(child);
                inner.out
            };
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(part);
        }
        if text.is_empty() && nested.is_empty() {
            return;
        }
        self.out.push_str(marker);
        self.out.push_str(&text);
        self.out.push('\n');
        for child in nested {
            let ordered = self.dom.is_tag(child, "ol");
            let mut inner = Writer {
                dom: self.dom,
                out: String::new(),
            };
            inner.list(child, ordered);
            for line in inner.out.trim_end().lines() {
                self.out.push_str("  ");
                self.out.push_str(line);
                self.out.push('\n');
            }
        }
    }

    fn code_block(&mut self, id: NodeId) {
        let mut language = String::new();
        let mut code = String::new();
        for child in self.dom.children(id).to_vec() {
            if !self.dom.is_tag(child, "code") {
                continue;
            }
            if let Some(class) = self.dom.attr(child, "class") {
                language = class
                    .split_ascii_whitespace()
                    .find_map(|token| token.strip_prefix("language-"))
                    .unwrap_or("")
                    .to_string();
            }
            code = self.dom.text_content(child);
        }
        if code.is_empty() {
            code = self.dom.text_content(id);
        }
        self.blank();
        self.out.push_str("```");
        self.out.push_str(&language);
        self.out.push('\n');
        self.out.push_str(code.trim_end());
        self.out.push_str("\n```\n\n");
    }

    fn table(&mut self, id: NodeId) {
        let mut header: Option<Vec<String>> = None;
        let mut rows: Vec<Vec<String>> = Vec::new();
        for row in self.dom.find_all(id, |dom, node| dom.is_tag(node, "tr")) {
            let cells: Vec<String> = self
                .dom
                .children(row)
                .iter()
                .filter(|cell| self.dom.is_tag(**cell, "td") || self.dom.is_tag(**cell, "th"))
                .map(|cell| self.inline_children(*cell).trim().replace('|', "\\|"))
                .collect();
            if cells.is_empty() {
                continue;
            }
            let is_header = self
                .dom
                .children(row)
                .iter()
                .any(|cell| self.dom.is_tag(*cell, "th"));
            if is_header && header.is_none() {
                header = Some(cells);
            } else {
                rows.push(cells);
            }
        }
        let Some(header) = header.or_else(|| (!rows.is_empty()).then(|| rows.remove(0))) else {
            return;
        };
        let width = header.len();
        self.blank();
        self.out.push_str("| ");
        self.out.push_str(&header.join(" | "));
        self.out.push_str(" |\n|");
        for _ in 0..width {
            self.out.push_str(" --- |");
        }
        self.out.push('\n');
        for row in rows {
            let mut row = row;
            row.resize(width, String::new());
            self.out.push_str("| ");
            self.out.push_str(&row.join(" | "));
            self.out.push_str(" |\n");
        }
        self.out.push('\n');
    }

    fn has_block_children(&self, id: NodeId) -> bool {
        self.dom.children(id).iter().any(|child| {
            self.dom.element(*child).is_some_and(|element| {
                matches!(
                    element.tag().to_ascii_lowercase().as_str(),
                    "p" | "div"
                        | "section"
                        | "article"
                        | "main"
                        | "header"
                        | "footer"
                        | "figure"
                        | "ul"
                        | "ol"
                        | "table"
                        | "pre"
                        | "blockquote"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "hr"
                )
            })
        })
    }

    fn push_block_text(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.blank();
        self.out.push_str(text);
        self.out.push_str("\n\n");
    }

    fn blank(&mut self) {
        if self.out.is_empty() || self.out.ends_with("\n\n") {
            return;
        }
        if self.out.ends_with('\n') {
            self.out.push('\n');
        } else {
            self.out.push_str("\n\n");
        }
    }
}

/// Inline text: runs of whitespace (newlines included) become one space.
fn collapse_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(character);
        }
    }
    out
}

/// Inline text with runs of whitespace collapsed to one space. Unlike
/// [`collapse_ws`] the single leading and trailing spaces are kept: they
/// separate the text from the inline element before or after it.
fn normalize_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            space = true;
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        out.push(character);
    }
    if space {
        out.push(' ');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze;
    use crate::clean_html;
    use crate::config::ScoringConfig;
    use crate::normalize::normalize_markdown;

    const SAMPLE: &str = "# Groups\n\n**Theorem 1.** Every group of prime order is cyclic.\n\n\
        *Proof.* Take $g \\neq e$; its order divides $p$. $\\blacksquare$\n";

    fn from_markdown(markdown: &str) -> String {
        let doc = analyze(markdown, &ScoringConfig::default());
        render(&doc, Some(&normalize_markdown(markdown)))
    }

    #[test]
    fn markdown_survives_the_round_trip() {
        let out = from_markdown(SAMPLE);
        assert_eq!(out, SAMPLE);
        let again = analyze(&out, &ScoringConfig::default());
        let kinds: Vec<_> = again.blocks.iter().map(|block| block.kind).collect();
        assert_eq!(
            kinds,
            vec![BlockType::Heading, BlockType::Theorem, BlockType::Proof]
        );
    }

    #[test]
    fn a_label_written_in_plain_text_is_bolded_once() {
        let out = from_markdown("# Groups\n\nTheorem 1. Every group of prime order is cyclic.\n");
        assert!(out.contains("**Theorem 1.** Every group"), "{out}");
        assert_eq!(out.matches("Theorem 1").count(), 1, "{out}");
    }

    #[test]
    fn an_unlabelled_block_gets_its_label() {
        // A proof recognised without a keyword has no label in the text.
        let out = from_markdown(
            "# Groups\n\n**Theorem 1.** Every group of prime order is cyclic.\n\n\
             Take $g \\neq e$; its order divides $p$. $\\blacksquare$\n",
        );
        assert!(
            out.contains("**Proof.** Take $g \\neq e$; its order divides $p$."),
            "{out}"
        );
    }

    #[test]
    fn a_latex_environment_keeps_its_form() {
        let source = "# Groups\n\n\\begin{theorem}[Lagrange]\n\
            The order of a subgroup divides the order of the group.\n\\end{theorem}\n";
        let out = from_markdown(source);
        assert_eq!(out, source);
    }

    #[test]
    fn imported_html_becomes_markdown() {
        let html = "<html><body><h1>Groups</h1>\
            <section class=\"mx-block mx-theorem\"><p><strong>Theorem 1.</strong> \
            Every group of prime order is cyclic.</p></section>\
            <p>Let $x$ be <em>odd</em> and see <a href=\"https://example.org/g\">the group</a>.</p>\
            <ul><li>one</li><li>two</li></ul>\
            <pre><code class=\"language-rust\">let x = 1;</code></pre>\
            <table><thead><tr><th>A</th><th>B</th></tr></thead>\
            <tbody><tr><td>1</td><td>2</td></tr></tbody></table></body></html>";
        let doc = clean_html::analyze_html(html);
        let out = render(&doc, None);
        assert!(out.contains("# Groups"), "{out}");
        assert!(out.contains("**Theorem 1.** Every group"), "{out}");
        assert!(out.contains("$x$"), "{out}");
        assert!(out.contains("*odd*"), "{out}");
        assert!(out.contains("[the group](https://example.org/g)"), "{out}");
        assert!(out.contains("- one\n- two"), "{out}");
        assert!(out.contains("```\nlet x = 1;\n```"), "{out}");
        assert!(out.contains("| A | B |\n| --- | --- |\n| 1 | 2 |"), "{out}");
    }

    #[test]
    fn a_theorem_opened_by_a_heading_keeps_the_heading() {
        let out = from_markdown("## Theorem 1\n\nEvery group of prime order is cyclic.\n");
        assert!(out.starts_with("## Theorem 1\n"), "{out}");
        assert!(!out.contains("**Theorem 1.**"), "{out}");
    }

    #[test]
    fn raw_block_markdown_and_source_range_handle_environments_and_lists() {
        let source = "# Intro\n\n\\begin{theorem}[Cauchy]\nLet $G$ be finite.\n\n- Case 1\n- Case 2\n\\end{theorem}\n\nAfter.\n";
        let norm = normalize_markdown(source);
        let doc = analyze(&norm, &ScoringConfig::default());
        assert_eq!(doc.blocks.len(), 3);

        let thm = &doc.blocks[1];
        let range = block_source_range(&doc, thm, &norm).expect("theorem range");
        assert_eq!(
            &norm[range],
            "\\begin{theorem}[Cauchy]\nLet $G$ be finite.\n\n- Case 1\n- Case 2\n\\end{theorem}"
        );
        assert_eq!(
            raw_block_markdown(&doc, thm, Some(&norm)),
            "\\begin{theorem}[Cauchy]\nLet $G$ be finite.\n\n- Case 1\n- Case 2\n\\end{theorem}"
        );

        let after = &doc.blocks[2];
        assert_eq!(raw_block_markdown(&doc, after, Some(&norm)), "After.");
    }
}
