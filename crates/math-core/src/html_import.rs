//! Extracts the readable content of a foreign HTML page.
//!
//! A page written for the web is mostly chrome: sidebars, search boxes, theme
//! buttons, sticky headers, wiki cards, and collapsible proof widgets. The
//! importer keeps headings, prose, tables, code, math, and SVG figures, and
//! turns them into Mathia blocks so the reader, search, and AI tools work on
//! the imported document.

use std::collections::HashSet;

use regex::{Captures, Regex};

use crate::clean_html::{normalize_html_math, slugify, strip_html_tags};
use crate::config::ScoringConfig;
use crate::dom::{Dom, Element, NodeData, NodeId, SerializeOptions};
use crate::features::{KeywordHit, Lexicon};
use crate::model::{BlockType, Direction, Document, EndReason, SemanticBlock, TocEntry};

pub(crate) struct ExtractedHtml {
    pub title: Option<String>,
    pub document: Document,
}

const SERIALIZE: SerializeOptions = SerializeOptions {
    keep_heading_ids: true,
    keep_math_classes: true,
};

/// Tags that never carry readable content (or whose content is a script).
const DROP_TAGS: &[&str] = &[
    "script", "style", "link", "meta", "base", "noscript", "template", "iframe", "object", "embed",
    "canvas", "video", "audio", "source", "track", "form", "input", "textarea", "select", "option",
    "button", "nav", "aside", "footer", "progress", "dialog", "menu",
];

/// Class tokens that identify page chrome. Any element carrying one is removed.
const CHROME_CLASSES: &[&str] = &[
    "site-header",
    "site-sidebar",
    "sidebar",
    "sidebar-backdrop",
    "sidebar-controls",
    "sidebar-filter",
    "sidebar-filter-wrap",
    "header-controls",
    "header-btn",
    "brand-box",
    "brand-wrap",
    "brand-glyph",
    "brand-name",
    "brand-title",
    "brand-sub",
    "font-stepper",
    "font-step-btn",
    "search-btn",
    "search-kbd",
    "search-input",
    "search-input-wrapper",
    "search-input-field",
    "search-results-list",
    "search-empty",
    "search-modal",
    "search-modal-backdrop",
    "search-result-item",
    "search-result-pill",
    "mobile-nav-toggle",
    "progress-bar",
    "back-to-top",
    "toc-nav",
    "toc-group",
    "toc-group-header",
    "toc-group-num",
    "toc-group-meta",
    "toc-group-title",
    "toc-group-pages",
    "toc-arrow",
    "toc-group-body",
    "toc-entry-h2",
    "toc-subentries",
    "toc-link",
    "toc-link-fa",
    "toc-link-en",
    "toc-link-h2",
    "toc-link-h3",
    "nav-link",
    "nav-num",
    "nav-text",
    "exam-banner",
    "ctrl-btn",
    "document-masthead",
    "masthead-meta",
    "masthead-title",
    "masthead-subtitle-en",
    "masthead-details-grid",
    "masthead-detail-item",
    "contribution-banner",
    "contribution-banner-content",
    "contribution-tag",
    "contribution-desc",
    "contribution-btn",
    "subject-wiki-card",
    "wiki-tag",
    "wiki-chips",
    "wiki-chip",
    "wiki-lang",
    "wiki-chip-title",
    "wiki-sidebar-list",
    "wiki-sidebar-item",
    "wiki-sidebar-header",
    "wiki-sidebar-title",
    "wiki-sidebar-links",
    "wiki-pill",
    "external-icon",
    "copy-math-btn",
    "proof-toggle-btn",
    "proof-collapse-footer",
    "proof-collapse-bottom-btn",
    "header-anchor",
    "chapter-meta",
    "chapter-pill",
    "chapter-badge",
    "chapter-count",
    "hero-meta",
    "excluded-pills",
    "pill-ok",
    "pill-ex",
    "mx-theme-toggle",
    "mx-print",
    "mx-toc-toggle",
    "mx-toc",
    "mx-search",
    "mx-actions",
    "mx-math-copy",
    "mx-ai-panel",
    "mx-ai-figure-wrap",
    "mx-edit-toggle",
    "mx-edit-toggle-standalone",
    "mx-edit-sep",
    "mx-edit-bar",
    "mx-edit-banner",
    "mx-edit-add-row",
    "mx-lp-math-preview",
];

