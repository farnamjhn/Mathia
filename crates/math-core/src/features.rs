//! Static feature extraction. Pure functions of a [`Node`] and the lexicon —
//! no model, no I/O, no randomness.

use serde::{Deserialize, Serialize};

use crate::config::ScoringConfig;
use crate::model::{BlockType, Node};
use crate::text::{contains_word, fold, lower_char, normalize, tail};

/// How many trailing chars of a node are searched for a QED marker.
const QED_TAIL_CHARS: usize = 80;
/// Logic-symbol count at which the logic feature saturates.
const LOGIC_SATURATION: f32 = 3.0;
/// LaTeX share of a node at which the math feature saturates.
const MATH_SATURATION: f32 = 0.3;

/// A keyword found at the very start of a node, with the label after it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeywordHit {
    pub kind: BlockType,
    /// `۳.۲` in "قضیه ۳.۲ (کوشی)."
    pub number: Option<String>,
    /// `کوشی` in "قضیه ۳.۲ (کوشی)."
    pub name: Option<String>,
    /// The keyword reads as a label: followed by a `(name)`, by `.`/`:`
    /// (possibly after a number), by nothing at all, or (for proofs) by a
    /// reference such as "of Theorem 2". A bare number is not enough:
    /// "Theorem 2 gives a bound" is prose.
    pub labelled: bool,
    /// Nothing but the label on this node; the statement is in the next one.
    pub body_empty: bool,
    /// A verb follows the keyword directly ("قضیه ۲ نتیجه می‌دهد که…",
    /// "Theorem 2 implies…"): the sentence talks *about* the block.
    pub reference: bool,
    /// "Proof of Theorem 2" / "اثبات قضیه ۲": the statement referred to.
    pub refers_to: Option<Reference>,
}

/// A mention of another block by type and (optionally) number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub kind: BlockType,
    pub number: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Features {
    pub lead: Option<KeywordHit>,
    /// Types whose keyword appears anywhere in the node as a whole word.
    pub mentions: Vec<BlockType>,
    pub emphasized: bool,
    /// Share of the node that is LaTeX, scaled to `[0, 1]`.
    pub math_density: f32,
    pub logic_count: usize,
    /// `logic_count` scaled to `[0, 1]`.
    pub logic_density: f32,
    pub qed: bool,
    pub definitional: bool,
    pub proof_cue: bool,
    pub statement_cue: bool,
    /// Starts with "where", "که در آن", … — continues the previous statement.
    pub continuation: bool,
}

/// Type alias for node-level static features.
pub type NodeFeatures = Features;

/// Exported JSON schema for `dump-features`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureDump {
    pub nodes: Vec<DumpNode>,
    pub features: Vec<NodeFeatures>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio_tags: Option<Vec<String>>,
}

/// A node representation serialized during `dump-features`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DumpNode {
    pub index: usize,
    pub kind: String,
    pub flat_text: String,
    pub char_len: usize,
    pub env_span: Option<crate::model::EnvSpan>,
}

/// Dumps AST nodes and extracted static features for an input Markdown document.
pub fn dump_features(markdown: &str, config: &ScoringConfig) -> FeatureDump {
    dump_features_with_overrides(markdown, config, None)
}

/// Dumps AST nodes and extracted static features for an HTML document.
///
/// The cleaned HTML document is rendered back to Markdown first, so the
/// dumped `nodes` and `features` describe the same AST the CRF sees.
pub fn dump_features_from_html(
    html: &str,
    config: &ScoringConfig,
    overrides: Option<&crate::classifier::ScopeOverrides>,
) -> FeatureDump {
    let analysis = crate::clean_html::analyze_html_detailed(html);
    let markdown = crate::markdown::render(&analysis.document, None);
    dump_features_with_overrides(&markdown, config, overrides)
}

