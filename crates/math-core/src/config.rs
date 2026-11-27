//! Everything tunable about classification lives here as plain data, so it can
//! be loaded from a file or edited in the UI without touching the scorer.
//!
//! The built-in defaults live in `assets/default_scoring.toml` rather than in
//! Rust literals. [`ScoringConfig::default`] reads that file at runtime (see
//! [`DEFAULT_CONFIG_ENV`] for the lookup order), so changing a default does not
//! require a recompile.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::BlockType;

/// Current version of the [`ScoringConfig`] file format. Files written by a
/// newer Mathia (a higher number) are rejected rather than half-understood.
pub const CONFIG_SCHEMA_VERSION: u32 = 3;

/// Environment variable naming a TOML file that replaces the built-in defaults.
/// When unset, `default_scoring.toml` next to the executable is used, then the
/// copy in the source tree (debug builds), then the embedded copy.
pub const DEFAULT_CONFIG_ENV: &str = "MATHIA_DEFAULT_CONFIG";

/// Compile-time copy of `assets/default_scoring.toml`, used as the last-resort
/// fallback so a build always has working defaults.
const EMBEDDED_DEFAULT_TOML: &str = include_str!("../assets/default_scoring.toml");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScoringConfig {
    /// Version of this file format; see [`CONFIG_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Which block-detection model to use.
    pub tagger: TaggerModel,
    /// A weighted-sum node opens a block when its best type score reaches this.
    pub threshold: f32,
    /// Feature weights for the weighted-sum tagger.
    pub weights: Weights,
    pub scope: ScopeConfig,
    pub keywords: Vec<KeywordRule>,
    /// Counted case-sensitively (`\Rightarrow` is not `\rightarrow`).
    pub logic_symbols: Vec<String>,
    /// Looked for at the very end of a node.
    pub qed_markers: Vec<String>,
    pub definitional_cues: Vec<String>,
    pub proof_cues: Vec<String>,
    pub statement_cues: Vec<String>,
    /// A paragraph starting with one of these continues the statement above it.
    pub continuation_cues: Vec<String>,
    /// Directly after a label these mean the sentence refers to the block
    /// rather than opening it ("Theorem 2 implies…").
    pub reference_cues: Vec<String>,
    /// Margin below which a CRF-tagged node is considered ambiguous and escalated to the judge.
    pub ml_confidence_threshold: f32,
    /// Whether to log local override telemetry for CRF retraining (opt-in).
    pub ml_telemetry: bool,
}

/// Which sequence tagger computes block boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaggerModel {
    /// Pure-Rust Linear-Chain CRF decoded with Viterbi over embedded weights.
    Crf,
    /// Linear weighted sum of the static features, with greedy structural scoping.
    #[default]
    #[serde(
        alias = "linear",
        alias = "heuristic",
        alias = "linear_tree",
        alias = "weighted"
    )]
    WeightedSum,
}

/// Feature weights for [`TaggerModel::WeightedSum`]. Each feature is in
/// `[0, 1]`; the weighted sum is the score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Weights {
    /// The node starts with a keyword of the type.
    pub lead: f32,
    /// …followed by a number, a `(name)`, or `.`/`:` — i.e. it reads as a label.
    pub label: f32,
    /// …and the node starts with bold/italic text.
    pub emphasis: f32,
    /// The keyword only appears somewhere inside the node.
    pub mention: f32,
    /// QED / Halmos ending (proofs).
    pub qed: f32,
    /// Density of implication and quantifier symbols.
    pub logic: f32,
    /// Share of the node that is LaTeX.
    pub math: f32,
    /// "is called", "گوییم", `:=` … (definitions).
    pub definitional: f32,
    /// Type-specific phrasing ("suppose", "فرض کنید", "if … then", "آنگاه").
    pub cue: f32,
    /// An unlabelled paragraph directly after a statement (keyword-less proofs).
    pub after_statement: f32,
}

/// Default scope limits, counted in top-level Markdown nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScopeConfig {
    /// Statements, definitions, examples and remarks.
    pub statement_max_nodes: usize,
    /// A proof with no QED marker stops after this many nodes.
    pub proof_max_nodes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeywordRule {
    pub kind: BlockType,
    pub words: Vec<String>,
}

