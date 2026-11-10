//! Placeholder for the Tauri v2 shell (PLAN.md, Phase 3). Until then this is
//! a thin CLI over `math-core`, handy for trying the compiler on real files:
//!
//! ```text
//! mathia notes.md [-o notes.html] [--katex-dir path/to/katex/dist] [--no-math]
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use math_core::{HtmlOptions, KatexAssets, MathAssets, ScoringConfig};

const KATEX_CDN_VERSION: &str = "0.16.22";

fn run() -> Result<(), String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut katex_dir: Option<PathBuf> = None;
    let mut no_math = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--output" => output = Some(args.next().ok_or("-o needs a path")?.into()),
            "--katex-dir" => katex_dir = Some(args.next().ok_or("--katex-dir needs a path")?.into()),
            "--no-math" => no_math = true,
            "-h" | "--help" => {
                println!("usage: mathia <input.md> [-o out.html] [--katex-dir DIR] [--no-math]");
                return Ok(());
            }
            _ if input.is_none() => input = Some(arg.into()),
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    let input = input.ok_or("usage: mathia <input.md> [-o out.html] [--katex-dir DIR] [--no-math]")?;
    let output = output.unwrap_or_else(|| input.with_extension("html"));

    let math = match (no_math, katex_dir) {
        (true, _) => MathAssets::None,
        (false, Some(dir)) => MathAssets::Inline(KatexAssets::from_dir(dir).map_err(|e| e.to_string())?),
        (false, None) => {
            eprintln!("note: no --katex-dir given; the output loads KaTeX from a CDN and needs a network");
            MathAssets::Cdn { version: KATEX_CDN_VERSION.into() }
        }
    };

    let markdown = std::fs::read_to_string(&input).map_err(|e| format!("{}: {e}", input.display()))?;
    let document = math_core::analyze(&markdown, &ScoringConfig::default());
    let html = math_core::html::render(&document, &HtmlOptions { math, title: None })
        .map_err(|e| e.to_string())?;
    std::fs::write(&output, html).map_err(|e| format!("{}: {e}", output.display()))?;

    let semantic = document.blocks.iter().filter(|b| b.kind.is_semantic()).count();
    println!("{} -> {} ({semantic} semantic blocks)", input.display(), output.display());
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