/// Wrappers that exist only for layout; their children are the content.
const UNWRAP_CLASSES: &[&str] = &["math-display-container", "math-display-scroll"];

pub(crate) fn extract_and_clean_html(input: &str) -> ExtractedHtml {
    let input = input.trim_start_matches('\u{feff}');
    let input = convert_mathjax_scripts(input);
    let mut dom = Dom::parse(&input);

    let title = extract_title(&dom);
    let explicit_dir = extract_direction(&dom);
    clean_dom(&mut dom);

    let root = content_root(&dom);
    let direction = explicit_dir
        .or_else(|| Direction::detect(&dom.text_content(root)))
        .unwrap_or(Direction::Ltr);

    let block_flags = compute_block_flags(&dom);
    let mut builder = BlockBuilder::new(direction);
    builder.walk(&dom, root, &block_flags);
    builder.flush(&dom);

    let final_title = title.or_else(|| {
        builder
            .blocks
            .iter()
            .find(|b| b.kind == BlockType::Heading && b.heading_level == Some(1))
            .or_else(|| builder.blocks.iter().find(|b| b.kind == BlockType::Heading))
            .map(|b| b.title.clone())
    });

    let document = Document {
        title: final_title.clone(),
        direction,
        nodes: Vec::new(),
        blocks: builder.blocks,
        toc: builder.toc,
    };

    ExtractedHtml {
        title: final_title,
        document,
    }
}

fn extract_title(dom: &Dom) -> Option<String> {
    dom.find_all(dom.root(), |d, id| d.is_tag(id, "title"))
        .first()
        .map(|id| normalize_ws(&dom.text_content(*id)))
        .filter(|title| !title.is_empty())
}

/// MathJax used to put the TeX source in a `<script type="math/tex">` tag.
/// Turn those into Mathia math spans before the script tags are dropped.
fn convert_mathjax_scripts(input: &str) -> String {
    let display = Regex::new(
        r#"(?is)<script\b[^>]*\btype\s*=\s*['"]math/tex;\s*mode=display['"][^>]*>([\s\S]*?)</script\s*>"#,
    )
    .unwrap();
    let inline = Regex::new(
        r#"(?is)<script\b[^>]*\btype\s*=\s*['"]math/tex['"][^>]*>([\s\S]*?)</script\s*>"#,
    )
    .unwrap();
    let s = display.replace_all(input, |caps: &Captures| {
        format!(
            r#"<span class="math math-display">{}</span>"#,
            caps[1].trim()
        )
    });
    inline
        .replace_all(&s, |caps: &Captures| {
            format!(
                r#"<span class="math math-inline">{}</span>"#,
                caps[1].trim()
            )
        })
        .to_string()
}

fn extract_direction(dom: &Dom) -> Option<Direction> {
    for id in dom.find_all(dom.root(), |d, id| {
        d.is_tag(id, "html") || d.is_tag(id, "body")
    }) {
        if let Some(dir) = dom.attr(id, "dir") {
            match dir.trim().to_ascii_lowercase().as_str() {
                "rtl" => return Some(Direction::Rtl),
                "ltr" => return Some(Direction::Ltr),
                _ => {}
            }
        }
    }
    None
}

