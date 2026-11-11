use std::fs;
use std::path::Path;

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

fn inline_fonts(css: &str, base_dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
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
                let bytes = fs::read(base_dir.join(path))?;
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

fn main() {
    println!("cargo:rerun-if-changed=assets/katex/katex.min.js");
    println!("cargo:rerun-if-changed=assets/katex/katex.min.css");
    println!("cargo:rerun-if-changed=assets/katex/katex.prepared.css");

    if std::env::var("CARGO_FEATURE_BUNDLED_KATEX").is_ok() {
        let katex_dir = Path::new("assets/katex");
        let min_css = katex_dir.join("katex.min.css");
        let min_js = katex_dir.join("katex.min.js");
        let prepared_css = katex_dir.join("katex.prepared.css");

        if !min_css.is_file() || !min_js.is_file() {
            panic!(
                "\n\n========================================================================\n\
                 The `bundled-katex` feature requires vendored KaTeX assets in:\n\
                 crates/math-core/assets/katex/\n\n\
                 Please run:\n\
                     ./scripts/vendor-katex.sh\n\
                 ========================================================================\n\n"
            );
        }

        if !prepared_css.is_file() {
            let css = fs::read_to_string(&min_css).expect("failed to read katex.min.css");
            let inlined = inline_fonts(&css, katex_dir)
                .expect("failed to inline woff2 fonts into katex.prepared.css");
            fs::write(&prepared_css, inlined).expect("failed to write katex.prepared.css");
        }
    }
}
