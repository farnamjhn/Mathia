//! A small, read-mostly DOM on top of `html5ever`.
//!
//! The HTML importer needs to remove nested chrome (sidebars, search boxes,
//! collapsible wrappers) without breaking the content around it. Regexes cannot
//! match nested elements reliably, so the importer parses the document once and
//! works on this tree instead.

use std::cell::{Ref, RefCell};
use std::fmt::Write as _;

use html5ever::interface::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{Attribute, LocalName, ParseOpts, QualName, parse_document};

pub const HTML_NS: &str = "http://www.w3.org/1999/xhtml";
pub const SVG_NS: &str = "http://www.w3.org/2000/svg";

pub type NodeId = usize;

#[derive(Debug, Clone)]
pub struct Attr {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub qual: QualName,
    pub attrs: Vec<Attr>,
    pub template_contents: Option<NodeId>,
    pub mathml_annotation_xml_integration_point: bool,
}

impl Element {
    pub fn tag(&self) -> &str {
        self.qual.local.as_ref()
    }

    pub fn namespace(&self) -> &str {
        self.qual.ns.as_ref()
    }

    pub fn is_html(&self) -> bool {
        self.namespace() == HTML_NS
    }

    pub fn is_svg(&self) -> bool {
        self.namespace() == SVG_NS
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .map(|a| a.value.as_str())
    }

    pub fn has_class(&self, token: &str) -> bool {
        self.attr("class").is_some_and(|classes| {
            classes
                .split_ascii_whitespace()
                .any(|c| c.eq_ignore_ascii_case(token))
        })
    }
}

#[derive(Debug, Clone)]
pub enum NodeData {
    Document,
    Doctype,
    Text(String),
    Comment,
    Element(Element),
    ProcessingInstruction,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub data: NodeData,
}

#[derive(Debug, Clone)]
pub struct Dom {
    nodes: Vec<Node>,
}

impl Dom {
    pub fn parse(input: &str) -> Dom {
        let sink = Sink::new();
        parse_document(sink, ParseOpts::default())
            .from_utf8()
            .one(input.as_bytes())
    }

