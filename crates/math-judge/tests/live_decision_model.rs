//! Live checks of [`ApiJudge`] against a running System One decision server:
//! Ollama's `/v1/systemone`, TypeSafe's Jev, or anything speaking the same
//! API. Ignored by default because they need a model and a network; run them
//! against a local one with
//!
//! ```text
//! MATHIA_DECISION_URL=http://localhost:11434 MATHIA_DECISION_MODEL=d1-omni \
//!   cargo test -p math-judge --test live_decision_model -- --ignored --nocapture
//! ```

use math_core::{
    ScopeOverrides, ScoringConfig, analyze_with_judge_detailed, analyze_with_overrides,
};
use math_judge::{ApiConfig, ApiJudge, ApiProvider};

const SAMPLE: &str = "\
Groups are the main object of this chapter.

Theorem 1. Every finite group of prime order is cyclic.

Proof. Let $G$ be such a group and take $g \\ne e$. The subgroup $\\langle g \\rangle$ has order dividing $|G|$, so it is all of $G$.

Definition 2. A group is abelian when its operation commutes.

Remark 3. The converse of Theorem 1 fails: the Klein four-group is abelian but not cyclic.

This paragraph is running prose that mentions a theorem in passing.";

fn config() -> ApiConfig {
    let base_url =
        std::env::var("MATHIA_DECISION_URL").unwrap_or_else(|_| "http://localhost:11434".into());
    let model = std::env::var("MATHIA_DECISION_MODEL").unwrap_or_else(|_| "d1-omni".into());
    ApiConfig {
        provider: ApiProvider::TypeSafe,
        base_url,
        model,
        api_key: String::new(),
    }
}

#[test]
#[ignore = "needs a running decision server"]
fn the_endpoint_answers_the_contract() {
    let judge = ApiJudge::new(config());
    if let Err(message) = judge.test() {
        panic!("{message}\nerrors: {:?}", judge.errors());
    }
    println!("connected to {}", judge.model_version());
}

#[test]
#[ignore = "needs a running decision server"]
fn the_model_reviews_a_document() {
    let judge = ApiJudge::new(config());
    // Review every detected block, not just the low-confidence ones.
    let mut scoring = ScoringConfig::default();
    scoring.ml_confidence_threshold = 1e9;

    let (document, overrides, log) =
        analyze_with_judge_detailed(SAMPLE, &scoring, &judge, 4, ScopeOverrides::default());

    for verdict in &log {
        println!(
            "round {} {} {:?} {:?} applied={}",
            verdict.round, verdict.block, verdict.kind, verdict.verdict, verdict.applied
        );
    }
    println!(
        "blocks: {:?}",
        document.blocks.iter().map(|b| b.kind).collect::<Vec<_>>()
    );

    assert!(judge.errors().is_empty(), "{:?}", judge.errors());
    assert!(!log.is_empty(), "the model was asked nothing");

    // The recorded overrides must rebuild the judged document without the model.
    let replayed = analyze_with_overrides(SAMPLE, &scoring, &overrides);
    assert_eq!(replayed.document, document);
}
