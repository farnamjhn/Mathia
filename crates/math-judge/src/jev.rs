//! The questions a decision model answers about a proposed block, in the
//! shape of TypeSafe's Jev API (`POST /v1/systemone`), and the truth table
//! that turns the answers into a [`Verdict`].
//!
//! All question wording, option descriptions and thresholds live in this
//! module. The model makes judgments ("is this a unit?", "does the next
//! paragraph continue it?"); [`decide`] makes the decision.
//!
//! Every provider gets the same questions: TypeSafe (or any server that
//! speaks the same API) natively, and chat models through
//! [`chat_instructions`], which asks them to answer in Jev's response shape.

use std::collections::BTreeMap;

use math_core::{BlockType, Node, SemanticBlock, Verdict};
use serde_json::{Value, json};

use crate::{JudgeResponse, with_delimiters};

/// Change whenever a question, its criteria or the state layout changes: it
/// is part of every cache key, so old verdicts are not reused for new
/// questions.
pub const QUESTIONS_VERSION: &str = "q1";

/// How many paragraphs before the block are sent.
const BEFORE_PARAGRAPHS: usize = 2;

/// Noul thresholds: above `YES` reads as yes, below `NO` as no, and the band
/// between means the state does not settle the question.
const YES: f64 = 0.7;
const NO: f64 = 0.3;
/// A different block type must get at least this probability to retype.
const RETYPE: f64 = 0.7;

const IS_BLOCK: &str = "is_block";
const KIND: &str = "kind";
const NEXT_CONTINUES: &str = "next_continues";
const LAST_BELONGS: &str = "last_belongs";

/// What each block type means, for the `kind` question.
fn kind_criteria() -> Value {
    let mut criteria = serde_json::Map::new();
    for kind in BlockType::SEMANTIC {
        let description = match kind {
            BlockType::Theorem => "A main result stated for proof, usually labelled Theorem.",
            BlockType::Lemma => "An auxiliary result stated to help prove a larger one.",
            BlockType::Proposition => "A result of moderate importance, stated for proof.",
            BlockType::Corollary => "A result that follows quickly from a previous one.",
            BlockType::Definition => "Introduces a new term or object and fixes its meaning.",
            BlockType::Proof => "An argument showing why a stated result holds.",
            BlockType::Example => "A worked, concrete instance that illustrates an idea.",
            BlockType::Remark => "A side comment or observation set apart from the main text.",
            BlockType::Exercise => "A problem left for the reader to solve.",
            BlockType::Solution => "The worked answer to an exercise or example.",
            BlockType::Claim => "An intermediate assertion made inside a longer argument.",
            BlockType::Notation => "Fixes symbols or conventions without defining a new concept.",
            BlockType::Heading | BlockType::Text => continue,
        };
        criteria.insert(type_name(kind), json!(description));
    }
    Value::Object(criteria)
}

fn type_name(kind: BlockType) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The `state` and `questions` of a Jev request about one block. The
/// proposed type is deliberately left out of the state so it cannot sway the
/// `kind` answer; [`decide`] compares against it in code.
pub fn request(block: &SemanticBlock, nodes: &[Node]) -> (Value, Value) {
    let text = |i: usize| nodes.get(i).map(with_delimiters);
    let span = block.span.clone();
    let paragraphs: Vec<String> = span.clone().filter_map(text).collect();
    let before: Vec<String> = (span.start.saturating_sub(BEFORE_PARAGRAPHS)..span.start)
        .filter_map(text)
        .collect();
    let next = text(span.end);

    let mut block_state = serde_json::Map::new();
    let mut questions = serde_json::Map::new();
    questions.insert(
        IS_BLOCK.into(),
        json!({
            "type": "noul",
            "instructions": "Is `block` a self-contained unit of a mathematics text, such as a \
                theorem, proof, definition or worked example?",
            "criteria": {
                "true": "`block` states, proves, defines or works through something as one unit, \
                    the way a textbook sets such units apart from the running text.",
                "false": "`block` is running prose, a section introduction, or a sentence that \
                    only mentions a theorem or proof in passing.",
            },
        }),
    );
    questions.insert(
        KIND.into(),
        json!({
            "type": "choice",
            "instructions": "What kind of unit is `block`?",
            "criteria": kind_criteria(),
        }),
    );
    match paragraphs.split_last() {
        Some((last, opening)) if !opening.is_empty() => {
            block_state.insert("opening".into(), json!(opening));
            block_state.insert("last_paragraph".into(), json!(last));
            questions.insert(
                LAST_BELONGS.into(),
                json!({
                    "type": "noul",
                    "instructions": "Does `block.last_paragraph` belong to the same unit as \
                        `block.opening`?",
                    "criteria": {
                        "true": "`block.last_paragraph` carries on the same statement, proof or \
                            example as `block.opening`.",
                        "false": "`block.last_paragraph` starts something new, such as the main \
                            text resuming after a proof has ended, or the next statement.",
                    },
                }),
            );
        }
        _ => {
            block_state.insert("paragraphs".into(), json!(paragraphs));
        }
    }

    let mut state = serde_json::Map::new();
    if !before.is_empty() {
        state.insert("paragraphs_before_block".into(), json!(before));
    }
    state.insert("block".into(), Value::Object(block_state));
    if let Some(next) = next {
        state.insert("next_paragraph".into(), json!(next));
        questions.insert(
            NEXT_CONTINUES.into(),
            json!({
                "type": "noul",
                "instructions": "Does `next_paragraph` continue the unit in `block`?",
                "criteria": {
                    "true": "`next_paragraph` carries on the same statement, proof or example, \
                        such as the next case of the proof or the end of a computation.",
                    "false": "`next_paragraph` starts something new: another statement, a new \
                        proof, a heading, or the main text resuming after the unit.",
                },
            }),
        );
    }
    (Value::Object(state), Value::Object(questions))
}