    pub fn root(&self) -> NodeId {
        0
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    pub fn element(&self, id: NodeId) -> Option<&Element> {
        match &self.nodes[id].data {
            NodeData::Element(el) => Some(el),
            _ => None,
        }
    }

    pub fn is_tag(&self, id: NodeId, tag: &str) -> bool {
        self.element(id)
            .is_some_and(|el| el.is_html() && el.tag().eq_ignore_ascii_case(tag))
    }

    pub fn has_class(&self, id: NodeId, token: &str) -> bool {
        self.element(id).is_some_and(|el| el.has_class(token))
    }

    pub fn attr(&self, id: NodeId, name: &str) -> Option<&str> {
        self.element(id).and_then(|el| el.attr(name))
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.nodes[id].children
    }

    pub fn detach(&mut self, id: NodeId) {
        if let Some(parent) = self.nodes[id].parent.take() {
            self.nodes[parent].children.retain(|child| *child != id);
        }
    }

    /// Moves `id` to sit immediately before `reference`, under the same parent.
    pub fn insert_before(&mut self, reference: NodeId, id: NodeId) {
        let Some(parent) = self.nodes[reference].parent else {
            return;
        };
        if let Some(old) = self.nodes[id].parent.take() {
            self.nodes[old].children.retain(|child| *child != id);
        }
        let Some(pos) = self.nodes[parent]
            .children
            .iter()
            .position(|child| *child == reference)
        else {
            return;
        };
        self.nodes[parent].children.insert(pos, id);
        self.nodes[id].parent = Some(parent);
    }

    /// Replaces `id` with its children, keeping their order.
    pub fn unwrap(&mut self, id: NodeId) {
        let Some(parent) = self.nodes[id].parent else {
            return;
        };
        let Some(pos) = self.nodes[parent]
            .children
            .iter()
            .position(|child| *child == id)
        else {
            return;
        };
        let children = std::mem::take(&mut self.nodes[id].children);
        for child in &children {
            self.nodes[*child].parent = Some(parent);
        }
        self.nodes[parent].children.splice(pos..=pos, children);
        self.nodes[id].parent = None;
    }

    pub fn text_content(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.collect_text(id, &mut out);
        out
    }

    fn collect_text(&self, id: NodeId, out: &mut String) {
        match &self.nodes[id].data {
            NodeData::Text(text) => out.push_str(text),
            NodeData::Element(_) | NodeData::Document => {
                for child in &self.nodes[id].children {
                    self.collect_text(*child, out);
                }
            }
            _ => {}
        }
    }

    pub fn text_len(&self, id: NodeId) -> usize {
        self.text_content(id)
            .chars()
            .filter(|c| !c.is_whitespace())
            .count()
    }

    pub fn find_all(&self, root: NodeId, predicate: impl Fn(&Dom, NodeId) -> bool) -> Vec<NodeId> {
        let mut found = Vec::new();
        self.find_all_into(root, &predicate, &mut found);
        found
    }

    fn find_all_into(
        &self,
        id: NodeId,
        predicate: &impl Fn(&Dom, NodeId) -> bool,
        found: &mut Vec<NodeId>,
    ) {
        if id != self.root() && predicate(self, id) {
            found.push(id);
        }
        for child in &self.nodes[id].children {
            self.find_all_into(*child, predicate, found);
        }
    }

    pub fn serialize(&self, id: NodeId, options: &SerializeOptions) -> String {
        let mut out = String::new();
        self.write_node(id, options, false, &mut out);
        out
    }

    pub fn serialize_children(&self, id: NodeId, options: &SerializeOptions) -> String {
        let inside_svg = self.element(id).is_some_and(Element::is_svg);
        let mut out = String::new();
        for child in &self.nodes[id].children {
            self.write_node(*child, options, inside_svg, &mut out);
        }
        out
    }

    /// Changes an element's tag while keeping its attributes and children.
    pub fn rename_html(&mut self, id: NodeId, tag: &str) {
        if let NodeData::Element(el) = &mut self.nodes[id].data {
            el.qual = QualName::new(None, html5ever::ns!(html), LocalName::from(tag));
        }
    }

    fn write_node(
        &self,
        id: NodeId,
        options: &SerializeOptions,
        inside_svg: bool,
        out: &mut String,
    ) {
        match &self.nodes[id].data {
            NodeData::Text(text) => escape_text(text, out),
            NodeData::Comment | NodeData::ProcessingInstruction | NodeData::Doctype => {}
            NodeData::Document => {
                for child in &self.nodes[id].children {
                    self.write_node(*child, options, inside_svg, out);
                }
            }
            NodeData::Element(el) => {
                let svg = inside_svg || el.is_svg();
                let tag = el.tag();
                out.push('<');
                out.push_str(tag);
                for attr in &el.attrs {
                    if keep_attr(el, attr, svg, options) {
                        out.push(' ');
                        out.push_str(&attr.name);
                        out.push_str("=\"");
                        escape_attr(&attr.value, out);
                        out.push('"');
                    }
                }
                if svg && tag.eq_ignore_ascii_case("svg") && el.attr("xmlns").is_none() {
                    out.push_str(" xmlns=\"");
                    out.push_str(SVG_NS);
                    out.push('"');
                }
                out.push('>');
                if el.is_html() && is_void(tag) {
                    return;
                }
                for child in &self.nodes[id].children {
                    self.write_node(*child, options, svg, out);
                }
                let _ = write!(out, "</{tag}>");
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SerializeOptions {
    /// Keep `id` on headings so the table of contents can link to them.
    pub keep_heading_ids: bool,
    /// Keep class names that math normalization understands.
    pub keep_math_classes: bool,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        SerializeOptions {
            keep_heading_ids: true,
            keep_math_classes: true,
        }
    }
}

fn is_void(tag: &str) -> bool {
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn is_heading(tag: &str) -> bool {
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
    )
}

fn keep_attr(el: &Element, attr: &Attr, inside_svg: bool, options: &SerializeOptions) -> bool {
    let name = attr.name.as_str();
    if name.len() > 2 && name[..2].eq_ignore_ascii_case("on") {
        return false;
    }
    if inside_svg {
        // SVG attributes are the drawing: keep them all except classes.
        return !name.eq_ignore_ascii_case("class");
    }
    match name.to_ascii_lowercase().as_str() {
        "class" => options.keep_math_classes && is_math_class(&attr.value),
        "style" => false,
        "id" => options.keep_heading_ids && is_heading(el.tag()),
        "dir" => true,
        "href" | "src" => !is_script_url(&attr.value),
        "alt" | "colspan" | "rowspan" | "scope" | "start" | "type" | "value" => true,
        "width" | "height" => el.is_html() && matches!(el.tag(), "img" | "table" | "td" | "th"),
        "data-mx-tex" => true,
        _ => false,
    }
}

fn is_script_url(value: &str) -> bool {
    value
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("javascript:")
}

fn is_math_class(value: &str) -> bool {
    value.split_ascii_whitespace().any(|token| {
        matches!(
            token.to_ascii_lowercase().as_str(),
            "math"
                | "math-inline"
                | "math-display"
                | "inline"
                | "display"
                | "arithmatex"
                | "latex"
                | "tex"
                | "katex"
                | "katex-display"
        )
    })
}

fn escape_text(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            _ => out.push(c),
        }
    }
}

fn escape_attr(value: &str, out: &mut String) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            _ => out.push(c),
        }
    }
}

