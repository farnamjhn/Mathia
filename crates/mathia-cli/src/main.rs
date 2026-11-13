//! Command-line front end: the desktop app lives in `src-tauri`. This is
//! a thin CLI over `math-core`, handy for trying the compiler on real files:
//!
//! ```text
//! mathia notes.md [-o notes.html] [--config scoring.toml] [--explain] [--font F.woff2] [--ai] [--ai-custom URL]
//! mathia chapters/ [-o site/] [--watch]
//! mathia notes.md --fix [-o notes.fixed.md]
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use math_core::{
    AiOptions, AiProvider, ConfigFormat, CustomProvider, EmbeddedFont, HtmlOptions, KatexAssets,
    MathAssets, ScopeOverrides, ScoringConfig,
};
use math_judge::{CachedJudge, CommandJudge, Policy, Replay};

#[cfg(not(feature = "bundled-katex"))]
const KATEX_CDN_VERSION: &str = "0.16.22";

const USAGE: &str = "usage: mathia <input.md | folder> [options]

  -o, --output FILE     where to write the HTML (default: input with .html);
                        for a folder input, the output folder (default: the input folder)
      --watch           recompile whenever an input file changes (Ctrl+C stops)
      --fix             write repaired Markdown instead of HTML: inline formulas that
                        lost their $ $, \\( \\) delimiters, bare environments, unbalanced $
                        (default output: input with .fixed.md; -o may name the input itself)
      --config FILE     scoring configuration (.toml or .json)
      --dump-config     print the default scoring configuration as TOML and exit
      --explain         print every node's scores and why each block ended
      --font FILE       embed a WOFF2 text font (repeatable; family from the file name);
                        replaces the bundled Vazirmatn
      --no-font         embed no text font (the reader's installed fonts are used)
      --title TEXT      document title (default: the first heading)
      --katex-dir DIR   use this KaTeX dist/ directory
      --no-math         leave LaTeX unrendered
      --ai              enable AI runtime with all providers (incl. openai-compatible: any URL)
      --ai-provider NAME
                        enable a specific AI provider (repeatable: anthropic, openai, gemini, openai-compatible)
      --ai-custom [NAME=]URL
                        enable a custom OpenAI-compatible AI provider (repeatable; e.g.
                        --ai-custom https://api.deepseek.com/v1 or
                        --ai-custom ollama=http://localhost:11434/v1)

decision models (see crates/math-judge):
      --judge CMD       review every block with an external model: CMD gets one JSON
                        request per line on stdin and answers one JSON verdict per line
      --judge-model ID  model name and version, part of every cache key (default: CMD)
      --judge-cache FILE
                        verdict cache (JSON); unchanged blocks are not asked again
      --judge-log FILE  write the overrides and every verdict, for --overrides later
      --judge-rounds N  at most N review rounds (default 4)
      --min-confidence X
                        treat answers below confidence X as accept (default 0.5)
      --overrides FILE  apply recorded corrections (a --judge-log file or bare
                        ScopeOverrides JSON) instead of running a model";

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

/// Everything that is the same for every file of a run.
struct Settings {
    config: ScoringConfig,
    html: HtmlOptions,
    explain: bool,
    judge: Option<JudgeSettings>,
    overrides: Option<ScopeOverrides>,
}

struct JudgeSettings {
    command: String,
    model: String,
    cache: Option<PathBuf>,
    log: Option<PathBuf>,
    rounds: usize,
    policy: Policy,
}

fn run() -> Result<(), String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut katex_dir: Option<PathBuf> = None;
    let mut config_path: Option<PathBuf> = None;
    let mut fonts: Vec<PathBuf> = Vec::new();
    let mut title: Option<String> = None;
    let mut no_math = false;
    let mut no_font = false;
    let mut explain = false;
    let mut watch = false;
    let mut fix = false;
    let mut ai_providers: Vec<AiProvider> = Vec::new();
    let mut custom_providers: Vec<CustomProvider> = Vec::new();
    let mut judge_command: Option<String> = None;
    let mut judge_model: Option<String> = None;
    let mut judge_cache: Option<PathBuf> = None;
    let mut judge_log: Option<PathBuf> = None;
    let mut judge_rounds = 4;
    let mut policy = Policy::default();
    let mut overrides_path: Option<PathBuf> = None;

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
            "--no-font" => no_font = true,
            "--explain" => explain = true,
            "--watch" => watch = true,
            "--fix" => fix = true,
            "--judge" => judge_command = Some(value(&mut args, &arg)?),
            "--judge-model" => judge_model = Some(value(&mut args, &arg)?),
            "--judge-cache" => judge_cache = Some(value(&mut args, &arg)?.into()),
            "--judge-log" => judge_log = Some(value(&mut args, &arg)?.into()),
            "--judge-rounds" => {
                judge_rounds = value(&mut args, &arg)?
                    .parse()
                    .map_err(|e| format!("--judge-rounds: {e}"))?;
            }
            "--min-confidence" => {
                policy.min_confidence = value(&mut args, &arg)?
                    .parse()
                    .map_err(|e| format!("--min-confidence: {e}"))?;
            }
            "--overrides" => overrides_path = Some(value(&mut args, &arg)?.into()),
            "--ai" => {
                for p in AiProvider::ALL {
                    if !ai_providers.contains(&p) {
                        ai_providers.push(p);
                    }
                }
            }
            "--ai-custom" => {
                let spec = value(&mut args, &arg)?;
                let custom = CustomProvider::from_spec(&spec)?;
                if !custom_providers.iter().any(|c| c.slug == custom.slug) {
                    custom_providers.push(custom);
                }
            }
            "--ai-provider" => {
                let name = value(&mut args, &arg)?;
                if name.contains('=') || name.starts_with("http://") || name.starts_with("https://")
                {
                    let custom = CustomProvider::from_spec(&name)?;
                    if !custom_providers.iter().any(|c| c.slug == custom.slug) {
                        custom_providers.push(custom);
                    }
                } else if name.trim().eq_ignore_ascii_case("custom") {
                    return Err(
                        "custom AI provider requires a URL or origin. Use --ai-custom [NAME=]URL (e.g. --ai-custom https://api.deepseek.com/v1)".into(),
                    );
                } else {
                    let provider = AiProvider::from_slug(&name).ok_or_else(|| {
                        let valid = AiProvider::ALL
                            .iter()
                            .map(|p| p.slug())
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!(
                            "unknown AI provider '{name}'. Valid providers: {valid} (or use --ai-custom [NAME=]URL)"
                        )
                    })?;
                    if !ai_providers.contains(&provider) {
                        ai_providers.push(provider);
                    }
                }
            }
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
    if judge_command.is_some() && overrides_path.is_some() {
        return Err("--judge and --overrides exclude each other".into());
    }
    if judge_command.is_none()
        && (judge_cache.is_some() || judge_log.is_some() || judge_model.is_some())
    {
        return Err("--judge-model, --judge-cache and --judge-log need --judge".into());
    }

    if fix {
        if watch || judge_command.is_some() || overrides_path.is_some() {
            return Err(
                "--fix writes Markdown; it does not go with --watch, --judge or --overrides".into(),
            );
        }
        return fix_files(&input, output);
    }

    let config = match config_path {
        Some(path) => ScoringConfig::load(path).map_err(|e| e.to_string())?,
        None => ScoringConfig::default(),
    };
    if no_font && !fonts.is_empty() {
        return Err("--font and --no-font exclude each other".into());
    }
    #[cfg_attr(not(feature = "bundled-fonts"), allow(unused_mut))]
    let mut fonts = fonts
        .iter()
        .map(|path| EmbeddedFont::from_file(path, &font_family(path), "100 900"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    #[cfg(feature = "bundled-fonts")]
    if fonts.is_empty() && !no_font {
        fonts.push(EmbeddedFont::bundled());
    }

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
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../math-core/assets/katex");
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

    let overrides = match overrides_path {
        Some(path) => Some(load_overrides(&path)?),
        None => None,
    };
    let settings = Settings {
        config,
        html: HtmlOptions {
            math,
            title,
            fonts,
            ai: AiOptions {
                providers: ai_providers,
                custom_providers,
            },
        },
        explain,
        judge: judge_command.map(|command| JudgeSettings {
            model: judge_model.unwrap_or_else(|| command.clone()),
            command,
            cache: judge_cache,
            log: judge_log,
            rounds: judge_rounds,
            policy,
        }),
        overrides,
    };

    let jobs = jobs(&input, output)?;
    if settings.judge.as_ref().is_some_and(|j| j.log.is_some()) && jobs.len() > 1 {
        return Err("--judge-log takes one input file, not a folder".into());
    }
    let mut stamps: Vec<Option<(SystemTime, u64)>> = vec![None; jobs.len()];
    loop {
        for (job, stamp) in jobs.iter().zip(stamps.iter_mut()) {
            let now = file_stamp(&job.0);
            if watch && now.is_some() && now == *stamp {
                continue;
            }
            *stamp = now;
            match compile_file(&job.0, &job.1, &settings) {
                Ok(()) => {}
                Err(message) if watch => eprintln!("error: {message}"),
                Err(message) => return Err(message),
            }
        }
        if !watch {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(400));
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        ["md", "markdown", "mdown"]
            .iter()
            .any(|m| e.eq_ignore_ascii_case(m))
    })
}

/// `(input, output)` pairs: one for a file, one per Markdown file (sorted,
/// not recursive) for a folder.
fn jobs(input: &Path, output: Option<PathBuf>) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    if !input.is_dir() {
        let output = output.unwrap_or_else(|| input.with_extension("html"));
        return Ok(vec![(input.to_path_buf(), output)]);
    }
    let out_dir = output.unwrap_or_else(|| input.to_path_buf());
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(input)
        .map_err(|e| format!("{}: {e}", input.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file() && is_markdown(path))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(format!("{}: no Markdown files", input.display()));
    }
    Ok(files
        .into_iter()
        .map(|file| {
            let name = file.with_extension("html");
            let name = name.file_name().expect("a file has a name");
            let out = out_dir.join(name);
            (file, out)
        })
        .collect())
}

/// `notes.md` → `notes.fixed.md`
fn fixed_name(input: &Path) -> PathBuf {
    let extension = input.extension().and_then(|e| e.to_str()).unwrap_or("md");
    input.with_extension(format!("fixed.{extension}"))
}

fn is_fixed_copy(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.ends_with(".fixed"))
}