/// The answers this module reads, taken out of a Jev `answers` object.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Answers {
    pub is_block: Option<f64>,
    pub kind: Option<(String, BTreeMap<String, f64>)>,
    pub next_continues: Option<f64>,
    pub last_belongs: Option<f64>,
}

impl Answers {
    /// Reads `answers` from a Jev response (or a chat model's imitation of
    /// one). A question that was asked but has no readable answer is an
    /// error, not a "no".
    pub fn read(answers: &Value, questions: &Value) -> Result<Answers, String> {
        let asked = |id: &str| questions.get(id).is_some();
        let noul = |id: &str| -> Result<Option<f64>, String> {
            if !asked(id) {
                return Ok(None);
            }
            let p = answers[id]["noul"]
                .as_f64()
                .ok_or_else(|| format!("no answer to `{id}`"))?;
            if !(0.0..=1.0).contains(&p) {
                return Err(format!("`{id}` is {p}, not a probability"));
            }
            Ok(Some(p))
        };
        let kind = if asked(KIND) {
            let choice = answers[KIND]["choice"]
                .as_str()
                .ok_or("no answer to `kind`")?
                .to_string();
            let probabilities = answers[KIND]["probabilities"]
                .as_object()
                .map(|map| {
                    map.iter()
                        .filter_map(|(k, v)| v.as_f64().map(|p| (k.clone(), p)))
                        .collect()
                })
                .unwrap_or_default();
            Some((choice, probabilities))
        } else {
            None
        };
        Ok(Answers {
            is_block: noul(IS_BLOCK)?,
            kind,
            next_continues: noul(NEXT_CONTINUES)?,
            last_belongs: noul(LAST_BELONGS)?,
        })
    }
}

/// The truth table. Rows are checked in order; anything not settled keeps
/// the heuristic result (`Accept`).
///
/// | `is_block` | `kind`                       | `next_continues` | `last_belongs` | verdict      |
/// |------------|------------------------------|------------------|----------------|--------------|
/// | < 0.3      | –                            | –                | –              | `Reject`     |
/// | 0.3 – 0.7  | –                            | –                | –              | `Accept`     |
/// | > 0.7      | other type, p ≥ 0.7          | –                | –              | `Retype`     |
/// | > 0.7      | –                            | > 0.7            | ≥ 0.3 or none  | `Incomplete` |
/// | > 0.7      | –                            | ≤ 0.7 or none    | < 0.3          | `Overreach`  |
/// | > 0.7      | –                            | > 0.7            | < 0.3          | `Accept`     |
/// | > 0.7      | otherwise                    |                  |                | `Accept`     |
///
/// The last-but-one row is a contradiction (the block should both grow and
/// shrink); the heuristic result stands.
pub fn decide(id: &str, proposed: BlockType, answers: &Answers) -> JudgeResponse {
    let respond = |verdict: Verdict, p: f64| JudgeResponse {
        id: id.to_string(),
        verdict,
        confidence: Some(p as f32),
    };
    let Some(is_block) = answers.is_block else {
        return respond(Verdict::Accept, 1.0);
    };
    if is_block < NO {
        return respond(Verdict::Reject, 1.0 - is_block);
    }
    if is_block <= YES {
        return respond(Verdict::Accept, 1.0);
    }
    if let Some((choice, probabilities)) = &answers.kind {
        let p = probabilities.get(choice).copied().unwrap_or(0.0);
        let other = serde_json::from_value::<BlockType>(json!(choice))
            .ok()
            .filter(|kind| kind.is_semantic() && *kind != proposed);
        if let Some(kind) = other
            && p >= RETYPE
        {
            return respond(Verdict::Retype(kind), p);
        }
    }
    let grows = answers.next_continues.filter(|&p| p > YES);
    let shrinks = answers.last_belongs.filter(|&p| p < NO);
    match (grows, shrinks) {
        (Some(p), None) => respond(Verdict::Incomplete, p),
        (None, Some(p)) => respond(Verdict::Overreach, 1.0 - p),
        _ => respond(Verdict::Accept, 1.0),
    }
}

