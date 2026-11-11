//! Decision models for Mathia.
//!
//! `math-core` defines the seam ([`BlockJudge`]) and stays deterministic and
//! model-free. This crate holds everything around a model:
//!
//! * the **contract** a model sees and answers: [`JudgeRequest`] /
//!   [`JudgeResponse`], plain JSON;
//! * [`CommandJudge`], which runs any external program (a local model such as
//!   jev, clef or laya behind a small wrapper script, or a remote call) and
//!   talks to it over stdin/stdout, one JSON object per line;
//! * [`ApiJudge`], which asks a hosted model (Anthropic or any OpenAI-compatible
//!   endpoint) with an API key;
//! * [`CachedJudge`], which keys verdicts by a hash of (block text, proposed
//!   type, model version) so recompiling an unchanged document costs nothing
//!   and gives the same result;
//! * [`Policy`], which turns low-confidence answers into `Accept`;
//! * [`Replay`], the overrides and verdict log of a run, enough to rebuild the
//!   document later without the model.
//!
//! Every judge here is deterministic as long as the model behind it is; the
//! cache makes a run reproducible even when it is not.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use math_core::{
    AppliedVerdict, BlockJudge, BlockType, Node, ScopeOverrides, SemanticBlock, Verdict,
};
use serde::{Deserialize, Serialize};

/// Version of the request/response format.
pub const PROTOCOL_VERSION: u32 = 1;

/// How many nodes of context are sent on each side of a block.
const CONTEXT_NODES: usize = 2;

/// A node's prose with every formula back inside `$…$` / `$$…$$`.
/// (`Node::flat` splices the LaTeX in as ` latex `, in order.)
fn with_delimiters(node: &Node) -> String {
    let flat = node.flat.as_str();
    let mut out = String::with_capacity(flat.len() + 4 * node.math.len());
    let mut at = 0;
    for span in &node.math {
        let needle = format!(" {} ", span.latex);
        let Some(found) = flat[at..].find(&needle) else {
            continue;
        };
        let fence = if span.display { "$$" } else { "$" };
        out.push_str(&flat[at..at + found]);
        out.push(' ');
        out.push_str(fence);
        out.push_str(&span.latex);
        out.push_str(fence);
        out.push(' ');
        at += found + needle.len();
    }
    out.push_str(&flat[at..]);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What a model is asked about one proposed block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JudgeRequest {
    pub protocol: u32,
    /// Echoed back in the response so batches can be matched up.
    pub id: String,
    /// The proposed type.
    pub kind: BlockType,
    /// Node range of the proposed scope; verdicts that name nodes
    /// (`end_at`, `start_at`) use the same absolute indices.
    pub start: usize,
    pub end: usize,
    pub nodes_total: usize,
    /// The block's Markdown text with LaTeX restored, nodes separated by a
    /// blank line.
    pub text: String,
    /// Up to two nodes before and after the block, each `(index, text)`.
    pub before: Vec<(usize, String)>,
    pub after: Vec<(usize, String)>,
    /// Id of the statement a proof proves / the exercise a solution answers.
    pub proves: Option<String>,
    /// The heuristic score of the opener.
    pub score: f32,
}

impl JudgeRequest {
    pub fn new(block: &SemanticBlock, nodes: &[Node]) -> Self {
        let text_of = |range: std::ops::Range<usize>| -> Vec<(usize, String)> {
            range
                .filter_map(|i| nodes.get(i).map(|n| (i, with_delimiters(n))))
                .collect()
        };
        let span = block.span.clone();
        JudgeRequest {
            protocol: PROTOCOL_VERSION,
            id: block.id.clone(),
            kind: block.kind,
            start: span.start,
            end: span.end,
            nodes_total: nodes.len(),
            text: text_of(span.clone())
                .into_iter()
                .map(|(_, text)| text)
                .collect::<Vec<_>>()
                .join("\n\n"),
            before: text_of(span.start.saturating_sub(CONTEXT_NODES)..span.start),
            after: text_of(span.end..span.end + CONTEXT_NODES),
            proves: block.proves.clone(),
            score: block.score,
        }
    }
}

/// A model's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JudgeResponse {
    /// The [`JudgeRequest::id`] this answers.
    pub id: String,
    /// `"accept"`, `"incomplete"`, `"overreach"`, `"reject"`,
    /// `{"retype": "lemma"}`, `{"end_at": 7}` or `{"start_at": 3}`.
    pub verdict: Verdict,
    /// In `[0, 1]`; missing means certain.
    #[serde(default)]
    pub confidence: Option<f32>,
}