/// Dumps AST nodes, extracted static features, and optionally resolved BIO tags
/// with [`crate::classifier::ScopeOverrides`] applied.
pub fn dump_features_with_overrides(
    markdown: &str,
    config: &ScoringConfig,
    overrides: Option<&crate::classifier::ScopeOverrides>,
) -> FeatureDump {
    let normalized = crate::normalize::normalize_markdown(markdown);
    let nodes = crate::parser::parse_markdown(&normalized);
    let lexicon = Lexicon::new(config);
    let mut dump_nodes = Vec::with_capacity(nodes.len());
    let mut dump_features = Vec::with_capacity(nodes.len());
    for (index, node) in nodes.iter().enumerate() {
        let features = lexicon.extract(node);
        let kind = match &node.kind {
            crate::model::NodeKind::Paragraph => "paragraph",
            crate::model::NodeKind::DisplayMath => "display_math",
            crate::model::NodeKind::List => "list",
            crate::model::NodeKind::ListItem => "list_item",
            crate::model::NodeKind::Heading(_) => "heading",
            crate::model::NodeKind::Quote => "quote",
            crate::model::NodeKind::Rule => "rule",
            crate::model::NodeKind::Table => "table",
            crate::model::NodeKind::Code => "code",
            crate::model::NodeKind::Other => "other",
        }
        .to_string();
        dump_nodes.push(DumpNode {
            index,
            kind,
            flat_text: node.flat.clone(),
            char_len: node.flat.chars().count(),
            env_span: node.env.clone(),
        });
        dump_features.push(features);
    }
    let bio_tags = overrides.map(|ov| {
        let classifier = crate::classifier::HeuristicClassifier::new(config.clone());
        let detailed = classifier.classify_detailed(&nodes, ov);
        let mut tags = vec!["O".to_string(); nodes.len()];
        for block in &detailed.blocks {
            if block.kind.is_semantic() && block.span.start < nodes.len() {
                tags[block.span.start] = format!("B-{}", block.kind.slug().to_uppercase());
                let end = block.span.end.min(nodes.len());
                for tag in &mut tags[block.span.start + 1..end] {
                    *tag = format!("I-{}", block.kind.slug().to_uppercase());
                }
            }
        }
        tags
    });
    FeatureDump {
        nodes: dump_nodes,
        features: dump_features,
        bio_tags,
    }
}

/// Anonymized telemetry record capturing human corrections for continuous CRF training.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryEntry {
    pub doc_id: String,
    pub language: String,
    #[serde(default)]
    pub title: String,
    pub nodes: Vec<DumpNode>,
    pub features: Vec<NodeFeatures>,
    pub bio_tags: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Anonymizes nodes by stripping prose text while preserving structural signals
/// (`kind`, `char_len`, `env_span`, and trailing colon for relational features).
pub fn anonymize_dump_nodes(mut nodes: Vec<DumpNode>) -> Vec<DumpNode> {
    for node in &mut nodes {
        let ends_with_colon =
            node.flat_text.trim().ends_with(':') || node.flat_text.trim().ends_with('：');
        node.flat_text = if ends_with_colon {
            ":".to_string()
        } else {
            String::new()
        };
    }
    nodes
}

/// Detects whether text is predominantly Persian or English.
pub fn detect_language(text: &str) -> &'static str {
    let persian_count = text
        .chars()
        .filter(|&c| {
            ('\u{0600}'..='\u{06FF}').contains(&c)
                || ('\u{FB50}'..='\u{FDFF}').contains(&c)
                || ('\u{FE70}'..='\u{FEFF}').contains(&c)
        })
        .count();
    let non_ws = text.chars().filter(|c| !c.is_whitespace()).count();
    if persian_count >= 5 || (non_ws > 0 && (persian_count as f32 / non_ws as f32) > 0.05) {
        "fa"
    } else {
        "en"
    }
}

/// Creates an anonymized [`TelemetryEntry`] from user overrides on a document.
pub fn create_override_telemetry(
    markdown: &str,
    config: &ScoringConfig,
    overrides: &crate::classifier::ScopeOverrides,
) -> Option<TelemetryEntry> {
    if overrides.is_empty() {
        return None;
    }
    let dump = dump_features_with_overrides(markdown, config, Some(overrides));
    let bio_tags = dump.bio_tags?;
    let nodes = anonymize_dump_nodes(dump.nodes);
    let language = detect_language(markdown).to_string();

    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for n in &nodes {
        n.kind.hash(&mut hasher);
        n.char_len.hash(&mut hasher);
    }
    for t in &bio_tags {
        t.hash(&mut hasher);
    }
    let doc_id = format!("telemetry_{:016x}", hasher.finish());

    Some(TelemetryEntry {
        doc_id,
        language,
        title: String::new(),
        nodes,
        features: dump.features,
        bio_tags,
        metadata: serde_json::json!({
            "source": "local_override_telemetry",
        }),
    })
}

/// Appends a [`TelemetryEntry`] as a JSON line to the target local JSONL file.
pub fn append_telemetry_entry(
    path: &std::path::Path,
    entry: &TelemetryEntry,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    use std::io::Write;
    let json = serde_json::to_string(entry)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    writeln!(file, "{json}")?;
    Ok(())
}

