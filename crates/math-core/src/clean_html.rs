//! HTML import and cleaning: strips bloat (old scripts, stylesheets, CSP,
//! and stale chrome) from existing HTML documents and replaces them with
//! Mathia's modern scripts, styles, KaTeX, and runtime.

use std::path::Path;
use regex::{Captures, Regex};

use crate::classifier::NodeScore;
use crate::error::Result;
use crate::html::{self, HtmlOptions};
use crate::model::{BlockType, Direction, Document, EndReason, SemanticBlock, TocEntry};
use crate::Analysis;

/// Checks if a file path has an HTML extension.
pub fn is_html_file(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
}

/// Sniffs whether a string represents an HTML document or snippet.
pub fn is_html(text: &str) -> bool {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    let lower = trimmed.to_ascii_lowercase();

    if lower.starts_with("<!doctype html")
        || lower.starts_with("<html")
        || lower.starts_with("<?xml")
        || lower.starts_with("<head")
        || lower.starts_with("<body")
        || lower.starts_with("<main id=\"mx-main\"")
        || lower.starts_with("<main id='mx-main'")
    {
        return true;
    }

    (lower.contains("<html") && lower.contains("</html>"))
        || (lower.contains("<body") && lower.contains("</body>"))
        || lower.contains("<main id=\"mx-main\"")
        || lower.contains("<main id='mx-main'")
        || lower.contains("<section class=\"mx-block")
        || lower.contains("<section class='mx-block")
}