fn clean_dom(dom: &mut Dom) {
    let root = dom.root();

    // Headings are content, not chrome: lift them out of chrome wrappers (a
    // masthead <header> around the document's <h1>, say) before the chrome is
    // removed, so the heading survives without the surrounding junk.
    hoist_headings_from_chrome(dom, root);

    let doomed = dom.find_all(root, |d, id| {
        d.element(id).is_some_and(|el| {
            let tag = el.tag().to_ascii_lowercase();
            !is_heading_tag(&tag) && (DROP_TAGS.contains(&tag.as_str()) || is_chrome(el))
        })
    });
    for id in doomed {
        dom.detach(id);
    }

    // A proof is a <details> widget in the source. Keep its label and body,
    // but remove the toggle so the proof is not hidden behind a click.
    for id in dom.find_all(root, |d, id| d.is_tag(id, "summary")) {
        dom.rename_html(id, "p");
    }
    for id in dom.find_all(root, |d, id| d.has_class(id, "proof-summary-title")) {
        dom.rename_html(id, "span");
    }
    for id in dom.find_all(root, |d, id| d.is_tag(id, "details")) {
        dom.unwrap(id);
    }

    // Turn illustrated cards into real figures so the caption travels with
    // the SVG (and the AI figure panel can reuse both).
    for id in dom.find_all(root, |d, id| d.has_class(id, "visual-card")) {
        for title in dom.find_all(id, |d, child| d.has_class(child, "visual-title")) {
            dom.rename_html(title, "figcaption");
        }
        dom.rename_html(id, "figure");
    }

    for id in dom.find_all(root, |d, id| {
        d.element(id)
            .is_some_and(|el| UNWRAP_CLASSES.iter().any(|class| el.has_class(class)))
    }) {
        dom.unwrap(id);
    }

    remove_empty(dom, root);
}

fn is_chrome(el: &Element) -> bool {
    CHROME_CLASSES.iter().any(|class| el.has_class(class))
}

/// Moves every heading that sits inside a chrome wrapper out to just before the
/// wrapper's outermost chrome ancestor. Headings under a `DROP_TAG` (nav,
/// aside, footer) are left alone: those are chrome too, not content.
fn hoist_headings_from_chrome(dom: &mut Dom, root: NodeId) {
    let headings = dom.find_all(root, |d, id| {
        d.element(id).is_some_and(|el| is_heading_tag(el.tag()))
    });
    for heading in headings {
        let mut ancestor = dom.node(heading).parent;
        let mut outermost_chrome = None;
        let mut under_drop_tag = false;
        while let Some(id) = ancestor {
            if id == root {
                break;
            }
            if let Some(el) = dom.element(id) {
                let tag = el.tag().to_ascii_lowercase();
                if DROP_TAGS.contains(&tag.as_str()) {
                    under_drop_tag = true;
                    break;
                }
                if is_chrome(el) {
                    outermost_chrome = Some(id);
                }
            }
            ancestor = dom.node(id).parent;
        }
        if !under_drop_tag && let Some(anchor) = outermost_chrome {
            dom.insert_before(anchor, heading);
        }
    }
}

fn remove_empty(dom: &mut Dom, id: NodeId) -> (bool, bool) {
    let mut has_text = false;
    let mut has_visual = false;
    for child in dom.children(id).to_vec() {
        match &dom.node(child).data {
            NodeData::Text(text) => {
                if !text.trim().is_empty() {
                    has_text = true;
                }
            }
            NodeData::Element(el) => {
                let visual = el.is_svg()
                    || matches!(
                        el.tag(),
                        "img" | "table" | "hr" | "br" | "pre" | "code" | "input"
                    );
                let (child_text, child_visual) = remove_empty(dom, child);
                has_text |= child_text;
                has_visual |= visual || child_visual;
            }
            _ => {}
        }
    }
    if id != dom.root()
        && let Some(el) = dom.element(id)
    {
        let keep =
            el.is_svg() || matches!(el.tag(), "br" | "hr" | "img" | "table" | "td" | "th" | "tr");
        if !keep && !has_text && !has_visual {
            dom.detach(id);
        }
    }
    (has_text, has_visual)
}