/// What to do with a model's answers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Policy {
    /// Answers below this confidence count as `Accept` (the heuristic result
    /// stands).
    pub min_confidence: f32,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            min_confidence: 0.5,
        }
    }
}

impl Policy {
    pub fn apply(&self, response: &JudgeResponse) -> Verdict {
        match response.confidence {
            Some(c) if c < self.min_confidence => Verdict::Accept,
            _ => response.verdict,
        }
    }
}

/// Runs an external program once per batch: one [`JudgeRequest`] per line
/// on its stdin, one [`JudgeResponse`] per line expected on its stdout, in
/// any order. Missing or unreadable answers count as `Accept`, and so does
/// any failure to run the program; [`CommandJudge::errors`] keeps the
/// messages.
#[derive(Debug)]
pub struct CommandJudge {
    program: PathBuf,
    args: Vec<String>,
    model_version: String,
    policy: Policy,
    errors: Mutex<Vec<String>>,
}

impl CommandJudge {
    /// `model_version` names the model and its version; it is part of every
    /// cache key, so change it whenever the model changes.
    pub fn new(program: impl Into<PathBuf>, args: Vec<String>, model_version: &str) -> Self {
        CommandJudge {
            program: program.into(),
            args,
            model_version: model_version.to_string(),
            policy: Policy::default(),
            errors: Mutex::new(Vec::new()),
        }
    }

    /// Parses a shell-like command line split on whitespace (no quoting).
    pub fn from_command_line(command: &str, model_version: &str) -> Result<Self, String> {
        let mut words = command.split_whitespace().map(str::to_string);
        let program = words.next().ok_or("empty judge command")?;
        Ok(Self::new(program, words.collect(), model_version))
    }

    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    pub fn model_version(&self) -> &str {
        &self.model_version
    }

    pub fn errors(&self) -> Vec<String> {
        self.errors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn error(&self, message: String) {
        self.errors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(message);
    }

    fn run(&self, requests: &[JudgeRequest]) -> Result<Vec<JudgeResponse>, String> {
        let name = self.program.display().to_string();
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("{name}: {e}"))?;
        let mut input = String::new();
        for request in requests {
            input.push_str(&serde_json::to_string(request).map_err(|e| e.to_string())?);
            input.push('\n');
        }
        let mut stdin = child.stdin.take().expect("stdin is piped");
        // Write from another thread so a model that answers while still
        // reading cannot deadlock on a full pipe.
        let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut responses = Vec::new();
        for (number, line) in BufReader::new(stdout).lines().enumerate() {
            let line = line.map_err(|e| format!("{name}: {e}"))?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<JudgeResponse>(&line) {
                Ok(response) => responses.push(response),
                Err(e) => self.error(format!("{name}: line {}: {e}", number + 1)),
            }
        }
        writer
            .join()
            .map_err(|_| format!("{name}: writer panicked"))?
            .map_err(|e| format!("{name}: {e}"))?;
        let status = child.wait().map_err(|e| format!("{name}: {e}"))?;
        if !status.success() {
            return Err(format!("{name}: exited with {status}"));
        }
        Ok(responses)
    }
}

impl BlockJudge for CommandJudge {
    fn judge(&self, block: &SemanticBlock, nodes: &[Node]) -> Verdict {
        self.judge_batch(&[block], nodes)[0]
    }