/// Decodes basic HTML entities commonly found in headings and titles.
fn decode_basic_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Strips all HTML tags from a string to yield plain text.
fn strip_html_tags(html: &str) -> String {
    let re_tag = Regex::new(r"<[^>]*>").unwrap();
    let text = re_tag.replace_all(html, " ");
    decode_basic_entities(&text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Generates a valid HTML id slug from a title.
fn slugify(text: &str, counter: usize) -> String {
    let mut slug = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if (c == ' ' || c == '-' || c == '_') && !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        format!("sec-{counter}")
    } else {
        slug.to_string()
    }
}

/// Strips enclosing LaTeX math delimiters (`\[ \]`, `\( \)`, `$$`, `$`).
fn clean_delimiters(s: &str) -> &str {
    let t = s.trim();
    if (t.starts_with(r"\[") && t.ends_with(r"\]"))
        || (t.starts_with(r"\(") && t.ends_with(r"\)"))
        || (t.starts_with("$$") && t.ends_with("$$") && t.len() >= 4)
    {
        t[2..t.len() - 2].trim()
    } else if t.starts_with('$') && t.ends_with('$') && t.len() >= 2 {
        t[1..t.len() - 1].trim()
    } else {
        t
    }
}

/// Checks if an environment name is a recognized LaTeX math environment.
fn is_math_env(env: &str) -> bool {
    let base = env.trim_end_matches('*');
    matches!(
        base,
        "equation"
            | "align"
            | "alignat"
            | "gather"
            | "multline"
            | "flalign"
            | "matrix"
            | "pmatrix"
            | "bmatrix"
            | "Bmatrix"
            | "vmatrix"
            | "Vmatrix"
            | "cases"
            | "dcases"
            | "rcases"
            | "drcases"
            | "split"
            | "array"
    )
}

/// Converts raw LaTeX delimiters (`$$`, `\[`, `\(`, `\begin{...}`, `$`) inside a plain text node
/// into Mathia's canonical `<span class="math math-inline">` and `<span class="math math-display">`.
fn convert_text_math(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let b = text.as_bytes();
    let n = b.len();

    while i < n {
        if b[i] == b'\\' {
            // \[ ... \]
            if b.get(i + 1) == Some(&b'[')
                && let Some(close) = text[i + 2..].find(r"\]")
            {
                let end = i + 2 + close;
                let tex = &text[i + 2..end];
                out.push_str(r#"<span class="math math-display">"#);
                out.push_str(tex.trim());
                out.push_str("</span>");
                i = end + 2;
                continue;
            }
            // \( ... \)
            if b.get(i + 1) == Some(&b'(')
                && let Some(close) = text[i + 2..].find(r"\)")
            {
                let end = i + 2 + close;
                let tex = &text[i + 2..end];
                out.push_str(r#"<span class="math math-inline">"#);
                out.push_str(tex.trim());
                out.push_str("</span>");
                i = end + 2;
                continue;
            }
            // \begin{env} ... \end{env}
            if text[i..].starts_with(r"\begin{")
                && let Some(brace_end) = text[i + 7..].find('}')
            {
                let env = &text[i + 7..i + 7 + brace_end];
                if is_math_env(env) {
                    let end_tag = format!(r"\end{{{env}}}");
                    if let Some(close) = text[i + 7 + brace_end + 1..].find(&end_tag) {
                        let end = i + 7 + brace_end + 1 + close + end_tag.len();
                        let tex = &text[i..end];
                        out.push_str(r#"<span class="math math-display">"#);
                        out.push_str(tex.trim());
                        out.push_str("</span>");
                        i = end;
                        continue;
                    }
                }
            }
            // Escaped dollar \$ -> literal $
            if b.get(i + 1) == Some(&b'$') {
                out.push('$');
                i += 2;
                continue;
            }
            out.push('\\');
            if let Some(&next) = b.get(i + 1) {
                out.push(next as char);
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        // Display math $$ ... $$
        if b[i] == b'$' && b.get(i + 1) == Some(&b'$')
            && let Some(close) = text[i + 2..].find("$$")
        {
            let end = i + 2 + close;
            let tex = &text[i + 2..end];
            out.push_str(r#"<span class="math math-display">"#);
            out.push_str(tex.trim());
            out.push_str("</span>");
            i = end + 2;
            continue;
        }

        // Inline math $ ... $
        if b[i] == b'$'
            && let Some(&next) = b.get(i + 1)
            && next != b' ' && next != b'\t' && next != b'\n' && next != b'\r' && next != b'$'
        {
            let mut close = None;
            let mut k = i + 1;
            while k < n {
                if b[k] == b'\n' {
                    break;
                }
                if b[k] == b'\\' {
                    k += 2;
                    continue;
                }
                if b[k] == b'$' {
                    if b[k - 1] != b' ' && b[k - 1] != b'\t' {
                        close = Some(k);
                    }
                    break;
                }
                k += 1;
            }
            if let Some(k) = close {
                let tex = &text[i + 1..k];
                out.push_str(r#"<span class="math math-inline">"#);
                out.push_str(tex.trim());
                out.push_str("</span>");
                i = k + 1;
                continue;
            }
        }

        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }

    out
}

/// Scans HTML text nodes outside tags and code blocks, converting delimiters to math spans.
fn process_html_text_nodes(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    let len = html.len();

    while i < len {
        let rest = &html[i..];

        // 1. Text node until next '<'
        if !rest.starts_with('<') {
            let text_end = rest.find('<').unwrap_or(rest.len());
            let text_chunk = &rest[..text_end];
            out.push_str(&convert_text_math(text_chunk));
            i += text_end;
            continue;
        }

        // 2. HTML Comment: <!-- ... -->
        if rest.starts_with("<!--") {
            if let Some(end) = rest.find("-->") {
                out.push_str(&rest[..end + 3]);
                i += end + 3;
            } else {
                out.push_str(rest);
                break;
            }
            continue;
        }

        // 3. Protected tags: <pre>, <code>, <textarea>, <kbd>, <samp>
        let protected_tags = ["pre", "code", "textarea", "kbd", "samp"];
        let mut matched_protected = false;
        for tag in &protected_tags {
            let open_len = 1 + tag.len();
            if rest.len() >= open_len
                && rest.is_char_boundary(open_len)
                && rest[1..open_len].eq_ignore_ascii_case(tag)
            {
                let next_byte = rest.as_bytes().get(open_len);
                if matches!(next_byte, Some(b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'/')) {
                    let close_pat = format!("</{tag}");
                    let end_pos = rest.to_ascii_lowercase().find(&close_pat).and_then(|idx| {
                        rest[idx..].find('>').map(|gt| idx + gt + 1)
                    });
                    if let Some(end) = end_pos {
                        out.push_str(&rest[..end]);
                        i += end;
                        matched_protected = true;
                        break;
                    }
                }
            }
        }
        if matched_protected {
            continue;
        }

        // 4. Existing math containers: <span class="...math..." or <div class="...math..."
        let starts_span = rest.len() >= 5 && rest.is_char_boundary(5) && rest[1..5].eq_ignore_ascii_case("span");
        let starts_div = rest.len() >= 4 && rest.is_char_boundary(4) && rest[1..4].eq_ignore_ascii_case("div");
        if (starts_span || starts_div)
            && (rest.contains(r#"class="math"#) || rest.contains(r#"class='math"#))
        {
            let close_tag = if starts_span { "</span>" } else { "</div>" };
            if let Some(close_idx) = rest.to_ascii_lowercase().find(close_tag) {
                let end = close_idx + close_tag.len();
                out.push_str(&rest[..end]);
                i += end;
                continue;
            }
        }

        // 5. Any other HTML tag: <...>
        if let Some(tag_end) = rest.find('>') {
            out.push_str(&rest[..tag_end + 1]);
            i += tag_end + 1;
        } else {
            out.push_str(rest);
            break;
        }
    }

    out
}

/// Normalizes all varieties of HTML math markup (Pandoc, MathJax scripts, Arithmatex,
/// pre-rendered KaTeX annotations, and raw delimiters) into canonical Mathia math spans.
pub fn normalize_html_math(html: &str) -> String {
    // Step 1: MathJax script tags
    let re_script_display_math = Regex::new(
        r#"(?is)<script\b[^>]*\btype\s*=\s*['"]math/tex;\s*mode=display['"][^>]*>([\s\S]*?)</script\s*>"#
    ).unwrap();
    let re_script_inline_math = Regex::new(
        r#"(?is)<script\b[^>]*\btype\s*=\s*['"]math/tex['"][^>]*>([\s\S]*?)</script\s*>"#
    ).unwrap();
    let s = re_script_display_math.replace_all(html, |caps: &Captures| {
        format!(r#"<span class="math math-display">{}</span>"#, caps[1].trim())
    });
    let s = re_script_inline_math.replace_all(&s, |caps: &Captures| {
        format!(r#"<span class="math math-inline">{}</span>"#, caps[1].trim())
    });

    // Step 2: Previously saved Mathia math with data-mx-tex
    let re_saved_math = Regex::new(
        r#"(?is)<(span|div)\b([^>]*\bclass\s*=\s*['"][^'"]*\bmath\b[^'"]*['"][^>]*\bdata-mx-tex\s*=\s*['"]([^'"]+)['"][^>]*)>[\s\S]*?</(?:span|div)\s*>"#
    ).unwrap();
    let s = re_saved_math.replace_all(&s, |caps: &Captures| {
        let is_display = caps[2].contains("math-display") || &caps[1] == "div";
        let tex = decode_basic_entities(&caps[3]);
        if is_display {
            format!(r#"<span class="math math-display">{tex}</span>"#)
        } else {
            format!(r#"<span class="math math-inline">{tex}</span>"#)
        }
    });

    // Step 3: Pre-rendered KaTeX spans with annotations
    let re_katex_annot = Regex::new(
        r#"(?is)<span\b[^>]*\bclass\s*=\s*['"][^'"]*\bkatex\b[^'"]*['"][^>]*>[\s\S]*?<annotation\b[^>]*\bencoding\s*=\s*['"]application/x-tex['"][^>]*>([\s\S]*?)</annotation\s*>[\s\S]*?</span\s*>"#
    ).unwrap();
    let s = re_katex_annot.replace_all(&s, |caps: &Captures| {
        let tex = decode_basic_entities(caps[1].trim());
        format!(r#"<span class="math math-inline">{tex}</span>"#)
    });

    // Step 4: Pandoc / Arithmatex / display wrappers
    let re_display_wrappers = Regex::new(
        r#"(?is)<(div|span)\b([^>]*\bclass\s*=\s*['"][^'"]*(?:math\s+display|display\s+math|arithmatex|latex-display|katex-display)\b[^'"]*['"][^>]*)>([\s\S]*?)</(?:div|span)\s*>"#
    ).unwrap();
    let s = re_display_wrappers.replace_all(&s, |caps: &Captures| {
        let inner = caps[3].trim();
        let cleaned = clean_delimiters(inner);
        format!(r#"<span class="math math-display">{cleaned}</span>"#)
    });

    // Step 5: Pandoc / Arithmatex / inline wrappers
    let re_inline_wrappers = Regex::new(
        r#"(?is)<span\b([^>]*\bclass\s*=\s*['"][^'"]*(?:math\s+inline|inline\s+math|arithmatex|latex-inline)\b[^'"]*['"][^>]*)>([\s\S]*?)</span\s*>"#
    ).unwrap();
    let s = re_inline_wrappers.replace_all(&s, |caps: &Captures| {
        let inner = caps[2].trim();
        let cleaned = clean_delimiters(inner);
        format!(r#"<span class="math math-inline">{cleaned}</span>"#)
    });

    // Step 6: Text node math delimiters
    process_html_text_nodes(&s)
}

/// Raw components extracted from an HTML input.
struct ExtractedHtml {
    title: Option<String>,
    cleaned_content: String,
    document: Document,
}

/// Removes all bloat from an HTML document (old scripts, stylesheets, CSP,
/// old Mathia buttons/nav, inline event handlers), extracts its content and metadata,
/// and builds a `Document`.
fn extract_and_clean_html(input: &str) -> ExtractedHtml {
    let input = input.trim_start_matches('\u{feff}');

    // 1. Extract metadata before removing head elements
    let re_title = Regex::new(r"(?is)<title\b[^>]*>([\s\S]*?)</title\s*>").unwrap();
    let title = re_title.captures(input).map(|cap| {
        let raw = strip_html_tags(&cap[1]);
        raw.trim().to_string()
    }).filter(|t| !t.is_empty());

    let re_html_tag = Regex::new(r"(?is)<html\b([^>]*)>").unwrap();
    let re_body_tag = Regex::new(r"(?is)<body\b([^>]*)>").unwrap();
    let re_lang_attr = Regex::new(r#"(?i)\blang\s*=\s*['"]?([^'"\s>]+)"#).unwrap();
    let re_dir_attr = Regex::new(r#"(?i)\bdir\s*=\s*['"]?([^'"\s>]+)"#).unwrap();

    let mut lang = None;
    let mut explicit_dir = None;

    for cap in [re_html_tag.captures(input), re_body_tag.captures(input)].into_iter().flatten() {
        let attrs = &cap[1];
        if lang.is_none() {
            lang = re_lang_attr.captures(attrs).map(|l| l[1].to_string());
        }
        if explicit_dir.is_none() {
            explicit_dir = re_dir_attr.captures(attrs).and_then(|d| {
                let d_str = d[1].to_ascii_lowercase();
                if d_str == "rtl" {
                    Some(Direction::Rtl)
                } else if d_str == "ltr" {
                    Some(Direction::Ltr)
                } else {
                    None
                }
            });
        }
    }

    // 2. Remove bloat

    // 2a. Convert MathJax <script type="math/tex"> tags to Mathia math containers before stripping scripts
    let re_script_display_math = Regex::new(
        r#"(?is)<script\b[^>]*\btype\s*=\s*['"]math/tex;\s*mode=display['"][^>]*>([\s\S]*?)</script\s*>"#
    ).unwrap();
    let re_script_inline_math = Regex::new(
        r#"(?is)<script\b[^>]*\btype\s*=\s*['"]math/tex['"][^>]*>([\s\S]*?)</script\s*>"#
    ).unwrap();
    let s = re_script_display_math.replace_all(input, r#"<span class="math math-display">$1</span>"#);
    let s = re_script_inline_math.replace_all(&s, r#"<span class="math math-inline">$1</span>"#);

    // All remaining <script> tags and <noscript> tags
    let re_script = Regex::new(r"(?is)<script\b[^>]*>[\s\S]*?</script\s*>").unwrap();
    let re_script_self_closing = Regex::new(r"(?is)<script\b[^>]*/>").unwrap();
    let re_noscript = Regex::new(r"(?is)<noscript\b[^>]*>[\s\S]*?</noscript\s*>").unwrap();
    let s = re_script.replace_all(&s, "");
    let s = re_script_self_closing.replace_all(&s, "");
    let s = re_noscript.replace_all(&s, "");

    // 2b. All <style> tags
    let re_style = Regex::new(r"(?is)<style\b[^>]*>[\s\S]*?</style\s*>").unwrap();
    let re_style_self_closing = Regex::new(r"(?is)<style\b[^>]*/>").unwrap();
    let s = re_style.replace_all(&s, "");
    let s = re_style_self_closing.replace_all(&s, "");

    // 2c. Stylesheet <link> tags and preloads
    let re_link = Regex::new(r"(?is)<link\b([^>]*)>").unwrap();
    let re_rel_stylesheet = Regex::new(r#"(?i)\brel\s*=\s*['"]?[^'"]*stylesheet"#).unwrap();
    let re_as_style_or_script = Regex::new(r#"(?i)\bas\s*=\s*['"]?(?:style|script)['"]?"#).unwrap();
    let re_rel_preload = Regex::new(r#"(?i)\brel\s*=\s*['"]?modulepreload['"]?"#).unwrap();
    let re_katex_href = Regex::new(r#"(?i)katex(?:\.min)?\.css"#).unwrap();

    let s = re_link.replace_all(&s, |caps: &Captures| {
        let attrs = &caps[1];
        if re_rel_stylesheet.is_match(attrs)
            || re_as_style_or_script.is_match(attrs)
            || re_rel_preload.is_match(attrs)
            || re_katex_href.is_match(attrs)
        {
            String::new()
        } else {
            caps[0].to_string()
        }
    });

    // 2d. Old CSP meta, generator meta, and base tags
    let re_meta_csp = Regex::new(r#"(?is)<meta\b[^>]*http-equiv\s*=\s*['"]?content-security-policy['"]?[^>]*>"#).unwrap();
    let re_meta_gen = Regex::new(r#"(?is)<meta\b[^>]*name\s*=\s*['"]?generator['"]?[^>]*>"#).unwrap();
    let re_base = Regex::new(r"(?is)<base\b[^>]*>").unwrap();
    let s = re_meta_csp.replace_all(&s, "");
    let s = re_meta_gen.replace_all(&s, "");
    let s = re_base.replace_all(&s, "");

    // 2e. Old Mathia UI buttons and navigation
    let re_mx_buttons = Regex::new(r#"(?is)<button\b[^>]*\bid\s*=\s*['"]?mx-(?:theme-toggle|print|toc-toggle)['"]?[^>]*>[\s\S]*?</button\s*>"#).unwrap();
    let re_mx_toc = Regex::new(r#"(?is)<nav\b[^>]*\bid\s*=\s*['"]?mx-toc['"]?[^>]*>[\s\S]*?</nav\s*>"#).unwrap();
    let re_mx_search = Regex::new(r#"(?is)<div\b[^>]*\bid\s*=\s*['"]?mx-search['"]?[^>]*>[\s\S]*?</div\s*>"#).unwrap();
    let re_mx_math_copy = Regex::new(r#"(?is)<button\b[^>]*\bclass\s*=\s*['"][^'"]*mx-math-copy[^'"]*['"][^>]*>[\s\S]*?</button\s*>"#).unwrap();
    let re_mx_actions_content = Regex::new(r#"(?is)(<span\b[^>]*\bclass\s*=\s*['"][^'"]*mx-actions[^'"]*['"][^>]*>)[\s\S]*?(</span\s*>)"#).unwrap();

    let s = re_mx_buttons.replace_all(&s, "");
    let s = re_mx_toc.replace_all(&s, "");
    let s = re_mx_search.replace_all(&s, "");
    let s = re_mx_math_copy.replace_all(&s, "");
    let s = re_mx_actions_content.replace_all(&s, "$1$2");

    // 2f. Inline event handlers on HTML tags (onload, onclick, onerror, etc.)
    let re_event_handler = Regex::new(r#"(?i)\s+on[a-zA-Z]+\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#).unwrap();
    let s = re_event_handler.replace_all(&s, "");

    // 3. Extract the main content
    let re_main_mx = Regex::new(r#"(?is)<main\b[^>]*\bid\s*=\s*['"]?mx-main['"]?[^>]*>([\s\S]*?)</main\s*>"#).unwrap();
    let re_main = Regex::new(r"(?is)<main\b[^>]*>([\s\S]*?)</main\s*>").unwrap();
    let re_body = Regex::new(r"(?is)<body\b[^>]*>([\s\S]*?)</body\s*>").unwrap();

    let raw_content = if let Some(cap) = re_main_mx.captures(&s) {
        cap[1].trim().to_string()
    } else if let Some(cap) = re_main.captures(&s) {
        cap[1].trim().to_string()
    } else if let Some(cap) = re_body.captures(&s) {
        cap[1].trim().to_string()
    } else {
        // Strip head or doctype if present
        let re_head = Regex::new(r"(?is)^[\s\S]*?</head\s*>").unwrap();
        let stripped = re_head.replace(&s, "");
        let re_doc_wrapper = Regex::new(r"(?is)</?(?:html|body)[^>]*>|<!doctype[^>]*>").unwrap();
        re_doc_wrapper.replace_all(&stripped, "").trim().to_string()
    };

    // Normalize all math markup and delimiters in the extracted content
    let mut raw_content = normalize_html_math(&raw_content);

    // 4. Direction detection
    let plain_text = strip_html_tags(&raw_content);
    let direction = explicit_dir
        .or_else(|| Direction::detect(&plain_text))
        .unwrap_or(Direction::Ltr);

    // 5. Extract blocks and TOC
    let mut blocks = Vec::new();
    let mut toc = Vec::new();

    let has_mx_blocks = raw_content.contains("mx-block") || raw_content.contains("mx-heading");

    if has_mx_blocks {
        // Parse Mathia structured blocks
        let re_block_or_heading = Regex::new(
            r#"(?is)<h([1-6])\b([^>]*\bclass\s*=\s*['"][^'"]*mx-heading[^'"]*['"][^>]*)>([\s\S]*?)</h[1-6]\s*>|<section\b([^>]*\bclass\s*=\s*['"][^'"]*mx-block[^'"]*['"][^>]*)>([\s\S]*?)</section\s*>"#
        ).unwrap();

        let re_id = Regex::new(r#"(?i)\bid\s*=\s*['"]([^'"]+)['"]"#).unwrap();
        let re_block_type = Regex::new(r#"(?i)\bdata-block-type\s*=\s*['"]([^'"]+)['"]"#).unwrap();
        let re_class_mx = Regex::new(r#"(?i)\bmx-([a-z]+)\b"#).unwrap();
        let re_aria_label = Regex::new(r#"(?i)\baria-label\s*=\s*['"]([^'"]+)['"]"#).unwrap();
        let re_proves = Regex::new(r#"(?i)\bdata-proves\s*=\s*['"]([^'"]+)['"]"#).unwrap();
        let re_dir = Regex::new(r#"(?i)\bdir\s*=\s*['"]([^'"]+)['"]"#).unwrap();
        let re_body_inner = Regex::new(r#"(?is)<div\b[^>]*\bclass\s*=\s*['"][^'"]*mx-block-body[^'"]*['"][^>]*>([\s\S]*?)</div\s*>"#).unwrap();

        let mut section_depth = 1u8;

        for cap in re_block_or_heading.captures_iter(&raw_content) {
            if let Some(level_str) = cap.get(1) {
                // Heading
                let level: u8 = level_str.as_str().parse().unwrap_or(2);
                let attrs = &cap[2];
                let inner_html = cap[3].trim().to_string();
                let heading_title = strip_html_tags(&inner_html);
                let id = re_id.captures(attrs).map(|c| c[1].to_string()).unwrap_or_else(|| {
                    slugify(&heading_title, blocks.len() + 1)
                });
                let dir = re_dir.captures(attrs)
                    .and_then(|c| match c[1].to_ascii_lowercase().as_str() {
                        "rtl" => Some(Direction::Rtl),
                        "ltr" => Some(Direction::Ltr),
                        _ => None,
                    })
                    .unwrap_or(direction);

                section_depth = level;

                blocks.push(SemanticBlock {
                    id: id.clone(),
                    kind: BlockType::Heading,
                    span: blocks.len()..blocks.len() + 1,
                    heading_level: Some(level),
                    number: None,
                    name: None,
                    title: heading_title.clone(),
                    direction: dir,
                    score: 0.0,
                    features: None,
                    proves: None,
                    end: EndReason::Heading,
                    html: inner_html,
                });

                toc.push(TocEntry {
                    id,
                    title: heading_title,
                    depth: level,
                    kind: BlockType::Heading,
                });
            } else if let Some(attrs) = cap.get(4) {
                // Semantic block (<section class="mx-block ...">)
                let attrs = attrs.as_str();
                let section_inner = cap[5].trim();
                let id = re_id.captures(attrs).map(|c| c[1].to_string()).unwrap_or_else(|| {
                    format!("block-{}", blocks.len() + 1)
                });
                let kind_slug = re_block_type.captures(attrs).map(|c| c[1].to_string())
                    .or_else(|| {
                        re_class_mx.captures_iter(attrs)
                            .map(|c| c[1].to_string())
                            .find(|slug| slug != "block" && slug != "math")
                    })
                    .unwrap_or_else(|| "text".to_string());

                let kind = match kind_slug.as_str() {
                    "theorem" => BlockType::Theorem,
                    "lemma" => BlockType::Lemma,
                    "proposition" => BlockType::Proposition,
                    "corollary" => BlockType::Corollary,
                    "definition" => BlockType::Definition,
                    "proof" => BlockType::Proof,
                    "example" => BlockType::Example,
                    "remark" => BlockType::Remark,
                    "exercise" => BlockType::Exercise,
                    "solution" => BlockType::Solution,
                    "claim" => BlockType::Claim,
                    "notation" => BlockType::Notation,
                    _ => BlockType::Text,
                };

                let block_title = re_aria_label.captures(attrs)
                    .map(|c| decode_basic_entities(&c[1]))
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| kind.label(direction).to_string());

                let proves = re_proves.captures(attrs).map(|c| c[1].to_string());
                let dir = re_dir.captures(attrs)
                    .and_then(|c| match c[1].to_ascii_lowercase().as_str() {
                        "rtl" => Some(Direction::Rtl),
                        "ltr" => Some(Direction::Ltr),
                        _ => None,
                    })
                    .unwrap_or(direction);

                let body_html = re_body_inner.captures(section_inner)
                    .map(|c| c[1].trim().to_string())
                    .unwrap_or_else(|| section_inner.to_string());

                blocks.push(SemanticBlock {
                    id: id.clone(),
                    kind,
                    span: blocks.len()..blocks.len() + 1,
                    heading_level: None,
                    number: None,
                    name: None,
                    title: block_title.clone(),
                    direction: dir,
                    score: 1.0,
                    features: None,
                    proves,
                    end: EndReason::Structural,
                    html: body_html,
                });

                if kind.is_semantic() {
                    toc.push(TocEntry {
                        id,
                        title: block_title,
                        depth: section_depth + 1,
                        kind,
                    });
                }
            }
        }
    } else {
        // Generic HTML document: scan for headings <h1..h6>
        let re_heading = Regex::new(r"(?is)<h([1-6])\b([^>]*)>([\s\S]*?)</h[1-6]\s*>").unwrap();
        let re_id = Regex::new(r#"(?i)\bid\s*=\s*['"]([^'"]+)['"]"#).unwrap();
        let mut replacements = Vec::new();
        let mut counter = 0;

        for cap in re_heading.captures_iter(&raw_content) {
            counter += 1;
            let full_match = cap.get(0).unwrap();
            let level: u8 = cap[1].parse().unwrap_or(2);
            let attrs = &cap[2];
            let inner_html = cap[3].trim().to_string();
            let heading_title = strip_html_tags(&inner_html);

            let (id, updated_tag) = if let Some(existing_id) = re_id.captures(attrs) {
                (existing_id[1].to_string(), None)
            } else {
                let generated_id = slugify(&heading_title, counter);
                let updated = format!("<h{level} id=\"{generated_id}\"{attrs}>{inner_html}</h{level}>");
                (generated_id, Some(updated))
            };

            if let Some(replacement) = updated_tag {
                replacements.push((full_match.range(), replacement));
            }

            blocks.push(SemanticBlock {
                id: id.clone(),
                kind: BlockType::Heading,
                span: blocks.len()..blocks.len() + 1,
                heading_level: Some(level),
                number: None,
                name: None,
                title: heading_title.clone(),
                direction,
                score: 0.0,
                features: None,
                proves: None,
                end: EndReason::Heading,
                html: inner_html,
            });

            toc.push(TocEntry {
                id,
                title: heading_title,
                depth: level,
                kind: BlockType::Heading,
            });
        }

        // Apply heading id additions in reverse offset order
        replacements.sort_by_key(|r| std::cmp::Reverse(r.0.start));
        for (range, repl) in replacements {
            raw_content.replace_range(range, &repl);
        }

        // If no blocks found, add a single fallback content block
        if blocks.is_empty() {
            blocks.push(SemanticBlock {
                id: "content-1".to_string(),
                kind: BlockType::Text,
                span: 0..1,
                heading_level: None,
                number: None,
                name: None,
                title: "Content".to_string(),
                direction,
                score: 0.0,
                features: None,
                proves: None,
                end: EndReason::EndOfDocument,
                html: raw_content.clone(),
            });
        }
    }

    let final_title = title.or_else(|| {
        blocks
            .iter()
            .find(|b| b.kind == BlockType::Heading && b.heading_level == Some(1))
            .or_else(|| blocks.iter().find(|b| b.kind == BlockType::Heading))
            .map(|b| b.title.clone())
    });

    let document = Document {
        title: final_title.clone(),
        direction,
        nodes: Vec::new(),
        blocks,
        toc,
    };

    ExtractedHtml {
        title: final_title,
        cleaned_content: raw_content,
        document,
    }
}

/// Takes an existing HTML document, strips all bloat, and replaces it with
/// Mathia's modern scripts, styles, and micro-runtime.
pub fn clean_html(input: &str, options: &HtmlOptions) -> Result<String> {
    let extracted = extract_and_clean_html(input);

    let mut options = options.clone();
    if options.title.is_none() {
        options.title = extracted.title;
    }

    html::render_with_raw_content(&extracted.document, &options, &extracted.cleaned_content)
}

/// Parses an existing HTML document into a Mathia `Document`.
pub fn analyze_html(input: &str) -> Document {
    extract_and_clean_html(input).document
}

/// Detailed analysis of an existing HTML document with score mappings for the desktop app.
pub fn analyze_html_detailed(input: &str) -> Analysis {
    let document = extract_and_clean_html(input).document;
    let mut scores = Vec::with_capacity(document.blocks.len());
    let mut block_of = Vec::with_capacity(document.blocks.len());

    for (index, block) in document.blocks.iter().enumerate() {
        scores.push(NodeScore {
            features: crate::features::Features::default(),
            best: (block.kind, block.score),
            opens: block.kind.is_semantic(),
            scores: vec![(block.kind, block.score)],
        });
        block_of.push(index);
    }

    Analysis {
        document,
        scores,
        block_of,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_html_correctly() {
        assert!(is_html("<!DOCTYPE html><html><body><h1>Test</h1></body></html>"));
        assert!(is_html("   <html lang=\"fa\"><head></head><body></body></html>"));
        assert!(is_html("<main id=\"mx-main\">content</main>"));
        assert!(is_html("<section class=\"mx-block mx-theorem\"><p>...</p></section>"));
        assert!(is_html("<body><p>Hello world</p></body>"));

        assert!(!is_html("# Heading\n\n**Theorem 1.** Let $x > 0$."));
        assert!(!is_html("Some text with <span class=\"math\">$x$</span> inline."));
    }

    #[test]
    fn removes_old_scripts_and_stylesheets() {
        let dirty = r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Sample Notes</title>
  <meta http-equiv="Content-Security-Policy" content="default-src 'self'">
  <link rel="stylesheet" href="bootstrap.min.css">
  <link rel="stylesheet" href="katex.min.css">
  <link rel="preload" as="style" href="font.css">
  <link rel="preload" as="script" href="bundle.js">
  <style>
    body { background: #fff; font-size: 16px; }
    .bad { color: red; }
  </style>
  <script src="tracking.js"></script>
  <script>
    console.log("old script 1");
  </script>
</head>
<body onload="alert('bad');" onclick="doSomething()">
  <button id="mx-theme-toggle" class="mx-theme-toggle">Old theme</button>
  <button id="mx-print" class="mx-print">Old print</button>
  <button id="mx-toc-toggle" class="mx-toc-toggle">Old toc toggle</button>
  <nav id="mx-toc" class="mx-toc">Old toc content</nav>
  <main id="mx-main">
    <h1 id="sec-1">Calculus Notes</h1>
    <p>Integration by parts: $\int u dv = uv - \int v du$.</p>
  </main>
  <script id="mx-runtime">console.log("old runtime");</script>
  <script src="jquery.min.js"></script>
</body>
</html>"#;

        let options = HtmlOptions::default();
        let cleaned = clean_html(dirty, &options).expect("cleaning succeeds");

        // Old scripts and bloat must NOT appear
        assert!(!cleaned.contains("tracking.js"));
        assert!(!cleaned.contains("old script 1"));
        assert!(!cleaned.contains("old runtime"));
        assert!(!cleaned.contains("jquery.min.js"));
        assert!(!cleaned.contains("bootstrap.min.css"));
        assert!(!cleaned.contains("background: #fff; font-size: 16px;"));
        assert!(!cleaned.contains("onload="));
        assert!(!cleaned.contains("onclick="));
        assert!(!cleaned.contains("Old theme"));
        assert!(!cleaned.contains("Old toc content"));

        // New Mathia scripts and styles MUST appear
        assert!(cleaned.contains("<style id=\"mx-style\">"));
        assert!(cleaned.contains("<script id=\"mx-runtime\">"));
        assert!(cleaned.contains("Mathia"));
        assert!(cleaned.contains("Calculus Notes"));
        assert!(cleaned.contains("Integration by parts"));
        assert!(cleaned.contains("<button id=\"mx-theme-toggle\""));
        assert!(cleaned.contains("<button id=\"mx-print\""));
        assert!(cleaned.contains("<button id=\"mx-toc-toggle\""));
        assert!(cleaned.contains("<nav id=\"mx-toc\""));
        assert!(cleaned.contains("Content-Security-Policy"));
    }

    #[test]
    fn extracts_blocks_from_previous_mathia_document() {
        let old_mathia = r#"<!DOCTYPE html>
<html lang="fa" dir="rtl">
<head>
<meta charset="utf-8">
<title>آنالیز ریاضی</title>
<style id="mx-style">body { font-size: 10px; }</style>
</head>
<body>
<main id="mx-main">
  <h1 id="sec-1" class="mx-heading" data-block-id="sec-1" data-block-type="heading" dir="rtl">فصل اول</h1>
  <section id="block-theorem-1" class="mx-block mx-theorem" data-block-id="block-theorem-1" data-block-type="theorem" dir="rtl" aria-label="قضیه بولتسانو–وایراشتراس">
    <header class="mx-block-head"><span class="mx-badge">قضیه</span><span class="mx-actions" data-block-actions="block-theorem-1"><button>Old</button></span></header>
    <div class="mx-block-body">
      <p>هر دنباله کراندار در $\mathbb{R}$ دارای یک زیردنباله همگرا است.</p>
    </div>
  </section>
</main>
<script id="mx-runtime">/* old */</script>
</body>
</html>"#;

        let doc = analyze_html(old_mathia);
        assert_eq!(doc.direction, Direction::Rtl);
        assert_eq!(doc.title.as_deref(), Some("آنالیز ریاضی"));
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[0].kind, BlockType::Heading);
        assert_eq!(doc.blocks[0].title, "فصل اول");
        assert_eq!(doc.blocks[1].kind, BlockType::Theorem);
        assert_eq!(doc.blocks[1].title, "قضیه بولتسانو–وایراشتراس");

        let cleaned = clean_html(old_mathia, &HtmlOptions::default()).expect("renders");
        assert!(cleaned.contains("dir=\"rtl\""));
        assert!(cleaned.contains("lang=\"fa\""));
        assert!(cleaned.contains("قضیه بولتسانو–وایراشتراس"));
        assert!(cleaned.contains("فصل اول"));
        // Old action button removed
        assert!(!cleaned.contains("<button>Old</button>"));
    }

    #[test]
    fn cleans_generic_html_and_assigns_ids_to_headings() {
        let generic = r#"<!DOCTYPE html>
<html>
<head><title>Linear Algebra</title></head>
<body>
  <h1>Vector Spaces</h1>
  <p>A vector space $V$ over field $F$...</p>
  <h2>Subspaces</h2>
  <p>A subset $W \subseteq V$ is a subspace if...</p>
</body>
</html>"#;

        let cleaned = clean_html(generic, &HtmlOptions::default()).expect("renders");
        assert!(cleaned.contains("id=\"vector-spaces\""));
        assert!(cleaned.contains("id=\"subspaces\""));
        assert!(cleaned.contains("<a href=\"#vector-spaces\" data-toc-target=\"vector-spaces\">Vector Spaces</a>"));
        assert!(cleaned.contains("<a href=\"#subspaces\" data-toc-target=\"subspaces\">Subspaces</a>"));
        assert!(cleaned.contains(r#"<span class="math math-inline">V</span>"#));
        assert!(cleaned.contains(r#"<span class="math math-inline">F</span>"#));
        assert!(cleaned.contains(r#"<span class="math math-inline">W \subseteq V</span>"#));
    }

    #[test]
    fn converts_and_normalizes_all_html_math_dialects() {
        let doc_with_math = r#"<!DOCTYPE html>
<html>
<head><title>Various Math Formats</title></head>
<body>
  <p>MathJax inline: <script type="math/tex">E = mc^2</script></p>
  <p>MathJax display: <script type="math/tex; mode=display">\sum_{i=1}^n i</script></p>
  <p>Pandoc inline: <span class="math inline">\(a + b = c\)</span></p>
  <p>Pandoc display: <span class="math display">\[\int_0^\infty e^{-x} dx = 1\]</span></p>
  <div class="arithmatex">\[x^2 + y^2 = r^2\]</div>
  <p>Raw text inline: $f(x) = \sin(x)$ and raw display:</p>
  <p>$$\lim_{x \to 0} \frac{\sin x}{x} = 1$$</p>
  <p>LaTeX brackets display: \[A x = \lambda x\] and parens: \(p \implies q\)</p>
  <p>\begin{align}
    u &= v \\
    w &= z
  \end{align}</p>
  <p>Code should stay intact: <code>$not_math$</code></p>
  <pre>$also_not_math$</pre>
</body>
</html>"#;

        let cleaned = clean_html(doc_with_math, &HtmlOptions::default()).expect("renders");

        // MathJax scripts converted
        assert!(cleaned.contains(r#"<span class="math math-inline">E = mc^2</span>"#));
        assert!(cleaned.contains(r#"<span class="math math-display">\sum_{i=1}^n i</span>"#));

        // Pandoc spans normalized (delimiters stripped)
        assert!(cleaned.contains(r#"<span class="math math-inline">a + b = c</span>"#));
        assert!(cleaned.contains(r#"<span class="math math-display">\int_0^\infty e^{-x} dx = 1</span>"#));

        // Arithmatex normalized
        assert!(cleaned.contains(r#"<span class="math math-display">x^2 + y^2 = r^2</span>"#));

        // Raw text math converted
        assert!(cleaned.contains(r#"<span class="math math-inline">f(x) = \sin(x)</span>"#));
        assert!(cleaned.contains(r#"<span class="math math-display">\lim_{x \to 0} \frac{\sin x}{x} = 1</span>"#));
        assert!(cleaned.contains(r#"<span class="math math-display">A x = \lambda x</span>"#));
        assert!(cleaned.contains(r#"<span class="math math-inline">p \implies q</span>"#));
        assert!(cleaned.contains("begin{align}"));

        // Protected blocks left unmodified
        assert!(cleaned.contains("<code>$not_math$</code>"));
        assert!(cleaned.contains("<pre>$also_not_math$</pre>"));
    }
}