/// For every node, whether its subtree contains a heading or a semantic block.
fn compute_block_flags(dom: &Dom) -> Vec<bool> {
    let lexicon = Lexicon::new(&ScoringConfig::default());
    let mut flags = vec![false; dom.len()];
    compute_block_flags_at(dom, dom.root(), &mut flags, &lexicon);
    flags
}

fn compute_block_flags_at(dom: &Dom, id: NodeId, flags: &mut Vec<bool>, lexicon: &Lexicon) -> bool {
    let mut any = false;
    for child in dom.children(id) {
        let mut child_any = dom.element(*child).is_some_and(|el| {
            is_heading_tag(el.tag())
                || semantic_kind(el).is_some()
                || keyword_label(dom, *child, lexicon).is_some()
        });
        if compute_block_flags_at(dom, *child, flags, lexicon) {
            child_any = true;
        }
        flags[*child] = child_any;
        any |= child_any;
    }
    any
}

/// A paragraph or list item whose text starts with a theorem-like keyword
/// label ("**Theorem 2.1.** …", "قضیه ۵.۳ (کوشی). …").
fn keyword_label(dom: &Dom, id: NodeId, lexicon: &Lexicon) -> Option<KeywordHit> {
    let el = dom.element(id)?;
    if !el.is_html() {
        return None;
    }
    let tag = el.tag().to_ascii_lowercase();
    if !matches!(tag.as_str(), "li" | "p") {
        return None;
    }
    lexicon.lead(&dom.text_content(id))
}

/// Whether `id` contains a heading or a block of its own. A malformed page can
/// leave a whole section inside one list item; such an item is a container, not
/// a keyword-labelled block.
fn has_block_children(dom: &Dom, id: NodeId, block_flags: &[bool]) -> bool {
    dom.children(id)
        .iter()
        .any(|child| block_flags.get(*child).copied().unwrap_or(false))
}

fn content_root(dom: &Dom) -> NodeId {
    let root = dom.root();
    let body = dom
        .find_all(root, |d, id| d.is_tag(id, "body"))
        .first()
        .copied()
        .unwrap_or(root);
    let main = dom
        .find_all(root, |d, id| d.is_tag(id, "main"))
        .first()
        .copied();

    if let Some(article) = dom
        .find_all(root, |d, id| d.is_tag(id, "article"))
        .into_iter()
        .max_by_key(|id| dom.text_len(*id))
    {
        let article_len = dom.text_len(article);
        let main_len = main.map_or(0, |id| dom.text_len(id));
        if article_len > 200 && article_len * 2 >= main_len {
            // Keep the article only when <main> holds no heading outside it;
            // a title heading (a masthead <h1>, say) is content too.
            let main_has_extra_heading =
                main.is_some_and(|id| count_headings(dom, id) > count_headings(dom, article));
            if !main_has_extra_heading {
                return article;
            }
        }
    }

    main.unwrap_or(body)
}

fn count_headings(dom: &Dom, id: NodeId) -> usize {
    dom.find_all(id, |d, child| {
        d.element(child).is_some_and(|el| is_heading_tag(el.tag()))
    })
    .len()
}

fn is_heading_tag(tag: &str) -> bool {
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
    )
}

fn heading_level(tag: &str) -> u8 {
    tag.as_bytes()
        .get(1)
        .and_then(|b| (*b as char).to_digit(10))
        .unwrap_or(2) as u8
}

