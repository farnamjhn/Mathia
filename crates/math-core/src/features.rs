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
    /// The keyword reads as a label: followed by a number, a name, `.`/`:`,
    /// nothing at all, or (for proofs) a reference such as "of Theorem 2".
    pub labelled: bool,
    /// Nothing but the label on this node; the statement is in the next one.
    pub body_empty: bool,
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
}

fn fold_all(items: &[String]) -> Vec<String> {
    items.iter().map(|s| fold(s)).filter(|s| !s.is_empty()).collect()
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
        }
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

        let logic_count: usize =
            self.logic.iter().map(|sym| count_symbol(&node.flat, sym)).sum();

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
                lead_text.strip_prefix(c.as_str()).is_some_and(|rest| {
                    !rest.chars().next().is_some_and(char::is_alphanumeric)
                })
            }),
        }
    }

    /// Longest keyword starting at `at`, as (type, length in chars).
    fn keyword_at(&self, low: &[char], at: usize) -> Option<(BlockType, usize)> {
        self.keywords
            .iter()
            .filter(|(_, word, _)| {
                low[at..].starts_with(word)
                    && !low.get(at + word.len()).is_some_and(|c| c.is_alphanumeric())
            })
            .max_by_key(|(_, word, _)| word.len())
            .map(|(kind, word, _)| (*kind, word.len()))
    }

    /// `norm` and `low` are the same text, char-aligned; labels are read from
    /// `norm` so names keep their original case.
    fn find_lead(&self, norm: &[char], low: &[char]) -> Option<KeywordHit> {
        let start = low.iter().position(|c| c.is_alphanumeric())?;
        let (kind, len) = self.keyword_at(low, start)?;
        let n = norm.len();
        let mut p = skip_ws(norm, start + len);

        let num_start = p;
        while p < n
            && (norm[p].is_numeric()
                || (p > num_start
                    && matches!(norm[p], '.' | '-' | '–' | '/')
                    && norm.get(p + 1).is_some_and(|c| c.is_numeric())))
        {
            p += 1;
        }
        let number = (p > num_start).then(|| norm[num_start..p].iter().collect::<String>());
        p = skip_ws(norm, p);

        let mut name = None;
        if norm.get(p) == Some(&'(')
            && let Some(close) = norm[p..].iter().position(|c| *c == ')')
        {
            let inner: String = norm[p + 1..p + close].iter().collect();
            name = Some(inner.trim().to_string()).filter(|s| !s.is_empty());
            p = skip_ws(norm, p + close + 1);
        }

        // "Proof of Theorem 2", "اثبات قضیه ۲".
        let mut refers = false;
        if kind == BlockType::Proof && number.is_none() && name.is_none() {
            let mut q = p;
            if low[q..].starts_with(&['o', 'f', ' ']) {
                q = skip_ws(low, q + 3);
            }
            if q < n {
                refers = self.keyword_at(low, q).is_some_and(|(k, _)| k != BlockType::Proof);
            }
        }

        let punct = norm.get(p).is_some_and(|c| matches!(c, ':' | '.' | '-' | '–' | '—'));
        if punct {
            p += 1;
        }
        let rest_empty = !norm[p.min(n)..].iter().any(|c| c.is_alphanumeric());

        Some(KeywordHit {
            kind,
            labelled: number.is_some() || name.is_some() || punct || refers || rest_empty,
            body_empty: rest_empty && !refers,
            number,
            name,
        })
    }
}

/// Occurrences of `symbol`; a `\command` must not run on into more letters
/// (`\to` is not found inside `\top`).
fn count_symbol(hay: &str, symbol: &str) -> usize {
    let is_command = symbol.starts_with('\\');
    hay.match_indices(symbol)
        .filter(|(i, m)| {
            !is_command
                || !hay[i + m.len()..].chars().next().is_some_and(|c| c.is_ascii_alphabetic())
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
        let lead = features("Theorem 1 (Cauchy). Let $G$ be a finite group.").lead.unwrap();
        assert_eq!(lead.kind, BlockType::Theorem);
        assert_eq!(lead.number.as_deref(), Some("1"));
        assert_eq!(lead.name.as_deref(), Some("Cauchy"));
    }

    #[test]
    fn arabic_letter_variants_from_ocr_still_match() {
        // "تعريف" typed with Arabic yeh, "قضيه" likewise.
        assert_eq!(features("تعريف ۱. گروه").lead.unwrap().kind, BlockType::Definition);
        assert_eq!(features("قضيه ۲: متن").lead.unwrap().kind, BlockType::Theorem);
    }

    #[test]
    fn keyword_prefixes_of_other_words_are_not_leads() {
        assert!(features("لمس کردن این سطح خطرناک است.").lead.is_none());
        assert!(features("Definitions vary between authors.").lead.is_none());
        assert!(features("تعریف‌ها و مفاهیم اولیه").lead.is_none());
    }

    #[test]
    fn unlabelled_lead_is_flagged() {
        let lead = features("Note that $f$ is continuous on the interval.").lead.unwrap();
        assert_eq!(lead.kind, BlockType::Remark);
        assert!(!lead.labelled);
    }

    #[test]
    fn proof_referring_to_a_statement_is_labelled() {
        assert!(features("اثبات قضیه ۲.۱ را می‌آوریم").lead.unwrap().labelled);
        assert!(features("Proof of Lemma 3 follows").lead.unwrap().labelled);
        assert!(!features("Proof techniques vary widely").lead.unwrap().labelled);
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
}