impl Weights {
    pub fn named(&self) -> [(&'static str, f32); 10] {
        [
            ("lead", self.lead),
            ("label", self.label),
            ("emphasis", self.emphasis),
            ("mention", self.mention),
            ("qed", self.qed),
            ("logic", self.logic),
            ("math", self.math),
            ("definitional", self.definitional),
            ("cue", self.cue),
            ("after_statement", self.after_statement),
        ]
    }
}

impl Default for Weights {
    fn default() -> Self {
        builtin_defaults().weights.clone()
    }
}

impl Default for ScopeConfig {
    fn default() -> Self {
        builtin_defaults().scope.clone()
    }
}

impl Default for ScoringConfig {
    fn default() -> Self {
        builtin_defaults().clone()
    }
}

/// Fully-specified shape of the defaults file. Unlike [`ScoringConfig`] it has
/// no serde defaults, so parsing it can never recurse back into `Default`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultsFile {
    schema_version: u32,
    tagger: TaggerModel,
    threshold: f32,
    weights: DefaultsWeights,
    scope: DefaultsScope,
    keywords: Vec<KeywordRule>,
    logic_symbols: Vec<String>,
    qed_markers: Vec<String>,
    definitional_cues: Vec<String>,
    proof_cues: Vec<String>,
    statement_cues: Vec<String>,
    continuation_cues: Vec<String>,
    reference_cues: Vec<String>,
    ml_confidence_threshold: f32,
    ml_telemetry: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultsWeights {
    lead: f32,
    label: f32,
    emphasis: f32,
    mention: f32,
    qed: f32,
    logic: f32,
    math: f32,
    definitional: f32,
    cue: f32,
    after_statement: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultsScope {
    statement_max_nodes: usize,
    proof_max_nodes: usize,
}

impl DefaultsFile {
    fn into_config(self) -> ScoringConfig {
        ScoringConfig {
            schema_version: self.schema_version,
            tagger: self.tagger,
            threshold: self.threshold,
            weights: Weights {
                lead: self.weights.lead,
                label: self.weights.label,
                emphasis: self.weights.emphasis,
                mention: self.weights.mention,
                qed: self.weights.qed,
                logic: self.weights.logic,
                math: self.weights.math,
                definitional: self.weights.definitional,
                cue: self.weights.cue,
                after_statement: self.weights.after_statement,
            },
            scope: ScopeConfig {
                statement_max_nodes: self.scope.statement_max_nodes,
                proof_max_nodes: self.scope.proof_max_nodes,
            },
            keywords: self.keywords,
            logic_symbols: self.logic_symbols,
            qed_markers: self.qed_markers,
            definitional_cues: self.definitional_cues,
            proof_cues: self.proof_cues,
            statement_cues: self.statement_cues,
            continuation_cues: self.continuation_cues,
            reference_cues: self.reference_cues,
            ml_confidence_threshold: self.ml_confidence_threshold,
            ml_telemetry: self.ml_telemetry,
        }
    }
}

/// The built-in defaults, loaded once and cached.
fn builtin_defaults() -> &'static ScoringConfig {
    static DEFAULTS: OnceLock<ScoringConfig> = OnceLock::new();
    DEFAULTS.get_or_init(|| runtime_defaults().unwrap_or_else(embedded_defaults))
}

/// Where to look for a defaults file, in priority order.
fn defaults_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(DEFAULT_CONFIG_ENV) {
        return Some(PathBuf::from(path));
    }
    #[cfg(debug_assertions)]
    {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/default_scoring.toml");
        if source.is_file() {
            return Some(source);
        }
    }
    let beside_exe = std::env::current_exe()
        .ok()?
        .parent()?
        .join("default_scoring.toml");
    beside_exe.is_file().then_some(beside_exe)
}