/// `--fix`: repairs one file, or every Markdown file of a folder.
fn fix_files(input: &Path, output: Option<PathBuf>) -> Result<(), String> {
    let jobs: Vec<(PathBuf, PathBuf)> = if input.is_dir() {
        let out_dir = output.unwrap_or_else(|| input.to_path_buf());
        std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
        let same_folder = out_dir == input;
        let mut files: Vec<PathBuf> = std::fs::read_dir(input)
            .map_err(|e| format!("{}: {e}", input.display()))?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.is_file() && is_markdown(path) && !is_fixed_copy(path))
            .collect();
        files.sort();
        if files.is_empty() {
            return Err(format!("{}: no Markdown files", input.display()));
        }
        files
            .into_iter()
            .map(|file| {
                let named = if same_folder {
                    fixed_name(&file)
                } else {
                    file.clone()
                };
                let out = out_dir.join(named.file_name().expect("a file has a name"));
                (file, out)
            })
            .collect()
    } else {
        let out = output.unwrap_or_else(|| fixed_name(input));
        vec![(input.to_path_buf(), out)]
    };
    for (file, out) in jobs {
        let markdown =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let repair = math_core::repair_markdown(&markdown);
        if !repair.changed {
            println!("{}: nothing to fix", file.display());
            continue;
        }
        std::fs::write(&out, &repair.text).map_err(|e| format!("{}: {e}", out.display()))?;
        println!(
            "{} -> {} ({} inline formulas wrapped)",
            file.display(),
            out.display(),
            repair.formulas
        );
    }
    Ok(())
}

