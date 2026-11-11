//! Placeholder for the Tauri v2 shell (PLAN.md, Phase 3). Until then this is
//! a thin CLI over `math-core`, handy for trying the compiler on real files:
//!
//! ```text
//! mathia notes.md [-o notes.html] [--config scoring.toml] [--explain] [--font F.woff2]
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use math_core::{ConfigFormat, EmbeddedFont, HtmlOptions, KatexAssets, MathAssets, ScoringConfig};

#[cfg(not(feature = "bundled-katex"))]
const KATEX_CDN_VERSION: &str = "0.16.22";

const USAGE: &str = "usage: mathia <input.md> [options]

  -o, --output FILE     where to write the HTML (default: input with .html)
      --config FILE     scoring configuration (.toml or .json)
      --dump-config     print the default scoring configuration as TOML and exit
      --explain         print every node's scores and why each block ended
      --font FILE       embed a WOFF2 text font (repeatable; family from the file name)
      --title TEXT      document title (default: the first heading)
      --katex-dir DIR   use this KaTeX dist/ directory
      --no-math         leave LaTeX unrendered";

/// `Vazirmatn[wght].woff2` and `Vazirmatn-Regular.woff2` are both "Vazirmatn".
fn font_family(path: &std::path::Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Embedded");
    stem.split(['-', '[', '.'])
        .next()
        .unwrap_or(stem)
        .to_string()
}

fn run() -> Result<(), String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut katex_dir: Option<PathBuf> = None;
    let mut config_path: Option<PathBuf> = None;
    let mut fonts: Vec<PathBuf> = Vec::new();
    let mut title: Option<String> = None;
    let mut no_math = false;
    let mut explain = false;

    let mut args = std::env::args().skip(1);
    let value = |args: &mut dyn Iterator<Item = String>, flag: &str| {
        args.next().ok_or(format!("{flag} needs a value"))
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--output" => output = Some(value(&mut args, &arg)?.into()),
            "--katex-dir" => katex_dir = Some(value(&mut args, &arg)?.into()),
            "--config" => config_path = Some(value(&mut args, &arg)?.into()),
            "--font" => fonts.push(value(&mut args, &arg)?.into()),
            "--title" => title = Some(value(&mut args, &arg)?),
            "--no-math" => no_math = true,
            "--explain" => explain = true,
            "--dump-config" => {
                let text = ScoringConfig::default()
                    .serialize(ConfigFormat::Toml)
                    .map_err(|e| e.to_string())?;
                print!("{text}");
                return Ok(());
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}\n{USAGE}"));
            }
            _ if input.is_none() => input = Some(arg.into()),
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    let input = input.ok_or(USAGE)?;
    let output = output.unwrap_or_else(|| input.with_extension("html"));

    let config = match config_path {
        Some(path) => ScoringConfig::load(path).map_err(|e| e.to_string())?,
        None => ScoringConfig::default(),
    };
    let fonts = fonts
        .iter()
        .map(|path| EmbeddedFont::from_file(path, &font_family(path), "100 900"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let math = match (no_math, katex_dir) {
        (true, _) => MathAssets::None,
        (false, Some(dir)) => {
            MathAssets::Inline(KatexAssets::from_dir(dir).map_err(|e| e.to_string())?)
        }
        #[cfg(feature = "bundled-katex")]
        (false, None) => MathAssets::bundled(),
        #[cfg(not(feature = "bundled-katex"))]
        (false, None) => {
            let vendored =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../crates/math-core/assets/katex");
            if vendored.join("katex.min.css").is_file() {
                MathAssets::Inline(KatexAssets::from_dir(&vendored).map_err(|e| e.to_string())?)
            } else {
                eprintln!(
                    "note: no --katex-dir given; the output loads KaTeX from a CDN and needs a network"
                );
                MathAssets::Cdn {
                    version: KATEX_CDN_VERSION.into(),
                }
            }
        }
    };

    let markdown =
        std::fs::read_to_string(&input).map_err(|e| format!("{}: {e}", input.display()))?;
    let analysis = math_core::analyze_detailed(&markdown, &config);
    if explain {
        print!("{}", analysis.explain(config.threshold));
    }
    let document = analysis.document;
    let html = math_core::html::render(&document, &HtmlOptions { math, title, fonts })
        .map_err(|e| e.to_string())?;
    std::fs::write(&output, html).map_err(|e| format!("{}: {e}", output.display()))?;

    let semantic = document
        .blocks
        .iter()
        .filter(|b| b.kind.is_semantic())
        .count();
    println!(
        "{} -> {} ({semantic} semantic blocks)",
        input.display(),
        output.display()
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}