struct Sink {
    arena: RefCell<Vec<Node>>,
}

impl Sink {
    fn new() -> Sink {
        Sink {
            arena: RefCell::new(vec![Node {
                parent: None,
                children: Vec::new(),
                data: NodeData::Document,
            }]),
        }
    }

    fn push(&self, data: NodeData) -> NodeId {
        let mut arena = self.arena.borrow_mut();
        let id = arena.len();
        arena.push(Node {
            parent: None,
            children: Vec::new(),
            data,
        });
        id
    }

    fn append_common(&self, parent: NodeId, child: NodeOrText<NodeId>, before: Option<NodeId>) {
        let mut arena = self.arena.borrow_mut();
        let new_id = match child {
            NodeOrText::AppendText(text) => {
                let previous = match before {
                    Some(sibling) => arena[parent]
                        .children
                        .iter()
                        .position(|c| *c == sibling)
                        .and_then(|pos| pos.checked_sub(1))
                        .map(|pos| arena[parent].children[pos]),
                    None => arena[parent].children.last().copied(),
                };
                if let Some(previous) = previous
                    && let NodeData::Text(existing) = &mut arena[previous].data
                {
                    existing.push_str(&text);
                    return;
                }
                let id = arena.len();
                arena.push(Node {
                    parent: None,
                    children: Vec::new(),
                    data: NodeData::Text(text.to_string()),
                });
                id
            }
            NodeOrText::AppendNode(id) => {
                if let Some(old_parent) = arena[id].parent.take() {
                    arena[old_parent].children.retain(|c| *c != id);
                }
                id
            }
        };

        arena[new_id].parent = Some(parent);
        match before {
            Some(sibling) => {
                let pos = arena[parent]
                    .children
                    .iter()
                    .position(|c| *c == sibling)
                    .unwrap_or(arena[parent].children.len());
                arena[parent].children.insert(pos, new_id);
            }
            None => arena[parent].children.push(new_id),
        }
    }
}

impl TreeSink for Sink {
    type Handle = NodeId;
    type Output = Dom;
    type ElemName<'a>
        = Ref<'a, QualName>
    where
        Self: 'a;

    fn finish(self) -> Dom {
        Dom {
            nodes: self.arena.into_inner(),
        }
    }

    fn parse_error(&self, _: std::borrow::Cow<'static, str>) {}

    fn get_document(&self) -> NodeId {
        0
    }

    fn elem_name<'a>(&'a self, target: &'a NodeId) -> Self::ElemName<'a> {
        Ref::map(self.arena.borrow(), |arena| match &arena[*target].data {
            NodeData::Element(el) => &el.qual,
            _ => panic!("elem_name called on a non-element"),
        })
    }

    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, flags: ElementFlags) -> NodeId {
        let template_contents = if flags.template {
            Some(self.push(NodeData::Document))
        } else {
            None
        };
        self.push(NodeData::Element(Element {
            qual: name,
            attrs: attrs
                .into_iter()
                .map(|attr| Attr {
                    name: qualified_name(&attr.name),
                    value: attr.value.to_string(),
                })
                .collect(),
            template_contents,
            mathml_annotation_xml_integration_point: flags.mathml_annotation_xml_integration_point,
        }))
    }

