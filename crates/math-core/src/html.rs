//! [`Document`] → one self-contained HTML5 file.

use std::path::Path;

use askama::Template;

use crate::error::{Error, Result};
use crate::model::{BlockType, Document};
use crate::sha256::sha256;

const STYLE_CSS: &str = include_str!("../assets/style.css");
const AI_CSS: &str = include_str!("../assets/ai.css");
const RUNTIME_JS: &str = include_str!("../assets/runtime.js");
const AI_JS: &str = include_str!("../assets/ai.js");

#[cfg(feature = "bundled-katex")]
const BUNDLED_KATEX_CSS: &str = include_str!("../assets/katex/katex.prepared.css");
#[cfg(feature = "bundled-katex")]
const BUNDLED_KATEX_JS: &str = include_str!("../assets/katex/katex.min.js");

/// KaTeX stylesheet (fonts already inlined as `data:` URIs) and script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KatexAssets {
    pub css: String,
    pub js: String,
}

/// Where the compiled file gets KaTeX from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MathAssets {
    /// Embedded in the file: works offline. The shipping default.
    Inline(KatexAssets),
    /// Loaded from jsDelivr: small files for development, needs a network.
    Cdn { version: String },
    /// No renderer; math stays as LaTeX source in `.math` spans.
    #[default]
    None,
}

/// A text font embedded in the compiled file as a `data:` URI, so Persian
/// renders the same on machines that do not have the font installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedFont {
    /// CSS family name, e.g. `Vazirmatn`. Put first in the document's font stack.
    pub family: String,
    /// CSS `font-weight` descriptor: `400`, or a range such as `100 900` for
    /// a variable font.
    pub weight: String,
    pub woff2: Vec<u8>,
}