fn semantic_kind(el: &Element) -> Option<BlockType> {
    if !el.is_html() {
        return None;
    }
    if let Some(slug) = el.attr("data-block-type")
        && let Some(kind) = kind_from_slug(slug)
    {
        return Some(kind);
    }
    if let Some(kind) = [
        "mx-theorem",
        "mx-lemma",
        "mx-proposition",
        "mx-corollary",
        "mx-definition",
        "mx-proof",
        "mx-example",
        "mx-remark",
        "mx-exercise",
        "mx-solution",
        "mx-claim",
        "mx-notation",
    ]
    .iter()
    .find(|class| el.has_class(class))
    .and_then(|class| kind_from_slug(&class[3..]))
    {
        return Some(kind);
    }
    if el.has_class("callout-def") {
        Some(BlockType::Definition)
    } else if el.has_class("callout-thm") {
        Some(BlockType::Theorem)
    } else if el.has_class("callout-lem") {
        Some(BlockType::Lemma)
    } else if el.has_class("callout-cor") {
        Some(BlockType::Corollary)
    } else if el.has_class("callout-proof") {
        Some(BlockType::Proof)
    } else if el.has_class("callout-example") {
        Some(BlockType::Example)
    } else if el.has_class("callout-note") {
        Some(BlockType::Remark)
    } else {
        None
    }
}

fn kind_from_slug(slug: &str) -> Option<BlockType> {
    match slug.trim().to_ascii_lowercase().as_str() {
        "theorem" => Some(BlockType::Theorem),
        "lemma" => Some(BlockType::Lemma),
        "proposition" => Some(BlockType::Proposition),
        "corollary" => Some(BlockType::Corollary),
        "definition" => Some(BlockType::Definition),
        "proof" => Some(BlockType::Proof),
        "example" => Some(BlockType::Example),
        "remark" => Some(BlockType::Remark),
        "exercise" => Some(BlockType::Exercise),
        "solution" => Some(BlockType::Solution),
        "claim" => Some(BlockType::Claim),
        "notation" => Some(BlockType::Notation),
        _ => None,
    }
}

fn should_recurse(dom: &Dom, id: NodeId, block_flags: &[bool]) -> bool {
    let Some(el) = dom.element(id) else {
        return false;
    };
    if !el.is_html() {
        return false;
    }
    // These elements are atomic content: never split them, even if malformed
    // markup nested a heading inside one.
    if matches!(
        el.tag(),
        "pre"
            | "code"
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "td"
            | "th"
            | "svg"
            | "math"
            | "textarea"
    ) {
        return false;
    }
    block_flags.get(id).copied().unwrap_or(false)
}

struct BlockBuilder {
    direction: Direction,
    lexicon: Lexicon,
    blocks: Vec<SemanticBlock>,
    toc: Vec<TocEntry>,
    pending: Vec<NodeId>,
    ids: HashSet<String>,
    heading_level: u8,
}

impl BlockBuilder {
    fn new(direction: Direction) -> Self {
        BlockBuilder {
            direction,
            lexicon: Lexicon::new(&ScoringConfig::default()),
            blocks: Vec::new(),
            toc: Vec::new(),
            pending: Vec::new(),
            ids: HashSet::new(),
            heading_level: 1,
        }
    }

    fn walk(&mut self, dom: &Dom, id: NodeId, block_flags: &[bool]) {
        for child in dom.children(id).to_vec() {
            match &dom.node(child).data {
                NodeData::Text(text) => {
                    if !text.trim().is_empty() {
                        self.pending.push(child);
                    }
                }
                NodeData::Element(el) => self.visit(dom, child, el, block_flags),
                _ => {}
            }
        }
    }

    fn visit(&mut self, dom: &Dom, id: NodeId, el: &Element, block_flags: &[bool]) {
        if is_heading_tag(el.tag()) {
            self.flush(dom);
            self.add_heading(dom, id, el);
        } else if let Some(kind) = semantic_kind(el) {
            self.flush(dom);
            self.add_semantic(dom, id, kind);
        } else if !has_block_children(dom, id, block_flags)
            && let Some(hit) = keyword_label(dom, id, &self.lexicon)
        {
            self.flush(dom);
            self.add_keyword(dom, id, hit);
        } else if matches!(el.tag().to_ascii_lowercase().as_str(), "ul" | "ol")
            && should_recurse(dom, id, block_flags)
        {
            self.walk_list(dom, id, block_flags);
        } else if should_recurse(dom, id, block_flags) {
            self.walk(dom, id, block_flags);
        } else {
            self.pending.push(id);
        }
    }