/// The [`ScoringConfig`] word lists, folded once for matching.
#[derive(Debug, Clone)]
pub struct Lexicon {
    keywords: Vec<(BlockType, Vec<char>, String)>,
    logic: Vec<String>,
    qed: Vec<String>,
    definitional: Vec<String>,
    proof_cues: Vec<String>,
    statement_cues: Vec<String>,
    continuation: Vec<String>,
    reference_cues: Vec<Vec<char>>,
}

fn fold_all(items: &[String]) -> Vec<String> {
    items
        .iter()
        .map(|s| fold(s))
        .filter(|s| !s.is_empty())
        .collect()
}

fn skip_ws(chars: &[char], mut p: usize) -> usize {
    while p < chars.len() && chars[p].is_whitespace() {
        p += 1;
    }
    p
}

impl Lexicon {
    pub fn new(config: &ScoringConfig) -> Self {
        let mut keywords = Vec::new();
        for rule in &config.keywords {
            for word in &rule.words {
                let folded = fold(word);
                if !folded.is_empty() {
                    keywords.push((rule.kind, folded.chars().collect(), folded));
                }
            }
        }
        Lexicon {
            keywords,
            logic: config.logic_symbols.clone(),
            qed: fold_all(&config.qed_markers),
            definitional: fold_all(&config.definitional_cues),
            proof_cues: fold_all(&config.proof_cues),
            statement_cues: fold_all(&config.statement_cues),
            continuation: fold_all(&config.continuation_cues),
            reference_cues: fold_all(&config.reference_cues)
                .iter()
                .map(|c| c.chars().collect())
                .collect(),
        }
    }

    /// The keyword label at the start of `text`, when it reads as a label
    /// ("Theorem 2.", "قضیه ۵.۳ (کوشی).") rather than prose that merely
    /// mentions a block.
    pub fn lead(&self, text: &str) -> Option<KeywordHit> {
        let norm: Vec<char> = normalize(text).chars().collect();
        let low: Vec<char> = norm.iter().copied().map(lower_char).collect();
        self.find_lead(&norm, &low)
            .filter(|hit| hit.labelled && !hit.reference)
    }

    pub fn extract(&self, node: &Node) -> Features {
        let norm: Vec<char> = normalize(&node.text).chars().collect();
        let low: Vec<char> = norm.iter().copied().map(lower_char).collect();
        let low_text: String = low.iter().collect();
        let low_flat = fold(&node.flat);

        let mut mentions = Vec::new();
        for (kind, _, word) in &self.keywords {
            if !mentions.contains(kind) && contains_word(&low_text, word) {
                mentions.push(*kind);
            }
        }

        let math_chars: usize = node.math.iter().map(|m| m.latex.chars().count()).sum();
        let text_chars = node.text.chars().filter(|c| !c.is_whitespace()).count();
        let math_share = match math_chars + text_chars {
            0 => 0.0,
            total => math_chars as f32 / total as f32,
        };

        let logic_count: usize = self
            .logic
            .iter()
            .map(|sym| count_symbol(&node.flat, sym))
            .sum();

        let qed_tail = tail(low_flat.trim_end(), QED_TAIL_CHARS);
        let has = |cues: &[String]| cues.iter().any(|c| low_flat.contains(c.as_str()));
        let lead_text = low_text.trim_start();

        Features {
            lead: self.find_lead(&norm, &low),
            mentions,
            emphasized: node.leading_emphasis,
            math_density: (math_share / MATH_SATURATION).min(1.0),
            logic_count,
            logic_density: (logic_count as f32 / LOGIC_SATURATION).min(1.0),
            qed: self.qed.iter().any(|m| qed_tail.contains(m.as_str())),
            definitional: has(&self.definitional),
            proof_cue: has(&self.proof_cues),
            statement_cue: has(&self.statement_cues),
            continuation: self.continuation.iter().any(|c| {
                lead_text
                    .strip_prefix(c.as_str())
                    .is_some_and(|rest| !rest.chars().next().is_some_and(char::is_alphanumeric))
            }),
        }
    }

    /// Longest keyword starting at `at`, as (type, length in chars).
    pub(crate) fn keyword_at(&self, low: &[char], at: usize) -> Option<(BlockType, usize)> {
        self.keywords
            .iter()
            .filter(|(_, word, _)| {
                low[at..].starts_with(word)
                    && !low
                        .get(at + word.len())
                        .is_some_and(|c| c.is_alphanumeric())
            })
            .max_by_key(|(_, word, _)| word.len())
            .map(|(kind, word, _)| (*kind, word.len()))
    }