impl EmbeddedFont {
    /// Reads a WOFF2 file (ideally a subset: every byte ends up in the output).
    pub fn from_file(path: impl AsRef<Path>, family: &str, weight: &str) -> Result<Self> {
        let path = path.as_ref();
        let woff2 = std::fs::read(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(EmbeddedFont {
            family: family.to_string(),
            weight: weight.to_string(),
            woff2,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AiProvider {
    Anthropic,
    OpenAi,
    Gemini,
    /// Any OpenAI-compatible `/chat/completions` server (OpenRouter, Groq, Together,
    /// DeepSeek, vLLM, LM Studio, Ollama, ...). The reader types the base URL into the
    /// settings dialog, so the page must be allowed to connect to any http(s) host.
    Compatible,
}

impl AiProvider {
    pub const ALL: [AiProvider; 4] = [
        AiProvider::Anthropic,
        AiProvider::OpenAi,
        AiProvider::Gemini,
        AiProvider::Compatible,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
            Self::Compatible => "openai-compatible",
        }
    }

    /// The CSP `connect-src` source expression for this provider: one origin, or
    /// scheme sources for [`AiProvider::Compatible`], whose host is chosen at runtime.
    pub fn origin(self) -> &'static str {
        match self {
            Self::Anthropic => "https://api.anthropic.com",
            Self::OpenAi => "https://api.openai.com",
            Self::Gemini => "https://generativelanguage.googleapis.com",
            Self::Compatible => "https: http:",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "anthropic" => Some(Self::Anthropic),
            "openai" => Some(Self::OpenAi),
            "gemini" => Some(Self::Gemini),
            "openai-compatible" => Some(Self::Compatible),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CustomProvider {
    /// Identifier slug, e.g. "custom", "ollama", "deepseek".
    pub slug: String,
    /// Origin for CSP `connect-src`, e.g. "https://api.deepseek.com" or "http://localhost:11434".
    pub origin: String,
    /// Optional default base URL or endpoint, e.g. "https://api.deepseek.com/v1".
    pub endpoint: Option<String>,
}

impl CustomProvider {
    pub fn new(slug: impl Into<String>, origin: impl Into<String>) -> Self {
        Self {
            slug: slug.into(),
            origin: origin.into(),
            endpoint: None,
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Parses a custom provider from a URL or `NAME=URL` string.
    ///
    /// The origin is automatically extracted from the URL scheme and authority
    /// (e.g. `https://api.deepseek.com/v1` -> origin `https://api.deepseek.com`).
    pub fn from_spec(spec: &str) -> std::result::Result<Self, String> {
        let spec = spec.trim();
        let (slug, url) = if let Some((name, u)) = spec.split_once('=') {
            (name.trim(), u.trim())
        } else {
            ("custom", spec)
        };

        if slug.is_empty() {
            return Err("custom provider name cannot be empty".to_string());
        }
        if !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!(
                "invalid custom provider name '{slug}': only alphanumeric, '-', and '_' are allowed"
            ));
        }

        let origin = extract_origin(url).ok_or_else(|| {
            format!(
                "invalid URL for custom AI provider '{slug}': '{url}' (expected http:// or https:// URL)"
            )
        })?;

        Ok(Self {
            slug: slug.to_ascii_lowercase(),
            origin,
            endpoint: Some(url.to_string()),
        })
    }
}

/// Extracts `<scheme>://<host>[:<port>]` from a URL string.
pub fn extract_origin(url: &str) -> Option<String> {
    let url = url.trim();
    let (scheme, rest) = if let Some(stripped) = url.strip_prefix("https://") {
        ("https://", stripped)
    } else {
        let stripped = url.strip_prefix("http://")?;
        ("http://", stripped)
    };
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}{authority}"))
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AiOptions {
    pub providers: Vec<AiProvider>,
    pub custom_providers: Vec<CustomProvider>,
}

impl AiOptions {
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty() && self.custom_providers.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct HtmlOptions {
    pub math: MathAssets,
    /// Overrides the title taken from the document's first heading.
    pub title: Option<String>,
    /// Text fonts to embed; the first family leads the font stack.
    pub fonts: Vec<EmbeddedFont>,
    pub ai: AiOptions,
}

struct BlockView<'a> {
    id: &'a str,
    kind: &'static str,
    dir: &'static str,
    label: &'static str,
    title: &'a str,
    /// 0 for anything that is not a structural heading.
    heading_level: u8,
    semantic: bool,
    /// Id and title of the block this one proves or answers; empty if none.
    proves: &'a str,
    proves_title: &'a str,
    html: &'a str,
}

struct TocView<'a> {
    id: &'a str,
    kind: &'static str,
    depth: u8,
    title: &'a str,
}

#[derive(Template)]
#[template(path = "document.html")]
struct DocumentTemplate<'a> {
    title: &'a str,
    lang: &'static str,
    dir: &'static str,
    toc_label: &'static str,
    theme_label: &'static str,
    search_label: &'static str,
    search_none: &'static str,
    search_prev: &'static str,
    search_next: &'static str,
    csp: String,
    css: String,
    runtime_js: String,
    ai_js: Option<String>,
    ai_attr: String,
    ai_endpoints: String,
    math_head: String,
    toc: Vec<TocView<'a>>,
    blocks: Vec<BlockView<'a>>,
}

pub fn render(doc: &Document, options: &HtmlOptions) -> Result<String> {
    let rtl = doc.direction == crate::model::Direction::Rtl;
    let title = options
        .title
        .as_deref()
        .or(doc.title.as_deref())
        .unwrap_or(if rtl { "جزوه" } else { "Study guide" });

    let runtime_js = RUNTIME_JS.replace("</script", "<\\/script");
    let katex = match &options.math {
        MathAssets::Inline(katex) => Some(KatexAssets {
            // Keep embedded sources from closing their own element.
            css: katex.css.replace("</style", "<\\/style"),
            js: katex.js.replace("</script", "<\\/script"),
        }),
        _ => None,
    };
    let active_providers: Vec<AiProvider> = AiProvider::ALL
        .into_iter()
        .filter(|p| options.ai.providers.contains(p))
        .collect();
    let has_ai = !options.ai.is_empty();
    let ai_js = if has_ai {
        Some(AI_JS.replace("</script", "<\\/script"))
    } else {
        None
    };
    let mut provider_slugs: Vec<String> = active_providers
        .iter()
        .map(|p| p.slug().to_string())
        .collect();
    for custom in &options.ai.custom_providers {
        if !provider_slugs.contains(&custom.slug) {
            provider_slugs.push(custom.slug.clone());
        }
    }
    let ai_attr = if has_ai {
        provider_slugs.join(" ")
    } else {
        String::new()
    };
    let ai_endpoints = if has_ai {
        let pairs: Vec<String> = options
            .ai
            .custom_providers
            .iter()
            .filter_map(|c| c.endpoint.as_ref().map(|ep| format!("{}={}", c.slug, ep)))
            .collect();
        pairs.join(" ")
    } else {
        String::new()
    };
    let mut inline_scripts = vec![runtime_js.as_str()];
    inline_scripts.extend(katex.as_ref().map(|k| k.js.as_str()));
    if let Some(ref js) = ai_js {
        inline_scripts.push(js.as_str());
    }
    let mut css = font_css(&options.fonts);
    css.push_str(STYLE_CSS);
    if has_ai {
        css.push_str(AI_CSS);
    }
    let mut connect_origins: Vec<String> = active_providers
        .iter()
        .map(|p| p.origin().to_string())
        .collect();
    for custom in &options.ai.custom_providers {
        if !connect_origins.contains(&custom.origin) {
            connect_origins.push(custom.origin.clone());
        }
    }

    let template = DocumentTemplate {
        title,
        lang: if rtl { "fa" } else { "en" },
        dir: doc.direction.as_str(),
        toc_label: if rtl { "فهرست" } else { "Contents" },
        theme_label: if rtl {
            "تغییر پوسته"
        } else {
            "Switch theme"
        },
        search_label: if rtl { "جستجو در متن…" } else { "Search the text…" },
        search_none: if rtl { "نتیجه‌ای نیست" } else { "No results" },
        search_prev: if rtl { "نتیجهٔ قبلی" } else { "Previous result" },
        search_next: if rtl { "نتیجهٔ بعدی" } else { "Next result" },
        csp: content_security_policy(&inline_scripts, &options.math, &connect_origins),
        css: css.replace("</style", "<\\/style"),
        math_head: math_head(&options.math, katex.as_ref()),
        runtime_js,
        ai_js,
        ai_attr,
        ai_endpoints,
        toc: doc
            .toc
            .iter()
            .map(|e| TocView {
                id: &e.id,
                kind: e.kind.slug(),
                depth: e.depth.max(1),
                title: &e.title,
            })
            .collect(),
        blocks: doc
            .blocks
            .iter()
            .map(|b| BlockView {
                id: &b.id,
                kind: b.kind.slug(),
                dir: b.direction.as_str(),
                label: b.kind.label(b.direction),
                title: &b.title,
                heading_level: if b.kind == BlockType::Heading {
                    b.heading_level.unwrap_or(2).clamp(1, 6)
                } else {
                    0
                },
                semantic: b.kind.is_semantic(),
                proves: b.proves.as_deref().unwrap_or(""),
                proves_title: b
                    .proves
                    .as_deref()
                    .and_then(|id| doc.blocks.iter().find(|t| t.id == id))
                    .map_or("", |t| t.title.as_str()),
                html: &b.html,
            })
            .collect(),
    };
    Ok(template.render()?)
}

fn math_head(math: &MathAssets, inline: Option<&KatexAssets>) -> String {
    match (math, inline) {
        (MathAssets::Inline(_), Some(katex)) => format!(
            "<style id=\"mx-katex-css\">{}</style>\n<script id=\"mx-katex-js\">{}</script>",
            katex.css, katex.js,
        ),
        (MathAssets::Cdn { version }, _) => {
            let version: String = version
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '.')
                .collect();
            format!(
                "<link rel=\"stylesheet\" href=\"{KATEX_CDN}/npm/katex@{version}/dist/katex.min.css\">\n\
                 <script src=\"{KATEX_CDN}/npm/katex@{version}/dist/katex.min.js\"></script>"
            )
        }
        _ => String::new(),
    }
}

const KATEX_CDN: &str = "https://cdn.jsdelivr.net";

/// The policy of the compiled file: nothing is loaded or executed except the
/// scripts Mathia inlined itself (allowed by hash, so an injected `<script>`,
/// event-handler attribute or `javascript:` URL cannot run), inline styles
/// (KaTeX needs them), embedded fonts, and images.
fn content_security_policy(
    inline_scripts: &[&str],
    math: &MathAssets,
    connect_origins: &[impl AsRef<str>],
) -> String {
    let cdn = match math {
        MathAssets::Cdn { .. } => format!(" {KATEX_CDN}"),
        _ => String::new(),
    };
    let hashes: String = inline_scripts
        .iter()
        .map(|script| format!(" 'sha256-{}'", base64(&sha256(script.as_bytes()))))
        .collect();
    let mut csp = format!(
        "default-src 'none'; script-src{hashes}{cdn}; style-src 'unsafe-inline'{cdn}; \
         font-src data:{cdn}; img-src 'self' data: blob: https: http: file:;"
    );
    if !connect_origins.is_empty() {
        let origins = connect_origins
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join(" ");
        csp.push_str(&format!(" connect-src {origins};"));
    }
    csp.push_str(" base-uri 'none'; form-action 'none'");
    csp
}

/// `@font-face` rules for the embedded fonts, plus the font stack led by them.
fn font_css(fonts: &[EmbeddedFont]) -> String {
    let clean = |text: &str, extra: &[char]| -> String {
        text.chars()
            .filter(|c| c.is_alphanumeric() || extra.contains(c))
            .collect()
    };
    let mut css = String::new();
    let mut families: Vec<String> = Vec::new();
    for font in fonts {
        let family = clean(&font.family, &[' ', '-', '_']);
        if family.trim().is_empty() || font.woff2.is_empty() {
            continue;
        }
        let weight = clean(&font.weight, &[' ']);
        let weight = if weight.trim().is_empty() {
            "400"
        } else {
            weight.trim()
        };
        css.push_str(&format!(
            "@font-face{{font-family:\"{family}\";src:url(data:font/woff2;base64,{}) format(\"woff2\");font-weight:{weight};font-display:swap}}\n",
            base64(&font.woff2)
        ));
        if !families.contains(&family) {
            families.push(family);
        }
    }
    if !families.is_empty() {
        let stack: String = families.iter().map(|f| format!("\"{f}\", ")).collect();
        css.push_str(&format!(":root{{--mx-font-embedded:{stack}}}\n"));
    }
    css
}

impl KatexAssets {
    /// Loads a KaTeX `dist/` directory (`katex.min.css`, `katex.min.js`,
    /// `fonts/*.woff2`) and inlines the fonts, so the result needs no files.
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let read = |name: &str| {
            let path = dir.join(name);
            std::fs::read(&path).map_err(|source| Error::Io { path, source })
        };
        let css = String::from_utf8_lossy(&read("katex.min.css")?).into_owned();
        let js = String::from_utf8_lossy(&read("katex.min.js")?).into_owned();
        let css = inline_fonts(&css, |file| read(file))?;
        Ok(KatexAssets { css, js })
    }

    /// Pre-bundled KaTeX assets embedded at compile time (requires `bundled-katex` feature).
    #[cfg(feature = "bundled-katex")]
    pub fn bundled() -> Self {
        KatexAssets {
            css: BUNDLED_KATEX_CSS.to_string(),
            js: BUNDLED_KATEX_JS.to_string(),
        }
    }
}

impl MathAssets {
    /// Offline inline KaTeX assets embedded at compile time (requires `bundled-katex` feature).
    #[cfg(feature = "bundled-katex")]
    pub fn bundled() -> Self {
        MathAssets::Inline(KatexAssets::bundled())
    }
}

/// Rewrites every `@font-face` `src:` list to a single embedded WOFF2 source.
/// `load` receives paths relative to the stylesheet (`fonts/KaTeX_Main-Regular.woff2`).
fn inline_fonts(css: &str, mut load: impl FnMut(&str) -> Result<Vec<u8>>) -> Result<String> {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("src:") {
        let (before, after) = rest.split_at(at + "src:".len());
        out.push_str(before);
        let end = after.find([';', '}']).unwrap_or(after.len());
        let (sources, tail) = after.split_at(end);

        let woff2 = sources.split(',').find_map(|source| {
            let open = source.find("url(")? + "url(".len();
            let close = open + source[open..].find(')')?;
            let path = source[open..close].trim().trim_matches(['"', '\'']);
            path.ends_with(".woff2").then_some(path)
        });
        match woff2 {
            Some(path) => {
                let bytes = load(path)?;
                out.push_str("url(data:font/woff2;base64,");
                out.push_str(&base64(&bytes));
                out.push_str(") format(\"woff2\")");
            }
            None => out.push_str(sources),
        }
        rest = tail;
    }
    out.push_str(rest);
    Ok(out)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().fold(0u32, |acc, b| (acc << 8) | *b as u32) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ScoringConfig, analyze};

    const SAMPLE: &str = "# آنالیز\n\n**قضیه ۱ (رول).** اگر $f(a)=f(b)$ آنگاه $f'(c)=0$.\n\n\
                          **برهان.** بدیهی است. $\\blacksquare$\n\nTheorem 2. English text.\n";

    fn render_sample(math: MathAssets) -> String {
        let doc = analyze(SAMPLE, &ScoringConfig::default());
        render(
            &doc,
            &HtmlOptions {
                math,
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn document_shell_is_rtl_aware() {
        let html = render_sample(MathAssets::None);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains(r#"<html lang="fa" dir="rtl">"#));
        assert!(html.contains("<title>آنالیز</title>"));
        assert!(html.contains(r#"<h1 id="sec-1""#));
    }

    #[test]
    fn blocks_carry_runtime_hooks() {
        let html = render_sample(MathAssets::None);
        assert!(html.contains(
            r#"<section id="block-theorem-1" class="mx-block mx-theorem" data-block-id="block-theorem-1" data-block-type="theorem" dir="rtl""#
        ));
        assert!(html.contains(r#"data-block-id="block-proof-1""#));
        assert!(
            html.contains(r#"data-block-id="block-theorem-2" data-block-type="theorem" dir="ltr""#)
        );
        assert!(html.contains("data-block-actions"));
        assert!(html.contains(r#"<span class="math math-inline">f(a)=f(b)</span>"#));
    }

    #[test]
    fn sidebar_lists_sections_and_blocks() {
        let html = render_sample(MathAssets::None);
        assert!(html.contains(r#"id="mx-toc""#));
        assert!(html.contains(r##"href="#block-theorem-1""##));
        assert!(html.contains("قضیه ۱ (رول)"));
        assert!(html.contains(r##"href="#sec-1""##));
        assert!(html.contains("mx-toc-toggle"));
    }

    #[test]
    fn math_assets_modes() {
        assert!(!render_sample(MathAssets::None).contains("katex.min.js"));
        let cdn = render_sample(MathAssets::Cdn {
            version: "0.16.22\"><x".into(),
        });
        assert!(cdn.contains("katex@0.16.22x/dist/katex.min.js"));

        let inline = render_sample(MathAssets::Inline(KatexAssets {
            css: ".katex{font:x}".into(),
            js: "var katex={};var s='</script>';".into(),
        }));
        assert!(inline.contains(".katex{font:x}"));
        assert!(inline.contains(r"var s='<\/script>';"));
        assert!(!inline.contains("cdn.jsdelivr.net"));
    }

    #[test]
    fn ai_provider_properties() {
        assert_eq!(AiProvider::Anthropic.slug(), "anthropic");
        assert_eq!(AiProvider::OpenAi.slug(), "openai");
        assert_eq!(AiProvider::Gemini.slug(), "gemini");
        assert_eq!(AiProvider::Compatible.slug(), "openai-compatible");
        assert_eq!(AiProvider::Compatible.origin(), "https: http:");

        assert_eq!(AiProvider::Anthropic.origin(), "https://api.anthropic.com");
        assert_eq!(AiProvider::OpenAi.origin(), "https://api.openai.com");
        assert_eq!(
            AiProvider::Gemini.origin(),
            "https://generativelanguage.googleapis.com"
        );

        assert_eq!(
            AiProvider::from_slug("anthropic"),
            Some(AiProvider::Anthropic)
        );
        assert_eq!(AiProvider::from_slug("openai"), Some(AiProvider::OpenAi));
        assert_eq!(AiProvider::from_slug("gemini"), Some(AiProvider::Gemini));
        assert_eq!(
            AiProvider::from_slug("openai-compatible"),
            Some(AiProvider::Compatible)
        );
        assert_eq!(AiProvider::from_slug("invalid"), None);
        assert_eq!(
            AiProvider::ALL,
            [
                AiProvider::Anthropic,
                AiProvider::OpenAi,
                AiProvider::Gemini,
                AiProvider::Compatible
            ]
        );
    }

    #[test]
    fn ai_embedding_and_csp() {
        let doc = analyze(SAMPLE, &ScoringConfig::default());

        // Disabled -> no connect-src, no mx-ai, no data-mx-ai
        let html_disabled = render(&doc, &HtmlOptions::default()).unwrap();
        assert!(!html_disabled.contains("connect-src"));
        assert!(!html_disabled.contains("mx-ai"));
        assert!(!html_disabled.contains("data-mx-ai"));

        // Enabled with [Gemini, Anthropic]
        let options_enabled = HtmlOptions {
            ai: AiOptions {
                providers: vec![AiProvider::Gemini, AiProvider::Anthropic],
                ..Default::default()
            },
            ..Default::default()
        };
        let html_enabled = render(&doc, &options_enabled).unwrap();

        assert!(html_enabled.contains(
            "connect-src https://api.anthropic.com https://generativelanguage.googleapis.com"
        ));
        assert!(html_enabled.contains(r#"data-mx-ai="anthropic gemini""#));

        // Script present after mx-runtime
        let runtime_pos = html_enabled
            .find(r#"id="mx-runtime""#)
            .expect("mx-runtime present");
        let ai_pos = html_enabled.find(r#"id="mx-ai""#).expect("mx-ai present");
        assert!(runtime_pos < ai_pos, "mx-ai must be after mx-runtime");

        // And its SHA-256 hash in the CSP (compute it the same way content_security_policy does)
        let ai_script = AI_JS.replace("</script", "<\\/script");
        let expected_hash = format!("'sha256-{}'", base64(&sha256(ai_script.as_bytes())));
        let csp_start = html_enabled
            .find("Content-Security-Policy")
            .expect("CSP meta tag");
        let csp_end = html_enabled[csp_start..]
            .find('>')
            .expect("end of meta tag");
        let csp_content = &html_enabled[csp_start..csp_start + csp_end];
        assert!(
            csp_content.contains(&expected_hash),
            "CSP missing ai.js hash: {expected_hash}"
        );
    }

    #[test]
    fn openai_compatible_provider_allows_any_http_host() {
        let doc = analyze(SAMPLE, &ScoringConfig::default());
        let options = HtmlOptions {
            ai: AiOptions {
                providers: vec![AiProvider::Compatible],
                ..Default::default()
            },
            ..Default::default()
        };
        let html = render(&doc, &options).unwrap();
        assert!(html.contains("connect-src https: http:;"));
        assert!(html.contains(r#"data-mx-ai="openai-compatible""#));
        assert!(!html.contains(r#"data-mx-ai-endpoints=""#));
        assert!(!html.contains("api.anthropic.com"));

        // Without it, connections stay pinned to the chosen origins.
        let pinned = HtmlOptions {
            ai: AiOptions {
                providers: vec![AiProvider::OpenAi],
                ..Default::default()
            },
            ..Default::default()
        };
        let html = render(&doc, &pinned).unwrap();
        assert!(html.contains("connect-src https://api.openai.com;"));
        assert!(!html.contains("connect-src https: http:"));
    }

    #[test]
    fn custom_ai_provider_parsing() {
        assert_eq!(
            extract_origin("https://api.deepseek.com/v1/chat/completions"),
            Some("https://api.deepseek.com".into())
        );
        assert_eq!(
            extract_origin("http://localhost:11434"),
            Some("http://localhost:11434".into())
        );
        assert_eq!(
            extract_origin("http://127.0.0.1:8080/v1"),
            Some("http://127.0.0.1:8080".into())
        );
        assert_eq!(extract_origin("not-a-url"), None);
        assert_eq!(extract_origin("ftp://example.com"), None);

        let custom1 = CustomProvider::from_spec("https://api.deepseek.com/v1").unwrap();
        assert_eq!(custom1.slug, "custom");
        assert_eq!(custom1.origin, "https://api.deepseek.com");
        assert_eq!(custom1.endpoint, Some("https://api.deepseek.com/v1".into()));

        let custom2 = CustomProvider::from_spec("ollama=http://localhost:11434").unwrap();
        assert_eq!(custom2.slug, "ollama");
        assert_eq!(custom2.origin, "http://localhost:11434");
        assert_eq!(custom2.endpoint, Some("http://localhost:11434".into()));

        assert!(CustomProvider::from_spec("").is_err());
        assert!(CustomProvider::from_spec("bad slug=http://localhost").is_err());
        assert!(CustomProvider::from_spec("foo=not-a-url").is_err());
    }

    #[test]
    fn custom_ai_provider_embedding_and_csp() {
        let doc = analyze(SAMPLE, &ScoringConfig::default());

        // Custom provider alone
        let options = HtmlOptions {
            ai: AiOptions {
                providers: vec![],
                custom_providers: vec![
                    CustomProvider::from_spec("ollama=http://localhost:11434/v1").unwrap(),
                ],
            },
            ..Default::default()
        };
        let html = render(&doc, &options).unwrap();
        assert!(html.contains("connect-src http://localhost:11434;"));
        assert!(html.contains(r#"data-mx-ai="ollama""#));
        assert!(html.contains(r#"data-mx-ai-endpoints="ollama=http://localhost:11434/v1""#));
        assert!(html.contains(r#"id="mx-ai""#));
        assert!(!html.contains("api.anthropic.com"));

        // Custom provider combined with built-in
        let options_combined = HtmlOptions {
            ai: AiOptions {
                providers: vec![AiProvider::OpenAi],
                custom_providers: vec![
                    CustomProvider::from_spec("https://api.deepseek.com/v1").unwrap(),
                ],
            },
            ..Default::default()
        };
        let html_combined = render(&doc, &options_combined).unwrap();
        assert!(
            html_combined.contains("connect-src https://api.openai.com https://api.deepseek.com;")
        );
        assert!(html_combined.contains(r#"data-mx-ai="openai custom""#));
        assert!(
            html_combined.contains(r#"data-mx-ai-endpoints="custom=https://api.deepseek.com/v1""#)
        );
    }

    #[test]
    fn policy_allows_exactly_the_inlined_scripts() {
        let html = render_sample(MathAssets::Inline(KatexAssets {
            css: ".katex{}".into(),
            js: "window.katex={render:function(){}};".into(),
        }));
        let policy_at = html.find("Content-Security-Policy").expect("CSP meta tag");
        assert!(
            policy_at < html.find("<script").unwrap(),
            "policy precedes every script"
        );
        assert!(policy_at < html.find("<style").unwrap());

        // Every inline script in the file is listed by hash, and nothing else is.
        let mut expected = 0;
        let mut rest = html.as_str();
        while let Some(open) = rest.find("<script") {
            let body_start = open + rest[open..].find('>').unwrap() + 1;
            let body_end = body_start + rest[body_start..].find("</script>").unwrap();
            let hash = base64(&sha256(&rest.as_bytes()[body_start..body_end]));
            assert!(html.contains(&format!("'sha256-{hash}'")), "missing hash");
            expected += 1;
            rest = &rest[body_end..];
        }
        assert_eq!(expected, 2);
        assert_eq!(html.matches("'sha256-").count(), 2);
        assert!(!html.contains("'unsafe-eval'"));
        assert!(html.contains("script-src 'sha256-"));
        assert!(html.contains("base-uri 'none'"));

        let cdn = render_sample(MathAssets::Cdn {
            version: "0.16.22".into(),
        });
        assert!(cdn.contains("https://cdn.jsdelivr.net; style-src"));
    }

    #[test]
    fn raw_html_in_the_source_cannot_inject_markup() {
        let doc = analyze(
            "# T\n\n<script>alert(1)</script>\n\n**Theorem 1.** <img src=x onerror=alert(2)> [x](javascript:alert(3))\n",
            &ScoringConfig::default(),
        );
        let html = render(&doc, &HtmlOptions::default()).unwrap();
        let body = &html[html.find("<main").unwrap()..html.find("</main>").unwrap()];
        assert!(!body.contains("<script") && !body.contains("<img"));
        assert!(!body.contains("javascript:"));
        assert!(body.contains("&lt;script&gt;alert(1)"));
    }

    #[test]
    fn proofs_point_at_their_statement() {
        let html = render_sample(MathAssets::None);
        assert!(html.contains(
            r#"data-block-id="block-proof-1" data-block-type="proof" dir="rtl" data-proves="block-theorem-1""#
        ));
        assert!(
            html.contains(r##"<a class="mx-proves" href="#block-theorem-1">قضیه ۱ (رول)</a>"##)
        );
        assert_eq!(html.matches("data-proves=").count(), 1);
    }

    #[test]
    fn embedded_fonts_lead_the_stack() {
        let doc = analyze(SAMPLE, &ScoringConfig::default());
        let options = HtmlOptions {
            fonts: vec![EmbeddedFont {
                family: "Vazirmatn\";}x{".into(),
                weight: "100 900".into(),
                woff2: b"foo".to_vec(),
            }],
            ..Default::default()
        };
        let html = render(&doc, &options).unwrap();
        assert!(html.contains(
            "@font-face{font-family:\"Vazirmatnx\";src:url(data:font/woff2;base64,Zm9v) format(\"woff2\");font-weight:100 900;font-display:swap}"
        ));
        assert!(html.contains(":root{--mx-font-embedded:\"Vazirmatnx\", }"));
        assert!(!render_sample(MathAssets::None).contains("--mx-font-embedded:\""));
    }

    #[test]
    fn fonts_are_inlined_as_woff2_only() {
        let css = "@font-face{font-family:KaTeX_Main;src:url(fonts/A.woff2) format(\"woff2\"),\
                   url(fonts/A.woff) format(\"woff\"),url(fonts/A.ttf) format(\"truetype\");font-weight:400}.k{src:none}";
        let mut asked = Vec::new();
        let out = inline_fonts(css, |path| {
            asked.push(path.to_string());
            Ok(b"hi!?".to_vec())
        })
        .unwrap();
        assert_eq!(asked, ["fonts/A.woff2"]);
        assert_eq!(
            out,
            "@font-face{font-family:KaTeX_Main;src:url(data:font/woff2;base64,aGkhPw==) format(\"woff2\");font-weight:400}.k{src:none}"
        );
    }

    #[test]
    fn base64_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn inline_build_contains_no_http_urls() {
        let sample = "# آنالیز\n\n**قضیه ۱ (رول).** اگر $f(a)=f(b)$ آنگاه $f'(c)=0$.\n\n$$\\int_a^b f(x) dx = 0$$\n";
        let doc = analyze(sample, &ScoringConfig::default());

        let fake_assets = KatexAssets {
            css: "@font-face{font-family:KaTeX_Main;src:url(data:font/woff2;base64,dGVzdA==) format(\"woff2\");}\n.katex{font-size:1em;}".into(),
            js: "window.katex={render:function(){}};".into(),
        };

        let html = render(
            &doc,
            &HtmlOptions {
                math: MathAssets::Inline(fake_assets),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(
            !html.contains("http://"),
            "Inline build should contain no http:// URLs"
        );
        assert!(
            !html.contains("https://"),
            "Inline build should contain no https:// URLs"
        );
        assert!(html.contains("default-src 'none'"));
        assert!(html.contains("mx-katex-css"));
        assert!(html.contains("mx-katex-js"));
    }

    #[test]
    fn vendored_from_dir_build_contains_no_http_urls() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let vendored_dir = manifest_dir.join("assets/katex");
        if vendored_dir.join("katex.min.css").is_file() {
            let assets = KatexAssets::from_dir(&vendored_dir)
                .expect("failed to load vendored KaTeX from dir");
            let doc = analyze("# آنالیز\n\n$f(x) = x^2$", &ScoringConfig::default());
            let html = render(
                &doc,
                &HtmlOptions {
                    math: MathAssets::Inline(assets),
                    ..Default::default()
                },
            )
            .unwrap();

            // KaTeX JS contains W3C XML namespace strings (e.g. xmlns="http://www.w3.org/2000/svg").
            // We ensure there are zero external HTTP/HTTPS network URLs.
            assert!(!html.contains("https://"), "No external HTTPS URLs allowed");
            assert!(!html.contains("cdn.jsdelivr.net"), "Must not depend on CDN");
            assert!(
                !html.contains("src=\"http"),
                "Must not load external scripts"
            );
            assert!(
                !html.contains("href=\"http"),
                "Must not link external stylesheets"
            );
            let non_xmlns_http = html
                .match_indices("http://")
                .filter(|&(idx, _)| !html[idx..].starts_with("http://www.w3.org/"))
                .count();
            assert_eq!(non_xmlns_http, 0, "No external HTTP URLs allowed");
            assert!(html.contains("KaTeX_Main"));
        }
    }

    #[cfg(feature = "bundled-katex")]
    #[test]
    fn bundled_katex_build_contains_no_http_urls() {
        let doc = analyze("# آنالیز\n\n$f(x) = x^2$", &ScoringConfig::default());
        let html = render(
            &doc,
            &HtmlOptions {
                math: MathAssets::bundled(),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(!html.contains("https://"), "No external HTTPS URLs allowed");
        assert!(!html.contains("cdn.jsdelivr.net"), "Must not depend on CDN");
        assert!(
            !html.contains("src=\"http"),
            "Must not load external scripts"
        );
        assert!(
            !html.contains("href=\"http"),
            "Must not link external stylesheets"
        );
        let non_xmlns_http = html
            .match_indices("http://")
            .filter(|&(idx, _)| !html[idx..].starts_with("http://www.w3.org/"))
            .count();
        assert_eq!(non_xmlns_http, 0, "No external HTTP URLs allowed");
        assert!(html.contains("KaTeX_Main"));
    }
}