    /// A list that holds keyword-labelled blocks: its block items become
    /// blocks, and the remaining items keep their `<ul>` / `<ol>` wrapper.
    fn walk_list(&mut self, dom: &Dom, id: NodeId, block_flags: &[bool]) {
        self.flush(dom);
        let ordered = dom
            .element(id)
            .is_some_and(|el| el.tag().eq_ignore_ascii_case("ol"));
        let mut run: Vec<NodeId> = Vec::new();
        for child in dom.children(id).to_vec() {
            let Some(el) = dom.element(child) else {
                run.push(child);
                continue;
            };
            let is_block = is_heading_tag(el.tag())
                || semantic_kind(el).is_some()
                || keyword_label(dom, child, &self.lexicon).is_some();
            if is_block || should_recurse(dom, child, block_flags) {
                self.flush_list_run(dom, &mut run, ordered);
                self.visit(dom, child, el, block_flags);
            } else {
                run.push(child);
            }
        }
        self.flush_list_run(dom, &mut run, ordered);
    }

    fn flush_list_run(&mut self, dom: &Dom, run: &mut Vec<NodeId>, ordered: bool) {
        if run.is_empty() {
            return;
        }
        // Only wrap a run in `<ul>` / `<ol>` when every item is a list item;
        // block content that merely sits inside the list (for example a
        // `div.math-block` between items) is emitted on its own.
        let all_items = run.iter().all(|id| match dom.element(*id) {
            Some(el) => el.tag().eq_ignore_ascii_case("li"),
            None => dom.text_content(*id).trim().is_empty(),
        });
        let mut html = String::new();
        if all_items {
            let tag = if ordered { "ol" } else { "ul" };
            html.push_str(&format!("<{tag}>"));
            for id in run.drain(..) {
                html.push_str(&dom.serialize(id, &SERIALIZE));
            }
            html.push_str(&format!("</{tag}>"));
        } else {
            for id in run.drain(..) {
                html.push_str(&dom.serialize(id, &SERIALIZE));
            }
        }
        self.push_text(html);
    }

    fn add_heading(&mut self, dom: &Dom, id: NodeId, el: &Element) {
        let level = heading_level(el.tag());
        let title = normalize_ws(&dom.text_content(id));
        let raw_id = dom
            .attr(id, "id")
            .map(str::to_string)
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| slugify(&title, self.blocks.len() + 1));
        let block_id = self.unique_id(raw_id);
        let html = normalize_html_math(&dom.serialize_children(id, &SERIALIZE))
            .trim()
            .to_string();

