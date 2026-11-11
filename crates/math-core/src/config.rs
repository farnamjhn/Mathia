//! Everything tunable about classification lives here as plain data, so it can
//! be loaded from a file or edited in the UI without touching the scorer.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::BlockType;

/// Current version of the [`ScoringConfig`] file format. Files written by a
/// newer Mathia (a higher number) are rejected rather than half-understood.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScoringConfig {
    /// Version of this file format; see [`CONFIG_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// A node opens a block when its best type score reaches this value.
    pub threshold: f32,
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
}

/// Feature weights. Each feature is in `[0, 1]`; the weighted sum is the score.
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

impl Default for Weights {
    fn default() -> Self {
        Weights {
            lead: 0.45,
            label: 0.20,
            emphasis: 0.15,
            mention: 0.10,
            qed: 0.30,
            logic: 0.15,
            math: 0.10,
            definitional: 0.25,
            cue: 0.10,
            after_statement: 0.25,
        }
    }
}

impl Default for ScopeConfig {
    fn default() -> Self {
        ScopeConfig {
            statement_max_nodes: 6,
            proof_max_nodes: 12,
        }
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

impl Default for ScoringConfig {
    fn default() -> Self {
        let rule = |kind, words: &[&str]| KeywordRule {
            kind,
            words: strings(words),
        };
        ScoringConfig {
            schema_version: CONFIG_SCHEMA_VERSION,
            threshold: 0.5,
            weights: Weights::default(),
            scope: ScopeConfig::default(),
            keywords: vec![
                rule(BlockType::Theorem, &["قضیه", "theorem", "thm"]),
                rule(BlockType::Lemma, &["لم", "lemma"]),
                rule(BlockType::Proposition, &["گزاره", "proposition", "prop"]),
                rule(BlockType::Corollary, &["نتیجه", "corollary", "cor"]),
                rule(
                    BlockType::Definition,
                    &["تعریف", "definition", "defn", "def"],
                ),
                rule(BlockType::Proof, &["برهان", "اثبات", "proof"]),
                rule(BlockType::Example, &["مثال", "example"]),
                rule(
                    BlockType::Remark,
                    &["تذکر", "نکته", "توجه", "remark", "note"],
                ),
                rule(
                    BlockType::Exercise,
                    &["تمرین", "مسئله", "مساله", "exercise", "problem"],
                ),
                rule(BlockType::Solution, &["حل", "پاسخ", "راه حل", "solution"]),
                rule(BlockType::Claim, &["ادعا", "claim"]),
                rule(BlockType::Notation, &["نمادگذاری", "نماد", "notation"]),
            ],
            logic_symbols: strings(&[
                "\\implies",
                "\\impliedby",
                "\\Rightarrow",
                "\\Longrightarrow",
                "\\iff",
                "\\Leftrightarrow",
                "\\Longleftrightarrow",
                "\\forall",
                "\\exists",
                "\\nexists",
                "\\therefore",
                "\\because",
                "⇒",
                "⟹",
                "⇔",
                "∀",
                "∃",
                "∴",
            ]),
            qed_markers: strings(&[
                "\\blacksquare",
                "\\square",
                "\\Box",
                "\\qed",
                "∎",
                "■",
                "□",
                "q.e.d",
                "qed",
                "حکم ثابت شد",
                "اثبات کامل است",
                "برهان کامل است",
                "اثبات کامل می‌شود",
                "برهان کامل می‌شود",
            ]),
            definitional_cues: strings(&[
                "گوییم",
                "می‌گوییم",
                "می‌نامیم",
                "نامیده می‌شود",
                "تعریف می‌کنیم",
                "تعریف می‌شود",
                "عبارت است از",
                "is called",
                "is said to be",
                "are called",
                "we say",
                "we call",
                "we define",
                "is defined",
                ":=",
                "\\coloneqq",
                "\\triangleq",
                "\\overset{\\text{def}}",
            ]),
            proof_cues: strings(&[
                "فرض کنید",
                "فرض کنیم",
                "برهان خلف",
                "تناقض",
                "استقرا",
                "بنابراین",
                "suppose",
                "assume",
                "contradiction",
                "induction",
                "without loss of generality",
                "it follows",
                "hence",
                "therefore",
            ]),
            statement_cues: strings(&[
                "آنگاه",
                "اگر و تنها اگر",
                "به ازای هر",
                "برای هر",
                "وجود دارد",
                "if and only if",
                "for all",
                "for every",
                "for any",
                "there exists",
                "then",
            ]),
            continuation_cues: strings(&[
                "که در آن",
                "که",
                "آنگاه",
                "به‌طوری‌که",
                "به طوری که",
                "و در نتیجه",
                "where",
                "such that",
                "then",
                "in which",
            ]),
            reference_cues: strings(&[
                "نتیجه می‌دهد",
                "نشان می‌دهد",
                "بیان می‌کند",
                "ایجاب می‌کند",
                "تضمین می‌کند",
                "می‌گوید",
                "را",
                "implies",
                "shows",
                "states",
                "says",
                "gives",
                "yields",
                "tells",
                "guarantees",
                "is",
                "was",
                "can",
                "does",
            ]),
        }
    }
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

impl Weights {
    fn named(&self) -> [(&'static str, f32); 10] {
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
        assert!(toml.contains("schema_version = 1"));
        assert!(toml.contains("threshold = 0.5"));
        assert!(
            toml.contains("lead = 0.45"),
            "weights should print as written:\n{toml}"
        );
    }

    #[test]
    fn partial_files_keep_defaults() {
        let config = ScoringConfig::parse(
            "threshold = 0.6\n[weights]\nlabel = 0.3\n[scope]\nproof_max_nodes = 20\n",
            ConfigFormat::Toml,
        )
        .unwrap();
        assert_eq!(config.threshold, 0.6);
        assert_eq!(config.weights.label, 0.3);
        assert_eq!(config.weights.lead, Weights::default().lead);
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
        assert!(message("threshold = 1.5").contains("threshold must be in [0, 1]"));
        assert!(message("[weights]\nqed = -0.1").contains("weights.qed"));
        assert!(message("keywords = []").contains("keywords must not be empty"));
        assert!(message("schema_version = 99").contains("schema_version 99"));
        assert!(message("treshold = 0.4").contains("unknown field"));
        assert!(message("[scope]\nproof_max_nodes = 0").contains("at least 1"));
        assert!(
            message("[[keywords]]\nkind = \"heading\"\nwords = [\"x\"]")
                .contains("not a block type")
        );
        assert!(message("[[keywords]]\nkind = \"lemma\"\nwords = [\" \"]").contains("non-empty"));
    }

    #[test]
    fn files_are_loaded_by_extension() {
        let dir = std::env::temp_dir().join(format!("mathia-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = ScoringConfig {
            threshold: 0.55,
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