/// For chat models: how to answer a Jev request in Jev's response shape.
pub const CHAT_INSTRUCTIONS: &str = "You answer typed questions about `state`, like TypeSafe's \
Jev API. The user message is a JSON object with `state` and `questions`; each question is \
answered on its own, from `state` alone. Reply with one JSON object and nothing else: \
{\"answers\": {QUESTION_ID: ANSWER, ...}}, one entry per question. For a \"noul\" question, \
ANSWER is {\"type\": \"noul\", \"noul\": P}, P the probability (0 to 1) that the answer is yes; \
its `criteria` say what counts as true and false. For a \"choice\" question, ANSWER is \
{\"type\": \"choice\", \"choice\": NAME, \"probabilities\": {NAME: P, ...}}, NAME one of the keys \
of its `criteria`, the probabilities summing to 1.";

/// The JSON a chat model is given: the Jev request without `model`.
pub fn chat_message(state: &Value, questions: &Value) -> String {
    json!({"state": state, "questions": questions}).to_string()
}

/// The `answers` object out of a chat model's reply, which may wrap it in
/// prose or a code fence.
pub fn chat_answers(reply: &str) -> Result<Value, String> {
    let start = reply.find('{').ok_or("the reply holds no JSON")?;
    let mut stream = serde_json::Deserializer::from_str(&reply[start..]).into_iter::<Value>();
    let value = stream
        .next()
        .ok_or("the reply holds no JSON")?
        .map_err(|e| format!("unreadable reply: {e}"))?;
    match value.get("answers") {
        Some(answers) if answers.is_object() => Ok(answers.clone()),
        _ => Err("the reply has no `answers` object".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers(is_block: f64, kind: (&str, f64), next: Option<f64>, last: Option<f64>) -> Answers {
        Answers {
            is_block: Some(is_block),
            kind: Some((kind.0.into(), BTreeMap::from([(kind.0.into(), kind.1)]))),
            next_continues: next,
            last_belongs: last,
        }
    }

    fn verdict(a: &Answers) -> Verdict {
        decide("b", BlockType::Theorem, a).verdict
    }

    #[test]
    fn truth_table() {
        assert_eq!(
            verdict(&answers(0.1, ("theorem", 0.9), None, None)),
            Verdict::Reject
        );
        assert_eq!(
            verdict(&answers(0.5, ("lemma", 0.9), Some(0.9), None)),
            Verdict::Accept
        );
        assert_eq!(
            verdict(&answers(0.9, ("lemma", 0.8), None, None)),
            Verdict::Retype(BlockType::Lemma)
        );
        // A weak or unknown different type does not retype.
        assert_eq!(
            verdict(&answers(0.9, ("lemma", 0.5), None, None)),
            Verdict::Accept
        );
        assert_eq!(
            verdict(&answers(0.9, ("text", 0.9), None, None)),
            Verdict::Accept
        );
        assert_eq!(
            verdict(&answers(0.9, ("theorem", 0.9), Some(0.8), Some(0.9))),
            Verdict::Incomplete
        );
        assert_eq!(
            verdict(&answers(0.9, ("theorem", 0.9), Some(0.1), Some(0.1))),
            Verdict::Overreach
        );
        assert_eq!(
            verdict(&answers(0.9, ("theorem", 0.9), Some(0.9), Some(0.1))),
            Verdict::Accept
        );
        assert_eq!(verdict(&Answers::default()), Verdict::Accept);
    }

    #[test]
    fn answers_are_read_and_missing_ones_are_errors() {
        let questions = json!({IS_BLOCK: {}, KIND: {}, NEXT_CONTINUES: {}});
        let good = json!({
            IS_BLOCK: {"type": "noul", "noul": 0.9},
            KIND: {"type": "choice", "choice": "lemma", "confidence": 0.6,
                   "probabilities": {"lemma": 0.8, "theorem": 0.2}},
            NEXT_CONTINUES: {"type": "noul", "noul": 0.2},
        });
        let read = Answers::read(&good, &questions).unwrap();
        assert_eq!(read.is_block, Some(0.9));
        assert_eq!(read.last_belongs, None);
        assert_eq!(read.kind.unwrap().1["lemma"], 0.8);
        let missing = json!({IS_BLOCK: {"type": "noul", "noul": 0.9}});
        assert!(Answers::read(&missing, &questions).is_err());
        let bad =
            json!({IS_BLOCK: {"noul": 3.0}, KIND: {"choice": "x"}, NEXT_CONTINUES: {"noul": 0.1}});
        assert!(Answers::read(&bad, &questions).is_err());
    }

    #[test]
    fn chat_replies_are_read_out_of_prose_and_fences() {
        let reply = "Sure:\n```json\n{\"answers\": {\"is_block\": {\"type\": \"noul\", \"noul\": 0.8}}}\n```";
        assert_eq!(chat_answers(reply).unwrap()["is_block"]["noul"], 0.8);
        assert!(chat_answers("no idea").is_err());
        assert!(chat_answers("{\"verdict\": \"accept\"}").is_err());
    }

    #[test]
    fn every_semantic_type_has_a_description() {
        assert_eq!(
            kind_criteria().as_object().unwrap().len(),
            BlockType::SEMANTIC.len()
        );
    }
}