    fn judge_batch(&self, blocks: &[&SemanticBlock], nodes: &[Node]) -> Vec<Verdict> {
        let requests: Vec<JudgeRequest> =
            blocks.iter().map(|b| JudgeRequest::new(b, nodes)).collect();
        let responses = match self.run(&requests) {
            Ok(responses) => responses,
            Err(message) => {
                self.error(message);
                Vec::new()
            }
        };
        let by_id: BTreeMap<&str, &JudgeResponse> =
            responses.iter().map(|r| (r.id.as_str(), r)).collect();
        requests
            .iter()
            .map(|request| {
                by_id
                    .get(request.id.as_str())
                    .map_or(Verdict::Accept, |r| self.policy.apply(r))
            })
            .collect()
    }
}

/// Which wire format an [`ApiJudge`] speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApiProvider {
    /// Anthropic Messages API.
    Anthropic,
    /// OpenAI Chat Completions, as served by OpenAI, Ollama, vLLM, LM Studio…
    OpenAiCompatible,
}

impl ApiProvider {
    pub fn default_base_url(self) -> &'static str {
        match self {
            ApiProvider::Anthropic => "https://api.anthropic.com",
            ApiProvider::OpenAiCompatible => "https://api.openai.com/v1",
        }
    }
}

/// Everything needed to reach a hosted decision model.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiConfig {
    pub provider: ApiProvider,
    /// Without a trailing slash; empty means the provider's default.
    pub base_url: String,
    pub model: String,
    /// May be empty for a local endpoint that needs none.
    pub api_key: String,
}

// Never print the key.
impl std::fmt::Debug for ApiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiConfig")
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &if self.api_key.is_empty() { "" } else { "<set>" })
            .finish()
    }
}

const SYSTEM_PROMPT: &str = "You review how a program split a mathematics document into blocks \
(theorem, lemma, proposition, corollary, definition, proof, example, remark, exercise, solution, \
claim, notation). You are given one proposed block: its type, its text (LaTeX inside $…$), the \
nodes (paragraphs) before and after it, and node indices. Decide whether the proposal is right. \
Answer with one JSON object and nothing else: {\"verdict\": V, \"confidence\": C}, C between 0 \
and 1. V is one of: \"accept\" (correct as is), \"incomplete\" (it stops too early; extend it by \
one node), \"overreach\" (it runs into unrelated content; shorten it by one node), \"reject\" \
(not a block at all), {\"retype\": \"lemma\"} (a block, but of another type; use the lowercase \
type name), {\"end_at\": N} (it should end right before node N, an absolute index) or \
{\"start_at\": N} (it really starts at node N). Prefer \"accept\" when unsure.";

/// Asks a hosted model about each block. Needs a network; every failure
/// leaves the heuristic result alone (`Accept`) and is kept in
/// [`ApiJudge::errors`]. After the first failed call the remaining blocks are
/// not sent, so a wrong key costs one request, not hundreds.
#[derive(Debug)]
pub struct ApiJudge {
    config: ApiConfig,
    policy: Policy,
    agent: ureq::Agent,
    failed: AtomicBool,
    errors: Mutex<Vec<String>>,
}

impl ApiJudge {
    /// Requests running at once.
    const PARALLEL: usize = 4;