/// Loads a defaults file from [`defaults_path`], or `None` if there is none or
/// it is unreadable or invalid (in which case the embedded copy takes over).
fn runtime_defaults() -> Option<ScoringConfig> {
    let path = defaults_path()?;
    let loaded = (|| {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let file: DefaultsFile = toml::from_str(&text).map_err(|e| e.to_string())?;
        let config = file.into_config();
        config.validate().map_err(|e| e.to_string())?;
        Ok::<_, String>(config)
    })();
    match loaded {
        Ok(config) => Some(config),
        Err(message) => {
            eprintln!(
                "mathia: ignoring default scoring config {}: {message}; using the built-in defaults",
                path.display()
            );
            None
        }
    }
}

fn embedded_defaults() -> ScoringConfig {
    let file: DefaultsFile =
        toml::from_str(EMBEDDED_DEFAULT_TOML).expect("embedded default scoring config is valid");
    let config = file.into_config();
    config
        .validate()
        .expect("embedded default scoring config is valid");
    config
}

/// File formats a [`ScoringConfig`] can be stored in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Toml,
    Json,
}

impl ConfigFormat {
    /// Chosen by file extension: `.toml` or `.json`.
    pub fn from_path(path: &Path) -> Result<Self> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("toml") => Ok(ConfigFormat::Toml),
            Some("json") => Ok(ConfigFormat::Json),
            _ => Err(Error::Config(format!(
                "{}: expected a .toml or .json file",
                path.display()
            ))),
        }
    }
}

impl ScoringConfig {
    /// Checks that the values make sense. Parsing a file always validates;
    /// call this after building or editing a config in code.
    pub fn validate(&self) -> Result<()> {
        let fail = |message: String| Err(Error::Config(message));
        if self.schema_version == 0 || self.schema_version > CONFIG_SCHEMA_VERSION {
            return fail(format!(
                "schema_version {} is not supported (this build reads versions 1 to {CONFIG_SCHEMA_VERSION})",
                self.schema_version
            ));
        }
        if self.ml_confidence_threshold < 0.0 {
            return fail(format!(
                "ml_confidence_threshold must be non-negative, got {}",
                self.ml_confidence_threshold
            ));
        }
        if !(0.0..=1.0).contains(&self.threshold) {
            return fail(format!(
                "threshold must be in [0, 1], got {}",
                self.threshold
            ));
        }
        for (name, weight) in self.weights.named() {
            if !(0.0..=1.0).contains(&weight) {
                return fail(format!("weights.{name} must be in [0, 1], got {weight}"));
            }
        }
        if self.scope.statement_max_nodes == 0 || self.scope.proof_max_nodes == 0 {
            return fail("scope limits must be at least 1".into());
        }
        if self.keywords.is_empty() {
            return fail("keywords must not be empty".into());
        }
        for rule in &self.keywords {
            if !rule.kind.is_semantic() {
                return fail(format!(
                    "keywords: `{}` is not a block type",
                    rule.kind.slug()
                ));
            }
            if rule.words.is_empty() || rule.words.iter().any(|w| w.trim().is_empty()) {
                return fail(format!(
                    "keywords for `{}` must be a non-empty list of non-empty words",
                    rule.kind.slug()
                ));
            }
        }
        Ok(())
    }

    /// Parses and validates. Fields missing from the file keep their defaults.
    pub fn parse(text: &str, format: ConfigFormat) -> Result<Self> {
        let config: ScoringConfig = match format {
            ConfigFormat::Toml => toml::from_str(text).map_err(|e| Error::Config(e.to_string()))?,
            ConfigFormat::Json => {
                serde_json::from_str(text).map_err(|e| Error::Config(e.to_string()))?
            }
        };
        config.validate()?;
        Ok(config)
    }

    pub fn serialize(&self, format: ConfigFormat) -> Result<String> {
        self.validate()?;
        match format {
            ConfigFormat::Toml => {
                toml::to_string_pretty(self).map_err(|e| Error::Config(e.to_string()))
            }
            ConfigFormat::Json => serde_json::to_string_pretty(self)
                .map(|mut json| {
                    json.push('\n');
                    json
                })
                .map_err(|e| Error::Config(e.to_string())),
        }
    }

