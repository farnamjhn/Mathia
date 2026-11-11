//! [`Document`] → one self-contained HTML5 file.

use std::path::Path;

use askama::Template;

use crate::error::{Error, Result};
use crate::model::{BlockType, Document};

const STYLE_CSS: &str = include_str!("../assets/style.css");
const RUNTIME_JS: &str = include_str!("../assets/runtime.js");

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

#[derive(Debug, Clone, Default)]
pub struct HtmlOptions {
    pub math: MathAssets,
    /// Overrides the title taken from the document's first heading.
    pub title: Option<String>,
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
    css: &'a str,
    runtime_js: &'a str,
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

    let template = DocumentTemplate {
        title,
        lang: if rtl { "fa" } else { "en" },
        dir: doc.direction.as_str(),
        toc_label: if rtl { "فهرست" } else { "Contents" },
        css: STYLE_CSS,
        runtime_js: RUNTIME_JS,
        math_head: math_head(&options.math),
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
                html: &b.html,
            })
            .collect(),
    };
    Ok(template.render()?)
}

fn math_head(math: &MathAssets) -> String {
    match math {
        MathAssets::Inline(katex) => format!(
            "<style id=\"mx-katex-css\">{}</style>\n<script id=\"mx-katex-js\">{}</script>",
            // Keep embedded sources from closing their own element.
            katex.css.replace("</style", "<\\/style"),
            katex.js.replace("</script", "<\\/script"),
        ),
        MathAssets::Cdn { version } => {
            let version: String = version
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '.')
                .collect();
            format!(
                "<link rel=\"stylesheet\" href=\"https://cdn.jsdelivr.net/npm/katex@{version}/dist/katex.min.css\">\n\
                 <script src=\"https://cdn.jsdelivr.net/npm/katex@{version}/dist/katex.min.js\"></script>"
            )
        }
        MathAssets::None => String::new(),
    }
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
        render(&doc, &HtmlOptions { math, title: None }).unwrap()
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
                title: None,
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
                    title: None,
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
                title: None,
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