    /// `norm` and `low` are the same text, char-aligned; labels are read from
    /// `norm` so names keep their original case.
    fn find_lead(&self, norm: &[char], low: &[char]) -> Option<KeywordHit> {
        let start = low.iter().position(|c| c.is_alphanumeric())?;
        let n = norm.len();
        let (kind, mut p, number) = match self.keyword_at(low, start) {
            Some((kind, len)) => {
                let mut p = skip_ws(norm, start + len);
                let num_start = p;
                p = scan_number(norm, p);
                let number = (p > num_start).then(|| norm[num_start..p].iter().collect::<String>());
                (kind, skip_ws(norm, p), number)
            }
            None if norm[start].is_numeric() => {
                // Textbook style where the number precedes the keyword:
                // "1.2 Theorem.", "1.2. Lemma (Zorn).", "(1.2) Theorem.", "۳.۲ قضیه."
                let end = scan_number(norm, start);
                let number = norm[start..end].iter().collect::<String>();
                let mut q = skip_ws(norm, end);
                if norm[..start].iter().rev().find(|c| !c.is_whitespace()) == Some(&'(')
                    && norm.get(q) == Some(&')')
                {
                    q = skip_ws(norm, q + 1);
                }
                if norm
                    .get(q)
                    .is_some_and(|c| matches!(c, '.' | ':' | '-' | '–' | '—'))
                {
                    q = skip_ws(norm, q + 1);
                }
                let (kind, len) = self.keyword_at(low, q)?;
                (kind, skip_ws(norm, q + len), Some(number))
            }
            None => return None,
        };

        let mut name = None;
        if norm.get(p) == Some(&'(')
            && let Some(close) = norm[p..].iter().position(|c| *c == ')')
        {
            let inner: String = norm[p + 1..p + close].iter().collect();
            name = Some(inner.trim().to_string()).filter(|s| !s.is_empty());
            p = skip_ws(norm, p + close + 1);
        }

        // "Proof of Theorem 2", "اثبات قضیه ۲".
        let mut refers_to = None;
        if kind.is_argument() && number.is_none() && name.is_none() {
            let mut q = p;
            if low[q..].starts_with(&['o', 'f', ' ']) {
                q = skip_ws(low, q + 3);
            }
            if q < n
                && let Some((target, len)) = self.keyword_at(low, q)
                && !target.is_argument()
            {
                let from = skip_ws(norm, q + len);
                let to = scan_number(norm, from);
                refers_to = Some(Reference {
                    kind: target,
                    number: (to > from).then(|| norm[from..to].iter().collect()),
                });
            }
        }

        let punct = norm
            .get(p)
            .is_some_and(|c| matches!(c, ':' | '.' | '-' | '–' | '—'));
        let reference = !punct
            && name.is_none()
            && refers_to.is_none()
            && self.reference_cues.iter().any(|cue| {
                low[p.min(n)..].starts_with(cue)
                    && !low.get(p + cue.len()).is_some_and(|c| c.is_alphanumeric())
            });
        if punct {
            p += 1;
        }
        let rest_empty = !norm[p.min(n)..].iter().any(|c| c.is_alphanumeric());

        Some(KeywordHit {
            kind,
            labelled: name.is_some() || punct || refers_to.is_some() || rest_empty,
            body_empty: rest_empty && refers_to.is_none(),
            reference,
            refers_to,
            number,
            name,
        })
    }
}

/// End of the label number starting at `p` (`3`, `۳.۲`, `2-1`); `p` itself
/// when there is none.
pub(crate) fn scan_number(chars: &[char], mut p: usize) -> usize {
    let start = p;
    while p < chars.len()
        && (chars[p].is_numeric()
            || (p > start
                && matches!(chars[p], '.' | '-' | '–' | '/')
                && chars.get(p + 1).is_some_and(|c| c.is_numeric())))
    {
        p += 1;
    }
    p
}