    fn create_comment(&self, _text: StrTendril) -> NodeId {
        self.push(NodeData::Comment)
    }

    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> NodeId {
        self.push(NodeData::ProcessingInstruction)
    }

    fn append(&self, parent: &NodeId, child: NodeOrText<NodeId>) {
        self.append_common(*parent, child, None);
    }

    fn append_before_sibling(&self, sibling: &NodeId, child: NodeOrText<NodeId>) {
        let parent = self.arena.borrow()[*sibling]
            .parent
            .expect("sibling has a parent");
        self.append_common(parent, child, Some(*sibling));
    }

    fn append_based_on_parent_node(
        &self,
        element: &NodeId,
        prev_element: &NodeId,
        child: NodeOrText<NodeId>,
    ) {
        if self.arena.borrow()[*element].parent.is_some() {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        let id = self.push(NodeData::Doctype);
        self.append_common(0, NodeOrText::AppendNode(id), None);
    }

    fn get_template_contents(&self, target: &NodeId) -> NodeId {
        match &self.arena.borrow()[*target].data {
            NodeData::Element(el) => el.template_contents.expect("not a template element"),
            _ => panic!("not an element"),
        }
    }

    fn same_node(&self, x: &NodeId, y: &NodeId) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn add_attrs_if_missing(&self, target: &NodeId, attrs: Vec<Attribute>) {
        let mut arena = self.arena.borrow_mut();
        let NodeData::Element(el) = &mut arena[*target].data else {
            panic!("not an element");
        };
        for attr in attrs {
            let name = qualified_name(&attr.name);
            if !el.attrs.iter().any(|a| a.name == name) {
                el.attrs.push(Attr {
                    name,
                    value: attr.value.to_string(),
                });
            }
        }
    }

    fn remove_from_parent(&self, target: &NodeId) {
        let mut arena = self.arena.borrow_mut();
        if let Some(parent) = arena[*target].parent.take() {
            arena[parent].children.retain(|c| *c != *target);
        }
    }

    fn reparent_children(&self, node: &NodeId, new_parent: &NodeId) {
        let mut arena = self.arena.borrow_mut();
        let children = std::mem::take(&mut arena[*node].children);
        for child in &children {
            arena[*child].parent = Some(*new_parent);
        }
        arena[*new_parent].children.extend(children);
    }

    fn is_mathml_annotation_xml_integration_point(&self, target: &NodeId) -> bool {
        match &self.arena.borrow()[*target].data {
            NodeData::Element(el) => el.mathml_annotation_xml_integration_point,
            _ => false,
        }
    }
}

fn qualified_name(name: &QualName) -> String {
    match &name.prefix {
        Some(prefix) if !prefix.as_ref().is_empty() => {
            format!("{}:{}", prefix.as_ref(), name.local.as_ref())
        }
        None => name.local.as_ref().to_string(),
        Some(_) => name.local.as_ref().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_serializes_nested_html() {
        let dom = Dom::parse("<div><p>Hello <b>world</b></p><br></div>");
        let html = dom.serialize(dom.root(), &SerializeOptions::default());
        assert!(html.contains("<p>Hello <b>world</b></p>"));
        assert!(html.contains("<br>"));
    }

    #[test]
    fn keeps_svg_attributes_and_drops_event_handlers() {
        let dom = Dom::parse(
            r#"<svg viewBox="0 0 10 10" onclick="alert(1)"><circle cx="5" cy="5" r="4" fill="red"/></svg>"#,
        );
        let html = dom.serialize(dom.root(), &SerializeOptions::default());
        assert!(html.contains("viewBox=\"0 0 10 10\""));
        assert!(html.contains("cx=\"5\""));
        assert!(!html.contains("onclick"));
    }

    #[test]
    fn serializes_svg_xmlns_without_an_empty_prefix() {
        let dom = Dom::parse(r#"<svg viewBox="0 0 10 10"><circle r="4"/></svg>"#);
        let html = dom.serialize(dom.root(), &SerializeOptions::default());
        assert!(html.contains("xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(!html.contains(":xmlns"));
    }
}