    pub fn new(config: ApiConfig) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .build()
            .into();
        ApiJudge {
            config,
            policy: Policy::default(),
            agent,
            failed: AtomicBool::new(false),
            errors: Mutex::new(Vec::new()),
        }
    }

    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// `"provider/model"`: part of every cache key.
    pub fn model_version(&self) -> String {
        let provider = match self.config.provider {
            ApiProvider::Anthropic => "anthropic",
            ApiProvider::OpenAiCompatible => "openai-compatible",
        };
        format!("{provider}/{}", self.config.model)
    }

    pub fn errors(&self) -> Vec<String> {
        self.errors.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn error(&self, message: String) {
        self.failed.store(true, Ordering::Relaxed);
        self.errors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(message);
    }

    fn base(&self) -> &str {
        let url = self.config.base_url.trim().trim_end_matches('/');
        if url.is_empty() {
            self.config.provider.default_base_url()
        } else {
            url
        }
    }

    /// One round trip: the model's reply text for `user`.
    pub fn complete(&self, system: &str, user: &str, max_tokens: u32) -> Result<String, String> {
        let (url, body) = match self.config.provider {
            ApiProvider::Anthropic => (
                format!("{}/v1/messages", self.base()),
                serde_json::json!({
                    "model": self.config.model,
                    "max_tokens": max_tokens,
                    "temperature": 0,
                    "system": system,
                    "messages": [{"role": "user", "content": user}],
                }),
            ),
            ApiProvider::OpenAiCompatible => (
                format!("{}/chat/completions", self.base()),
                serde_json::json!({
                    "model": self.config.model,
                    "max_tokens": max_tokens,
                    "temperature": 0,
                    "messages": [
                        {"role": "system", "content": system},
                        {"role": "user", "content": user},
                    ],
                }),
            ),
        };
        let mut request = self.agent.post(&url).header("content-type", "application/json");
        match self.config.provider {
            ApiProvider::Anthropic => {
                request = request
                    .header("x-api-key", &self.config.api_key)
                    .header("anthropic-version", "2023-06-01");
            }
            ApiProvider::OpenAiCompatible if !self.config.api_key.is_empty() => {
                request = request
                    .header("authorization", &format!("Bearer {}", self.config.api_key));
            }
            ApiProvider::OpenAiCompatible => {}
        }
        let mut response = request
            .send(body.to_string())
            .map_err(|e| format!("{url}: {e}"))?;
        let status = response.status();
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("{url}: {e}"))?;
        let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        if !status.is_success() {
            let detail = json["error"]["message"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| text.chars().take(200).collect());
            return Err(format!("{url}: HTTP {}: {detail}", status.as_u16()));
        }
        let reply = match self.config.provider {
            ApiProvider::Anthropic => json["content"]
                .as_array()
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| p["text"].as_str())
                        .collect::<String>()
                })
                .unwrap_or_default(),
            ApiProvider::OpenAiCompatible => json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        };
        if reply.is_empty() {
            return Err(format!("{url}: the reply had no text"));
        }
        Ok(reply)
    }

    /// Checks the key, URL and model with a one-word question.
    pub fn test(&self) -> Result<(), String> {
        self.complete("Reply with the single word: ok", "ping", 16)
            .map(|_| ())
    }

    fn ask(&self, request: &JudgeRequest) -> Result<Verdict, String> {
        let user = serde_json::to_string(request).map_err(|e| e.to_string())?;
        let reply = self.complete(SYSTEM_PROMPT, &user, 120)?;
        let response = parse_reply(&reply, &request.id)?;
        Ok(self.policy.apply(&response))
    }
}

/// The first JSON object in a model's reply (models like to wrap it in
/// prose or a code fence), read as a [`JudgeResponse`] for `id`.
fn parse_reply(reply: &str, id: &str) -> Result<JudgeResponse, String> {
    #[derive(Deserialize)]
    struct Answer {
        verdict: Verdict,
        #[serde(default)]
        confidence: Option<f32>,
    }
    let start = reply.find('{').ok_or("the reply holds no JSON")?;
    let mut stream = serde_json::Deserializer::from_str(&reply[start..]).into_iter::<Answer>();
    let answer = stream
        .next()
        .ok_or("the reply holds no JSON")?
        .map_err(|e| format!("unreadable verdict: {e}"))?;
    Ok(JudgeResponse {
        id: id.to_string(),
        verdict: answer.verdict,
        confidence: answer.confidence,
    })
}

impl BlockJudge for ApiJudge {
    fn judge(&self, block: &SemanticBlock, nodes: &[Node]) -> Verdict {
        self.judge_batch(&[block], nodes)[0]
    }