        self.blocks.push(SemanticBlock {
            id: block_id.clone(),
            kind: BlockType::Heading,
            span: self.blocks.len()..self.blocks.len() + 1,
            heading_level: Some(level),
            number: None,
            name: None,
            title: title.clone(),
            direction: self.direction,
            score: 0.0,
            features: None,
            proves: None,
            end: EndReason::Heading,
            html,
        });
        self.toc.push(TocEntry {
            id: block_id,
            title,
            depth: level,
            kind: BlockType::Heading,
        });
        self.heading_level = level;
    }

    fn add_semantic(&mut self, dom: &Dom, id: NodeId, kind: BlockType) {
        let title = callout_title(dom, id)
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| kind.label(self.direction).to_string());
        let body = callout_body(dom, id).unwrap_or_else(|| dom.serialize_children(id, &SERIALIZE));
        let html = normalize_html_math(&body).trim().to_string();
        let block_id = self.unique_id(format!("block-{}", self.blocks.len() + 1));

        self.blocks.push(SemanticBlock {
            id: block_id.clone(),
            kind,
            span: self.blocks.len()..self.blocks.len() + 1,
            heading_level: None,
            number: None,
            name: None,
            title: title.clone(),
            direction: self.direction,
            score: 1.0,
            features: None,
            proves: None,
            end: EndReason::Structural,
            html,
        });
        self.toc.push(TocEntry {
            id: block_id,
            title,
            depth: self.heading_level.saturating_add(1).max(2),
            kind,
        });
    }

    fn add_keyword(&mut self, dom: &Dom, id: NodeId, hit: KeywordHit) {
        let kind = hit.kind;
        let direction = Direction::detect(&dom.text_content(id)).unwrap_or(self.direction);
        let mut title = kind.label(direction).to_string();
        if let Some(number) = &hit.number {
            title.push(' ');
            title.push_str(number);
        }
        if let Some(name) = &hit.name {
            title.push_str(&format!(" ({name})"));
        }
        let body = dom.serialize_children(id, &SERIALIZE);
        let html = normalize_html_math(&body).trim().to_string();
        let block_id = self.unique_id(format!("block-{}-{}", kind.slug(), self.blocks.len() + 1));

        self.blocks.push(SemanticBlock {
            id: block_id.clone(),
            kind,
            span: self.blocks.len()..self.blocks.len() + 1,
            heading_level: None,
            number: hit.number,
            name: hit.name,
            title: title.clone(),
            direction,
            score: 1.0,
            features: None,
            proves: None,
            end: EndReason::Structural,
            html,
        });
        self.toc.push(TocEntry {
            id: block_id,
            title,
            depth: self.heading_level.saturating_add(1).max(2),
            kind,
        });
    }

    fn flush(&mut self, dom: &Dom) {
        if self.pending.is_empty() {
            return;
        }
        let mut html = String::new();
        for id in self.pending.drain(..) {
            html.push_str(&dom.serialize(id, &SERIALIZE));
        }
        self.push_text(html);
    }

    fn push_text(&mut self, html: String) {
        let html = normalize_html_math(&html).trim().to_string();
        if !has_content(&html) {
            return;
        }
        let block_id = self.unique_id(format!("block-{}", self.blocks.len() + 1));
        self.blocks.push(SemanticBlock {
            id: block_id,
            kind: BlockType::Text,
            span: self.blocks.len()..self.blocks.len() + 1,
            heading_level: None,
            number: None,
            name: None,
            title: "Content".to_string(),
            direction: self.direction,
            score: 0.0,
            features: None,
            proves: None,
            end: EndReason::EndOfDocument,
            html,
        });
    }

    fn unique_id(&mut self, id: String) -> String {
        let base = if id.trim().is_empty() {
            format!("block-{}", self.blocks.len() + 1)
        } else {
            id
        };
        let mut candidate = base.clone();
        let mut suffix = 2;
        while !self.ids.insert(candidate.clone()) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        candidate
    }
}

fn callout_title(dom: &Dom, id: NodeId) -> Option<String> {
    if let Some(label) = dom.attr(id, "aria-label") {
        let title = normalize_ws(label);
        if !title.is_empty() {
            return Some(title);
        }
    }
    for class in ["callout-label", "proof-title-extra", "callout-title"] {
        if let Some(label) = dom
            .find_all(id, |d, child| d.has_class(child, class))
            .first()
        {
            let title = normalize_ws(&dom.text_content(*label));
            if !title.is_empty() {
                return Some(title);
            }
        }
    }
    None
}

fn callout_body(dom: &Dom, id: NodeId) -> Option<String> {
    for class in ["callout-body", "proof-body", "mx-block-body"] {
        if let Some(body) = dom
            .find_all(id, |d, child| d.has_class(child, class))
            .first()
        {
            return Some(dom.serialize_children(*body, &SERIALIZE));
        }
    }
    None
}

