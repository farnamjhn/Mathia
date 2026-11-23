//! Regression tests for the CRF block tagger on real compiled notes.
//!
//! The CRF used to run away on math-dense exposition: a strong proof
//! self-transition plus an over-broad `proof_cue` turned whole sections into a
//! single proof. These tests pin the behaviour that matters.

use math_core::{BlockType, ScoringConfig, analyze};

fn crf_config() -> ScoringConfig {
    ScoringConfig::default()
}

/// A definition followed by math-dense axioms, then a labelled proof. The
/// axioms are plain exposition and must not become a proof.
const FIELD_AXIOMS: &str = r#"
# Field Axioms and Subspaces

## Field Axioms

**Definition (Field).** A field is a non-empty set $F$ together with two operations $+ : F \times F \to F$ and $\cdot : F \times F \to F$.

For all $x, y, z \in F$ the following axioms hold.

Commutative laws: $x + y = y + x$ and $xy = yx$.

Associative laws: $x + (y + z) = (x + y) + z$ and $x(yz) = (xy)z$.

Distributive laws: $x(y + z) = xy + xz$.

There is an additive identity $0 \in F$ with $x + 0 = x$.

There is a multiplicative identity $1 \in F$ with $x \cdot 1 = x$.

**Theorem (Subspace Test).** A non-empty subset $W \subseteq V$ is a subspace if and only if $cu + v \in W$ for all $u, v \in W$ and $c \in F$.

**Proof.** Suppose $W$ is a subspace. Then it is closed under addition and scalar multiplication, so $cu + v \in W$. Conversely, taking $c = 0$ gives $0 \in W$, and taking $c = 1$ gives closure under addition.
"#;

#[test]
fn crf_does_not_swallow_exposition_into_a_proof() {
    let doc = analyze(FIELD_AXIOMS, &crf_config());

    let proofs: Vec<_> = doc
        .blocks_of(BlockType::Proof)
        .map(|b| b.span.clone())
        .collect();
    assert_eq!(
        proofs.len(),
        1,
        "expected exactly the labelled proof, got {proofs:?}"
    );

    // The axioms paragraph must stay plain text, not become a proof.
    let axiom_block = doc
        .blocks
        .iter()
        .find(|b| b.html.contains("Commutative laws"))
        .expect("axioms paragraph present");
    assert_eq!(
        axiom_block.kind,
        BlockType::Text,
        "math-dense axioms must not be tagged as a proof"
    );

    let kinds: Vec<BlockType> = doc.blocks.iter().map(|b| b.kind).collect();
    assert!(kinds.contains(&BlockType::Definition));
    assert!(kinds.contains(&BlockType::Theorem));
}

/// A theorem followed by a list-item proof and then a definition — taken from
/// `linear_algebra_notes_compiled.md`. The CRF emission model scored `O` above
/// `B-PROOF` on this math- and logic-dense proof opener, so the labelled
/// "برهان (Proof):" node was left as plain text. A labelled lead is
/// deterministic and must still open a proof block.
const PERSIAN_LABELLED_PROOF: &str = r#"
# زیرفضاها

**۴-۲ (یا ۵-۲)- قضیه اشتراک زیرفضاها (Intersection of Subspaces Theorem):** فرض کنید $V$ فضای برداری روی میدان $F$ باشد و $\{W_i\}_{i \in I}$ یک خانواده از زیرفضاهای (Subspaces) $V$ باشد؛ در این صورت اشتراک (Intersection) آن‌ها یعنی $\bigcap_{i \in I} W_i$ نیز زیرفضای $V$ است.

- **برهان (Proof):** توجه کنید برای هر $i \in I$ داریم $0_V \in W_i$ ، پس $0_V \in \bigcap_{i \in I} W_i \neq \emptyset$. فرض کنید $u, v \in \bigcap_{i \in I} W_i$ و $c \in F$. در این صورت:

    $$u, v \in W_i \quad (\forall i \in I) \stackrel{W_i \text{ زیرفضای } V}{\Longrightarrow} cu + v \in W_i \quad (\forall i \in I) \Longrightarrow cu + v \in \bigcap_{i \in I} W_i$$

    حکم از قضیه‌ی محک فشرده زیرفضا (۲-۲) نتیجه می‌شود.

**۵-۲- تعریف زیرفضای پدیدآمده یا اسپن (Definition of Span):** فرض کنید $V$ یک فضای برداری روی میدان $F$ باشد و $S \subseteq V$. **زیرفضای پدیدآمده توسط $S$ (Subspace Spanned by $S$ / Span of $S$)** را کوچکترین زیرفضایی از $V$ که شامل $S$ است تعریف می‌کنیم.
"#;

#[test]
fn crf_honours_labelled_persian_proof_opener() {
    let doc = analyze(PERSIAN_LABELLED_PROOF, &crf_config());

    let proofs: Vec<_> = doc
        .blocks_of(BlockType::Proof)
        .map(|b| b.span.clone())
        .collect();
    assert_eq!(
        proofs.len(),
        1,
        "the labelled proof must open its own block, got {proofs:?}"
    );
    let proof = doc.blocks_of(BlockType::Proof).next().unwrap();
    assert!(
        proof.html.contains("برهان"),
        "proof block should start at the labelled opener"
    );
}

/// A sentence that merely mentions a block ("قضیهٔ ۱۸-۵ را نباید…" = "Theorem
/// 18-5 should not be used…") is prose. The CRF used to open a spurious theorem
/// block on it; the reference demotion pass must keep it as text.
const PERSIAN_REFERENCE: &str = r#"
# دترمینان

**قضیه ۱۸-۵.** فرض کنید $A$ یک ماتریس مربعی روی میدان $F$ باشد.

قضیهٔ ۱۸-۵ را نباید برای محاسبهٔ تابع $\det$ وقتی که ماتریس بزرگ است به کار برد.
"#;

#[test]
fn crf_does_not_open_a_block_on_a_reference() {
    let doc = analyze(PERSIAN_REFERENCE, &crf_config());

    let theorems: Vec<_> = doc.blocks_of(BlockType::Theorem).collect();
    assert_eq!(
        theorems.len(),
        1,
        "the reference sentence must not open a theorem block"
    );
    assert!(
        !theorems[0].html.contains("را نباید"),
        "the reference must stay out of the theorem block"
    );
}