    fn judge_batch(&self, blocks: &[&SemanticBlock], nodes: &[Node]) -> Vec<Verdict> {
        let requests: Vec<JudgeRequest> =
            blocks.iter().map(|b| JudgeRequest::new(b, nodes)).collect();
        let mut verdicts = vec![Verdict::Accept; requests.len()];
        let per_thread = requests.len().div_ceil(Self::PARALLEL).max(1);
        std::thread::scope(|scope| {
            for (requests, verdicts) in requests
                .chunks(per_thread)
                .zip(verdicts.chunks_mut(per_thread))
            {
                scope.spawn(move || {
                    for (request, slot) in requests.iter().zip(verdicts) {
                        if self.failed.load(Ordering::Relaxed) {
                            return;
                        }
                        match self.ask(request) {
                            Ok(verdict) => *slot = verdict,
                            Err(message) => self.error(message),
                        }
                    }
                });
            }
        });
        verdicts
    }
}

/// A 128-bit FNV-1a hash of the parts, as hex. Stable across platforms and
/// releases (unlike `std`'s hasher), which is what a cache file needs.
pub fn cache_key(model_version: &str, kind: BlockType, text: &str) -> String {
    let mut hash: u128 = 0x6c62272e07bb014262b821756295c58d;
    const PRIME: u128 = 0x0000000001000000000000000000013B;
    for part in [model_version, kind.slug(), text] {
        for byte in part.bytes().chain([0xff]) {
            hash ^= u128::from(byte);
            hash = hash.wrapping_mul(PRIME);
        }
    }
    format!("{hash:032x}")
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    protocol: u32,
    verdicts: BTreeMap<String, Verdict>,
}

/// Wraps a judge with a verdict cache keyed by [`cache_key`]. Only misses
/// reach the inner judge, in one batch.
pub struct CachedJudge<J> {
    inner: J,
    model_version: String,
    verdicts: Mutex<BTreeMap<String, Verdict>>,
    misses: Mutex<usize>,
}

impl<J: BlockJudge> CachedJudge<J> {
    pub fn new(inner: J, model_version: &str) -> Self {
        CachedJudge {
            inner,
            model_version: model_version.to_string(),
            verdicts: Mutex::new(BTreeMap::new()),
            misses: Mutex::new(0),
        }
    }