    /// Loads a `.toml` or `.json` file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let format = ConfigFormat::from_path(path)?;
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text, format).map_err(|e| Error::Config(format!("{}: {e}", path.display())))
    }

    /// Writes a `.toml` or `.json` file.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let text = self.serialize(ConfigFormat::from_path(path)?)?;
        std::fs::write(path, text).map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid_and_round_trips() {
        let config = ScoringConfig::default();
        config.validate().unwrap();
        for format in [ConfigFormat::Toml, ConfigFormat::Json] {
            let text = config.serialize(format).unwrap();
            assert_eq!(
                ScoringConfig::parse(&text, format).unwrap(),
                config,
                "{text}"
            );
        }
        let toml = config.serialize(ConfigFormat::Toml).unwrap();
        assert!(toml.contains("schema_version = 3"));
        assert!(toml.contains("tagger = \"weighted_sum\""));
        assert!(toml.contains("threshold = 0.5"));
        assert!(toml.contains("[weights]"));
        assert!(toml.contains("[scope]"));
        assert!(toml.contains("statement_max_nodes = 6"));
    }

    #[test]
    fn builtin_defaults_come_from_the_toml_file() {
        let config = ScoringConfig::default();
        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(config, embedded_defaults());

        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/default_scoring.toml");
        let text = std::fs::read_to_string(&source).unwrap();
        assert_eq!(
            ScoringConfig::parse(&text, ConfigFormat::Toml).unwrap(),
            config
        );
    }

    #[test]
    fn partial_files_keep_defaults() {
        let config =
            ScoringConfig::parse("[scope]\nproof_max_nodes = 20\n", ConfigFormat::Toml).unwrap();
        assert_eq!(config.scope.proof_max_nodes, 20);
        assert_eq!(config.keywords, ScoringConfig::default().keywords);

        let json = r#"{"keywords": [{"kind": "theorem", "words": ["satz"]}]}"#;
        let config = ScoringConfig::parse(json, ConfigFormat::Json).unwrap();
        assert_eq!(config.keywords.len(), 1);
        assert_eq!(config.keywords[0].kind, BlockType::Theorem);
    }

    #[test]
    fn invalid_values_are_rejected_with_a_reason() {
        let message = |text: &str| {
            ScoringConfig::parse(text, ConfigFormat::Toml)
                .unwrap_err()
                .to_string()
        };
        assert!(message("keywords = []").contains("keywords must not be empty"));
        assert!(message("schema_version = 99").contains("schema_version 99"));
        assert!(message("threshold = 1.5").contains("threshold must be in [0, 1]"));
        assert!(message("[weights]\nqed = -0.1").contains("weights.qed"));
        assert!(message("[scope]\nproof_max_nodes = 0").contains("at least 1"));
        assert!(
            message("[[keywords]]\nkind = \"heading\"\nwords = [\"x\"]")
                .contains("not a block type")
        );
        assert!(message("[[keywords]]\nkind = \"lemma\"\nwords = [\" \"]").contains("non-empty"));
    }

    #[test]
    fn tagger_model_round_trips_and_accepts_old_names() {
        let mut config = ScoringConfig::default();
        config.tagger = TaggerModel::WeightedSum;
        for format in [ConfigFormat::Toml, ConfigFormat::Json] {
            let text = config.serialize(format).unwrap();
            assert_eq!(ScoringConfig::parse(&text, format).unwrap(), config);
        }
        for alias in ["linear", "heuristic", "linear_tree", "weighted"] {
            let parsed =
                ScoringConfig::parse(&format!("tagger = \"{alias}\""), ConfigFormat::Toml).unwrap();
            assert_eq!(parsed.tagger, TaggerModel::WeightedSum);
        }
    }

    #[test]
    fn files_are_loaded_by_extension() {
        let dir = std::env::temp_dir().join(format!("mathia-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = ScoringConfig {
            ml_confidence_threshold: 2.5,
            ..Default::default()
        };
        for name in ["scoring.toml", "scoring.json"] {
            let path = dir.join(name);
            config.save(&path).unwrap();
            assert_eq!(ScoringConfig::load(&path).unwrap(), config);
        }
        assert!(ScoringConfig::load(dir.join("scoring.yaml")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