fn has_content(html: &str) -> bool {
    if !strip_html_tags(html).trim().is_empty() {
        return true;
    }
    ["<svg", "<img", "<table", "<hr", "<pre", "<code", "<br"]
        .iter()
        .any(|tag| html.contains(tag))
}

fn normalize_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(html: &str) -> ExtractedHtml {
        extract_and_clean_html(html)
    }

    #[test]
    fn drops_scripts_styles_and_search_chrome() {
        let out = import(
            r#"<!doctype html><html><head><title>Notes</title><style>p{color:red}</style></head>
            <body><header class="site-header"><button onclick="openSearch()">Search</button></header>
            <main><h1>Chapter</h1><p>Real content.</p>
            <div class="search-modal-backdrop"><input class="search-input-field"></div>
            <script>alert(1)</script></main></body></html>"#,
        );
        let html = crate::html::render(&out.document, &crate::html::HtmlOptions::default())
            .expect("renders");
        assert!(html.contains("Real content."));
        assert!(!html.contains("openSearch"));
        assert!(!html.contains("search-modal"));
        assert!(!html.contains("alert(1)"));
        assert!(!html.contains("color:red"));
    }

    #[test]
    fn opens_proof_details_and_keeps_svg_figures() {
        let out = import(
            r#"<html><body><main>
            <h2>Proofs</h2>
            <details class="callout callout-proof"><summary class="callout-header">
              <span class="callout-label">برهان</span><button class="proof-toggle-btn">x</button>
            </summary><div class="callout-body"><p>Because it is true.</p></div></details>
            <div class="visual-card"><div class="visual-title">Figure</div>
              <svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="4"/></svg></div>
            </main></body></html>"#,
        );
        let html = crate::html::render(&out.document, &crate::html::HtmlOptions::default())
            .expect("renders");
        assert!(html.contains("Because it is true."));
        assert!(!html.contains("<details"));
        assert!(html.contains("<figure>"));
        assert!(html.contains("<figcaption>Figure</figcaption>"));
        assert!(html.contains("<svg"));
        assert!(html.contains("viewBox=\"0 0 10 10\""));
    }

    #[test]
    fn keeps_masthead_heading_but_drops_its_chrome() {
        let out = import(
            r#"<html><body><main>
            <header class="document-masthead"><h1>Site title</h1><p>Meta</p></header>
            <article><h2>Lesson</h2><p>Lesson body.</p></article>
            </main></body></html>"#,
        );
        let html = crate::html::render(&out.document, &crate::html::HtmlOptions::default())
            .expect("renders");
        assert!(html.contains("Lesson body."));
        // The document heading is content and survives; the masthead junk goes.
        assert!(html.contains("Site title"));
        assert!(!html.contains("Meta"));
    }

    #[test]
    fn keyword_labelled_list_items_become_blocks() {
        let out = import(
            r#"<html><body><main>
            <ul>
              <li><strong>قضیه ۵.۳.</strong> اگر $W$ زیرفضا باشد آنگاه حکم برقرار است.</li>
              <li><strong>Theorem 2.1.</strong> Every finite set is bounded.</li>
              <li>یک مورد عادی.</li>
            </ul>
            <p>Theorem 2 gives a bound.</p>
            </main></body></html>"#,
        );
        let kinds: Vec<_> = out.document.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [
                BlockType::Theorem,
                BlockType::Theorem,
                BlockType::Text,
                BlockType::Text
            ]
        );
        assert_eq!(out.document.blocks[0].title, "قضیه ۵.۳");
        assert_eq!(out.document.blocks[1].title, "Theorem 2.1");

        let html = crate::html::render(&out.document, &crate::html::HtmlOptions::default())
            .expect("renders");
        // The plain item keeps its list wrapper; prose that only mentions a
        // theorem stays text.
        assert!(html.contains("<ul>"));
        assert!(html.contains("یک مورد عادی."));
        assert!(html.contains("Theorem 2 gives a bound."));
    }
}