fn file_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// A `--judge-log` file, or a bare `ScopeOverrides` object.
fn load_overrides(path: &Path) -> Result<ScopeOverrides, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Ok(replay) = serde_json::from_str::<Replay>(&text) {
        return Ok(replay.overrides);
    }
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn compile_file(input: &Path, output: &Path, settings: &Settings) -> Result<(), String> {
    let config = &settings.config;
    let markdown =
        std::fs::read_to_string(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let analysis = match (&settings.judge, &settings.overrides) {
        (Some(judge), _) => {
            let overrides = run_judge(&markdown, config, judge)?;
            math_core::analyze_with_overrides(&markdown, config, &overrides)
        }
        (None, Some(overrides)) => {
            let nodes =
                math_core::parser::parse_markdown(&math_core::normalize_markdown(&markdown));
            overrides
                .validate(nodes.len())
                .map_err(|e| format!("{}: {e}", input.display()))?;
            math_core::analyze_with_overrides(&markdown, config, overrides)
        }
        (None, None) => math_core::analyze_detailed(&markdown, config),
    };
    if settings.explain {
        print!("{}", analysis.explain(config.threshold));
    }
    let document = analysis.document;
    let html = math_core::html::render(&document, &settings.html).map_err(|e| e.to_string())?;
    std::fs::write(output, html).map_err(|e| format!("{}: {e}", output.display()))?;

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

/// Runs the model over `markdown` and returns the overrides it settled on.
fn run_judge(
    markdown: &str,
    config: &ScoringConfig,
    settings: &JudgeSettings,
) -> Result<ScopeOverrides, String> {
    let command = CommandJudge::from_command_line(&settings.command, &settings.model)?
        .with_policy(settings.policy);
    let mut judge = CachedJudge::new(command, &settings.model);
    if let Some(path) = &settings.cache {
        judge = judge.load(path)?;
    }
    let (_, overrides, log) = math_core::analyze_with_judge_detailed(
        markdown,
        config,
        &judge,
        settings.rounds,
        ScopeOverrides::default(),
    );
    for error in judge.inner().errors() {
        eprintln!("judge: {error}");
    }
    let changes = log
        .iter()
        .filter(|v| v.applied && v.verdict != math_core::Verdict::Accept)
        .count();
    eprintln!(
        "judge: {} verdicts ({} from the model, {changes} changes)",
        log.len(),
        judge.misses()
    );
    if let Some(path) = &settings.cache {
        judge.save(path)?;
    }
    if let Some(path) = &settings.log {
        Replay::new(&settings.model, overrides.clone(), log).save(path)?;
    }
    Ok(overrides)
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