    /// Loads cached verdicts from `path`; a missing file is an empty cache.
    pub fn load(mut self, path: &Path) -> Result<Self, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(self),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let file: CacheFile =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if file.protocol != PROTOCOL_VERSION {
            return Err(format!(
                "{}: cache protocol {} is not {PROTOCOL_VERSION}",
                path.display(),
                file.protocol
            ));
        }
        self.verdicts = Mutex::new(file.verdicts);
        Ok(self)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let file = CacheFile {
            protocol: PROTOCOL_VERSION,
            verdicts: self.lock().clone(),
        };
        let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))
    }

    /// How many verdicts had to be asked of the inner judge.
    pub fn misses(&self) -> usize {
        *self.misses.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn inner(&self) -> &J {
        &self.inner
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Verdict>> {
        self.verdicts.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn key(&self, block: &SemanticBlock, nodes: &[Node]) -> String {
        // The scope's position matters for verdicts naming nodes, so it is
        // part of the text that is hashed.
        let request = JudgeRequest::new(block, nodes);
        let text = format!("{}..{}\n{}", request.start, request.end, request.text);
        cache_key(&self.model_version, block.kind, &text)
    }
}

impl<J: BlockJudge> BlockJudge for CachedJudge<J> {
    fn judge(&self, block: &SemanticBlock, nodes: &[Node]) -> Verdict {
        self.judge_batch(&[block], nodes)[0]
    }

    fn judge_batch(&self, blocks: &[&SemanticBlock], nodes: &[Node]) -> Vec<Verdict> {
        let keys: Vec<String> = blocks.iter().map(|b| self.key(b, nodes)).collect();
        let mut found: Vec<Option<Verdict>> = {
            let cache = self.lock();
            keys.iter().map(|k| cache.get(k).copied()).collect()
        };
        let missing: Vec<usize> = (0..blocks.len()).filter(|&i| found[i].is_none()).collect();
        if !missing.is_empty() {
            let asked: Vec<&SemanticBlock> = missing.iter().map(|&i| blocks[i]).collect();
            let answers = self.inner.judge_batch(&asked, nodes);
            *self.misses.lock().unwrap_or_else(|e| e.into_inner()) += missing.len();
            let mut cache = self.lock();
            for (&i, verdict) in missing.iter().zip(answers) {
                cache.insert(keys[i].clone(), verdict);
                found[i] = Some(verdict);
            }
        }
        found
            .into_iter()
            .map(|v| v.unwrap_or(Verdict::Accept))
            .collect()
    }
}

/// Everything needed to rebuild a judged document without the model:
/// pass `overrides` to `math_core::analyze_with_overrides`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Replay {
    pub protocol: u32,
    pub model_version: String,
    pub overrides: ScopeOverrides,
    pub log: Vec<AppliedVerdict>,
}

impl Replay {
    pub fn new(model_version: &str, overrides: ScopeOverrides, log: Vec<AppliedVerdict>) -> Self {
        Replay {
            protocol: PROTOCOL_VERSION,
            model_version: model_version.to_string(),
            overrides,
            log,
        }
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use math_core::{ScoringConfig, analyze_with_judge_detailed, analyze_with_overrides};

    const SAMPLE: &str = "Theorem 1. Statement.\n\nSecond half.\n\nThird.\n\nUnrelated.";

    /// Counts calls; wants theorems three nodes long.
    struct Counting(Mutex<usize>);
    impl BlockJudge for Counting {
        fn judge(&self, block: &SemanticBlock, _: &[Node]) -> Verdict {
            *self.0.lock().unwrap() += 1;
            if block.span.len() < 3 {
                Verdict::Incomplete
            } else {
                Verdict::Accept
            }
        }
    }

    #[test]
    fn requests_carry_text_context_and_round_trip() {
        let config = ScoringConfig::default();
        let doc = math_core::analyze(
            "Intro.\n\nLemma 1. Squares are positive: $x^2 \\ge 0$.\n\nAfter.",
            &config,
        );
        let block = &doc.blocks[1];
        let request = JudgeRequest::new(block, &doc.nodes);
        assert_eq!(
            (request.start, request.end, request.kind),
            (1, 2, BlockType::Lemma)
        );
        assert!(request.text.contains("$x^2 \\ge 0$"), "{}", request.text);
        assert_eq!(request.before, [(0, "Intro.".to_string())]);
        assert_eq!(request.after, [(2, "After.".to_string())]);
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<JudgeRequest>(&json).unwrap(),
            request
        );

        let response: JudgeResponse =
            serde_json::from_str(r#"{"id": "b", "verdict": {"end_at": 7}, "confidence": 0.9}"#)
                .unwrap();
        assert_eq!(response.verdict, Verdict::EndAt(7));
        let unsure = JudgeResponse {
            confidence: Some(0.2),
            ..response
        };
        assert_eq!(Policy::default().apply(&unsure), Verdict::Accept);
    }

    #[test]
    fn cache_answers_repeat_runs_without_the_model() {
        let config = ScoringConfig::default();
        let judge = CachedJudge::new(Counting(Mutex::new(0)), "test-1");
        let (first, overrides, log) =
            analyze_with_judge_detailed(SAMPLE, &config, &judge, 8, ScopeOverrides::default());
        assert_eq!(first.blocks[0].span, 0..3);
        let calls = *judge.inner().0.lock().unwrap();
        assert_eq!(calls, judge.misses());
        assert!(calls >= 3, "{calls}");

        let (second, ..) =
            analyze_with_judge_detailed(SAMPLE, &config, &judge, 8, ScopeOverrides::default());
        assert_eq!(second, first);
        assert_eq!(
            *judge.inner().0.lock().unwrap(),
            calls,
            "no new model calls"
        );

        // Saved and loaded, the cache serves a fresh judge too.
        let dir = std::env::temp_dir().join(format!("math-judge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cache.json");
        judge.save(&path).unwrap();
        let fresh = CachedJudge::new(Counting(Mutex::new(0)), "test-1")
            .load(&path)
            .unwrap();
        analyze_with_judge_detailed(SAMPLE, &config, &fresh, 8, ScopeOverrides::default());
        assert_eq!(fresh.misses(), 0);
        // Another model version shares nothing.
        let other = CachedJudge::new(Counting(Mutex::new(0)), "test-2")
            .load(&path)
            .unwrap();
        analyze_with_judge_detailed(SAMPLE, &config, &other, 8, ScopeOverrides::default());
        assert!(other.misses() > 0);

        // The replay file rebuilds the document with no judge at all.
        let replay = Replay::new("test-1", overrides, log);
        let replay_path = dir.join("replay.json");
        replay.save(&replay_path).unwrap();
        let loaded = Replay::load(&replay_path).unwrap();
        assert_eq!(loaded, replay);
        let replayed = analyze_with_overrides(SAMPLE, &config, &loaded.overrides);
        assert_eq!(replayed.document, first);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cache_keys_are_stable() {
        assert_eq!(
            cache_key("m", BlockType::Theorem, "x"),
            cache_key("m", BlockType::Theorem, "x")
        );
        assert_ne!(
            cache_key("m", BlockType::Theorem, "x"),
            cache_key("m", BlockType::Lemma, "x")
        );
        assert_ne!(
            cache_key("ab", BlockType::Theorem, "c"),
            cache_key("a", BlockType::Theorem, "bc")
        );
        assert_eq!(cache_key("m", BlockType::Theorem, "x").len(), 32);
    }

    #[test]
    fn replies_are_parsed_out_of_prose_and_fences() {
        let r = parse_reply("Sure:\n```json\n{\"verdict\": {\"retype\": \"lemma\"}, \"confidence\": 0.8}\n```", "b").unwrap();
        assert_eq!(r.verdict, Verdict::Retype(BlockType::Lemma));
        assert_eq!(r.confidence, Some(0.8));
        assert_eq!(parse_reply("{\"verdict\":\"accept\"}", "b").unwrap().verdict, Verdict::Accept);
        assert!(parse_reply("no idea", "b").is_err());
        assert!(parse_reply("{\"verdict\":\"maybe\"}", "b").is_err());
        let config = ApiConfig {
            provider: ApiProvider::Anthropic,
            base_url: String::new(),
            model: "m".into(),
            api_key: "secret".into(),
        };
        assert!(!format!("{config:?}").contains("secret"));
    }

    #[cfg(unix)]
    #[test]
    fn command_judge_talks_json_lines() {
        let dir = std::env::temp_dir().join(format!("math-judge-cmd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Rejects every remark, accepts the rest; ignores the request format
        // beyond the id and kind.
        let script = dir.join("judge.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nwhile IFS= read -r line; do\n\
             id=$(printf '%s' \"$line\" | sed 's/.*\"id\":\"\\([^\"]*\\)\".*/\\1/')\n\
             case \"$line\" in\n\
             *'\"kind\":\"remark\"'*) printf '{\"id\":\"%s\",\"verdict\":\"reject\"}\\n' \"$id\" ;;\n\
             *) printf '{\"id\":\"%s\",\"verdict\":\"accept\",\"confidence\":1}\\n' \"$id\" ;;\n\
             esac\ndone\n",
        )
        .unwrap();
        let judge = CommandJudge::new("/bin/sh", vec![script.display().to_string()], "sh-1");
        let config = ScoringConfig::default();
        let doc = math_core::analyze_with_judge(
            "Theorem 1. A.\n\nNote: this is just prose.",
            &config,
            &judge,
            4,
        );
        let kinds: Vec<_> = doc.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [BlockType::Theorem, BlockType::Text],
            "{:?}",
            judge.errors()
        );
        assert!(judge.errors().is_empty(), "{:?}", judge.errors());

        // A program that cannot run leaves the heuristic result alone.
        let broken = CommandJudge::new(dir.join("missing"), Vec::new(), "none");
        let doc = math_core::analyze_with_judge("Theorem 1. A.", &config, &broken, 4);
        assert_eq!(doc.blocks[0].kind, BlockType::Theorem);
        assert_eq!(broken.errors().len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