/// Occurrences of `symbol`; a `\command` must not run on into more letters
/// (`\to` is not found inside `\top`).
fn count_symbol(hay: &str, symbol: &str) -> usize {
    let is_command = symbol.starts_with('\\');
    hay.match_indices(symbol)
        .filter(|(i, m)| {
            !is_command
                || !hay[i + m.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_markdown;

    fn features(md: &str) -> Features {
        let node = parse_markdown(md).remove(0);
        Lexicon::new(&ScoringConfig::default()).extract(&node)
    }

    #[test]
    fn persian_label_with_number_and_name() {
        let f = features("**قضیه ۳.۲ (هاینه–کانتور).** اگر $f$ پیوسته باشد، آنگاه یکنواخت است.");
        let lead = f.lead.expect("lead keyword");
        assert_eq!(lead.kind, BlockType::Theorem);
        assert_eq!(lead.number.as_deref(), Some("۳.۲"));
        assert_eq!(lead.name.as_deref(), Some("هاینه–کانتور"));
        assert!(lead.labelled && !lead.body_empty);
        assert!(f.emphasized && f.statement_cue);
    }

    #[test]
    fn english_label_keeps_name_case() {
        let lead = features("Theorem 1 (Cauchy). Let $G$ be a finite group.")
            .lead
            .unwrap();
        assert_eq!(lead.kind, BlockType::Theorem);
        assert_eq!(lead.number.as_deref(), Some("1"));
        assert_eq!(lead.name.as_deref(), Some("Cauchy"));
    }

    #[test]
    fn arabic_letter_variants_from_ocr_still_match() {
        // "تعريف" typed with Arabic yeh, "قضيه" likewise.
        assert_eq!(
            features("تعريف ۱. گروه").lead.unwrap().kind,
            BlockType::Definition
        );
        assert_eq!(
            features("قضيه ۲: متن").lead.unwrap().kind,
            BlockType::Theorem
        );
    }

    #[test]
    fn keyword_prefixes_of_other_words_are_not_leads() {
        assert!(features("لمس کردن این سطح خطرناک است.").lead.is_none());
        assert!(features("Definitions vary between authors.").lead.is_none());
        assert!(features("تعریف‌ها و مفاهیم اولیه").lead.is_none());
    }

    #[test]
    fn unlabelled_lead_is_flagged() {
        let lead = features("Note that $f$ is continuous on the interval.")
            .lead
            .unwrap();
        assert_eq!(lead.kind, BlockType::Remark);
        assert!(!lead.labelled);
    }

    #[test]
    fn proof_referring_to_a_statement_is_labelled() {
        assert!(features("اثبات قضیه ۲.۱ را می‌آوریم").lead.unwrap().labelled);
        assert!(features("Proof of Lemma 3 follows").lead.unwrap().labelled);
        assert!(
            !features("Proof techniques vary widely")
                .lead
                .unwrap()
                .labelled
        );
    }

    #[test]
    fn proof_reference_names_its_statement() {
        let hit = features("اثبات قضیه ۲.۱. فرض کنید").lead.unwrap();
        assert_eq!(
            hit.refers_to,
            Some(Reference {
                kind: BlockType::Theorem,
                number: Some("۲.۱".into())
            })
        );
        let hit = features("Proof of the main result.").lead.unwrap();
        assert_eq!(hit.refers_to, None);
    }

    #[test]
    fn a_verb_after_the_label_marks_a_reference() {
        for md in [
            "قضیه ۲ نتیجه می‌دهد که $f$ کراندار است.",
            "Theorem 2 implies that $f$ is bounded.",
            "لم ۳ را در بخش بعد ثابت می‌کنیم.",
        ] {
            let hit = features(md).lead.unwrap();
            assert!(hit.reference && !hit.labelled, "{md}");
        }
        let hit = features("Theorem 2 Let $G$ be a group.").lead.unwrap();
        assert!(!hit.reference && !hit.labelled);
        assert!(!features("Theorem 2. This implies").lead.unwrap().reference);
    }

    #[test]
    fn title_only_opener_has_empty_body() {
        assert!(features("قضیه ۴ (رول)").lead.unwrap().body_empty);
        assert!(features("**Proof.**").lead.unwrap().body_empty);
    }

    #[test]
    fn logic_symbols_and_density() {
        let f = features(r"$$\forall x \; \exists y : x < y \implies f(x) \to \top$$");
        assert_eq!(f.logic_count, 3);
        assert_eq!(f.logic_density, 1.0);
        assert_eq!(f.math_density, 1.0);
        assert!(f.lead.is_none());
    }

    #[test]
    fn qed_endings() {
        assert!(features(r"که تناقض است. $\blacksquare$").qed);
        assert!(features("و بنابراین حکم ثابت شد.").qed);
        assert!(features("which completes the argument. □").qed);
        assert!(features("This is what we wanted. Q.E.D.").qed);
        assert!(!features(r"فرض کنید $\square$ یک عملگر باشد و سپس متن بسیار طولانی‌تری در ادامه می‌آید که نشان می‌دهد این نماد در انتهای بند نیست.").qed);
    }

    #[test]
    fn definitional_and_continuation_cues() {
        assert!(features("تابع $f$ را پیوسته می‌نامیم هرگاه حد وجود داشته باشد.").definitional);
        assert!(features("A group $G$ is called abelian if it commutes.").definitional);
        assert!(features("که در آن $c$ ثابت است.").continuation);
        assert!(features("where $c$ is a constant.").continuation);
        assert!(!features("whereas this one is different.").continuation);
    }

    #[test]
    fn number_first_textbook_labels() {
        let hit = features("1.2 Theorem. Let $G$ be a finite group.")
            .lead
            .unwrap();
        assert_eq!(hit.kind, BlockType::Theorem);
        assert_eq!(hit.number.as_deref(), Some("1.2"));
        assert!(hit.labelled && !hit.reference && !hit.body_empty);

        let hit = features("**1.2.3. Lemma (Zorn):** Every chain has an upper bound.")
            .lead
            .unwrap();
        assert_eq!(hit.kind, BlockType::Lemma);
        assert_eq!(hit.number.as_deref(), Some("1.2.3"));
        assert_eq!(hit.name.as_deref(), Some("Zorn"));
        assert!(hit.labelled);

        let hit = features("(2.1) Corollary. Hence $f$ is bounded.")
            .lead
            .unwrap();
        assert_eq!(hit.kind, BlockType::Corollary);
        assert_eq!(hit.number.as_deref(), Some("2.1"));
        assert!(hit.labelled);

        let hit = features("۳.۲ قضیه (هاینه–کانتور). اگر $f$ پیوسته باشد...")
            .lead
            .unwrap();
        assert_eq!(hit.kind, BlockType::Theorem);
        assert_eq!(hit.number.as_deref(), Some("۳.۲"));
        assert_eq!(hit.name.as_deref(), Some("هاینه–کانتور"));
        assert!(hit.labelled);

        let hit = features("1.2 Theorem implies that $f$ is bounded.")
            .lead
            .unwrap();
        assert!(hit.reference && !hit.labelled);
    }

    #[test]
    fn dump_features_produces_consistent_schema() {
        let md = "# Title\n\n**Theorem 1.** Let $G$ be a group.\n\n$$\\sum_{i=1}^n i$$\n";
        let dump = dump_features(md, &ScoringConfig::default());
        assert_eq!(dump.nodes.len(), 3);
        assert_eq!(dump.features.len(), 3);
        assert_eq!(dump.nodes[0].kind, "heading");
        assert_eq!(dump.nodes[1].kind, "paragraph");
        assert_eq!(dump.nodes[2].kind, "display_math");
        assert!(dump.features[1].lead.is_some());
        assert!(dump.bio_tags.is_none());

        let mut ov = crate::classifier::ScopeOverrides::default();
        ov.retype.insert(1.into(), BlockType::Lemma);
        let dump_with_ov = dump_features_with_overrides(md, &ScoringConfig::default(), Some(&ov));
        assert!(dump_with_ov.bio_tags.is_some());
        let tags = dump_with_ov.bio_tags.unwrap();
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0], "O");
        assert_eq!(tags[1], "B-LEMMA");
        assert_eq!(tags[2], "I-LEMMA");
    }

    #[test]
    fn dump_features_from_html_produces_valid_nodes_and_features() {
        let html = "<html><body><h1>Groups</h1>\
                    <p><strong>Theorem 1.</strong> Let $G$ be a finite group.</p>\
                    <p>$$|G| = \\sum_{g \\in G} 1$$</p>\
                    <p><strong>Proof.</strong> Count the elements.</p></body></html>";
        let dump = dump_features_from_html(html, &ScoringConfig::default(), None);
        assert!(!dump.nodes.is_empty());
        assert_eq!(dump.nodes.len(), dump.features.len());
        for (index, node) in dump.nodes.iter().enumerate() {
            assert_eq!(node.index, index);
            assert!(!node.kind.is_empty());
            assert_eq!(node.char_len, node.flat_text.chars().count());
        }
        assert!(dump.nodes.iter().any(|n| n.kind == "heading"));
        assert!(dump.nodes.iter().any(|n| n.kind == "display_math"));
        assert!(dump.features.iter().any(|f| f.lead.is_some()));
        assert!(dump.bio_tags.is_none());
    }
}
