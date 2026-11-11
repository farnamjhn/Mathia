//! End-to-end classification of realistic Persian and English lecture notes.

use math_core::{BlockType, Direction, HtmlOptions, MathAssets, ScoringConfig, analyze, compile};

const ANALYSIS_FA: &str = include_str!("fixtures/analysis_fa.md");
const ALGEBRA_EN: &str = include_str!("fixtures/algebra_en.md");

/// (id, type, number of Markdown nodes) for every semantic block, in order.
fn semantic(md: &str) -> Vec<(String, BlockType, usize)> {
    analyze(md, &ScoringConfig::default())
        .blocks
        .into_iter()
        .filter(|b| b.kind.is_semantic())
        .map(|b| (b.id, b.kind, b.span.len()))
        .collect()
}

/// The HTML of all plain-text blocks, with cross-reference links unwrapped.
fn plain_text(doc: &math_core::Document) -> String {
    let html: String = doc
        .blocks_of(BlockType::Text)
        .map(|b| b.html.as_str())
        .collect();
    let mut out = String::new();
    let mut rest = html.as_str();
    while let Some(open) = rest.find("<a class=\"mx-ref\"") {
        out.push_str(&rest[..open]);
        rest = &rest[open + rest[open..].find('>').unwrap() + 1..];
    }
    out.push_str(rest);
    out.replace("</a>", "")
}

fn expect(rows: &[(&str, BlockType, usize)]) -> Vec<(String, BlockType, usize)> {
    rows.iter()
        .map(|(id, kind, len)| (id.to_string(), *kind, *len))
        .collect()
}

#[test]
fn persian_analysis_notes() {
    use BlockType::*;
    assert_eq!(
        semantic(ANALYSIS_FA),
        expect(&[
            ("block-definition-1", Definition, 1),
            ("block-example-1", Example, 1),
            ("block-theorem-1", Theorem, 1),
            ("block-proof-1", Proof, 4),
            ("block-lemma-1", Lemma, 1),
            ("block-proof-2", Proof, 1),
            ("block-corollary-1", Corollary, 1),
            ("block-remark-1", Remark, 1),
        ])
    );
}

#[test]
fn persian_labels_and_direction() {
    let doc = analyze(ANALYSIS_FA, &ScoringConfig::default());
    assert_eq!(doc.direction, Direction::Rtl);
    assert_eq!(doc.title.as_deref(), Some("آنالیز ریاضی — جلسهٔ سوم"));

    let theorem = doc.blocks_of(BlockType::Theorem).next().unwrap();
    assert_eq!(theorem.number.as_deref(), Some("۳.۳"));
    assert_eq!(theorem.name.as_deref(), Some("هاینه–کانتور"));
    assert_eq!(theorem.title, "قضیه ۳.۳ (هاینه–کانتور)");
    assert_eq!(theorem.direction, Direction::Rtl);

    // The proof runs through the Halmos mark and no further.
    let proof = doc.blocks_of(BlockType::Proof).next().unwrap();
    assert!(proof.html.contains("blacksquare"));
    assert!(!proof.html.contains("طبق قضیه"));
}

#[test]
fn persian_prose_using_keywords_is_not_a_block() {
    let doc = analyze(ANALYSIS_FA, &ScoringConfig::default());
    let text = plain_text(&doc);
    for prose in [
        "قضیهٔ هاینه–کانتور را ثابت می‌کنیم",
        "نتیجه می‌گیریم",
        "طبق قضیه ۳.۳",
    ] {
        assert!(text.contains(prose), "should be plain text: {prose}");
    }
    // The structural heading "قضیهٔ اصلی" is a section, not a theorem.
    let headings: Vec<_> = doc
        .blocks_of(BlockType::Heading)
        .map(|b| b.title.as_str())
        .collect();
    assert_eq!(
        headings,
        ["آنالیز ریاضی — جلسهٔ سوم", "پیوستگی یکنواخت", "قضیهٔ اصلی"]
    );
}

#[test]
fn english_algebra_notes() {
    use BlockType::*;
    assert_eq!(
        semantic(ALGEBRA_EN),
        expect(&[
            ("block-definition-1", Definition, 2),
            ("block-example-1", Example, 1),
            ("block-theorem-1", Theorem, 3),
            ("block-proof-1", Proof, 2),
            ("block-corollary-1", Corollary, 1),
            ("block-proposition-1", Proposition, 1),
            ("block-proof-2", Proof, 1),
            ("block-lemma-1", Lemma, 1),
            ("block-remark-1", Remark, 1),
        ])
    );

    let doc = analyze(ALGEBRA_EN, &ScoringConfig::default());
    assert_eq!(doc.direction, Direction::Ltr);
    let text = plain_text(&doc);
    for prose in [
        "Definitions vary",
        "Note that the operation",
        "By Theorem 1.3",
    ] {
        assert!(text.contains(prose), "should be plain text: {prose}");
    }
}

#[test]
fn table_of_contents_nests_blocks_under_sections() {
    let doc = analyze(ALGEBRA_EN, &ScoringConfig::default());
    let toc: Vec<_> = doc
        .toc
        .iter()
        .map(|e| (e.depth, e.title.as_str()))
        .collect();
    assert_eq!(
        toc,
        [
            (1, "Group Theory Notes"),
            (2, "Basic notions"),
            (3, "Definition 1.1"),
            (3, "Example 1.2"),
            (2, "Lagrange"),
            (3, "Theorem 1.3 (Lagrange)"),
            (3, "Proof"),
            (3, "Corollary 1.4"),
            (3, "Proposition 1.5"),
            (3, "Proof"),
            (3, "Lemma 1.6"),
            (3, "Remark"),
        ]
    );
    // Every entry points at a real block.
    for entry in &doc.toc {
        assert!(doc.blocks.iter().any(|b| b.id == entry.id));
    }
}

#[test]
fn classification_is_deterministic() {
    let config = ScoringConfig::default();
    assert_eq!(analyze(ANALYSIS_FA, &config), analyze(ANALYSIS_FA, &config));
    let options = HtmlOptions {
        math: MathAssets::None,
        ..Default::default()
    };
    assert_eq!(
        compile(ALGEBRA_EN, &config, &options).unwrap(),
        compile(ALGEBRA_EN, &config, &options).unwrap()
    );
}

#[test]
fn compiled_html_has_a_hook_for_every_block() {
    let config = ScoringConfig::default();
    let doc = analyze(ANALYSIS_FA, &config);
    let html = compile(ANALYSIS_FA, &config, &HtmlOptions::default()).unwrap();
    for block in &doc.blocks {
        assert!(
            html.contains(&format!("data-block-id=\"{}\"", block.id)),
            "{}",
            block.id
        );
    }
    assert_eq!(html.matches("data-block-actions=").count(), 8);
}

#[test]
fn references_link_to_the_blocks_they_name() {
    let doc = analyze(ANALYSIS_FA, &ScoringConfig::default());
    let text: String = doc
        .blocks_of(BlockType::Text)
        .map(|b| b.html.as_str())
        .collect();
    assert!(
        text.contains(r##"طبق <a class="mx-ref" href="#block-theorem-1">قضیه ۳.۳</a> می‌توان"##)
    );

    let doc = analyze(ALGEBRA_EN, &ScoringConfig::default());
    let all: String = doc.blocks.iter().map(|b| b.html.as_str()).collect();
    assert!(
        all.contains(r##"By <a class="mx-ref" href="#block-theorem-1">Theorem 1.3</a> the order"##)
    );
    // Inside the proof of Proposition 1.5, both the proposition and the corollary.
    let proof = doc.blocks_of(BlockType::Proof).nth(1).unwrap();
    assert!(
        proof
            .html
            .contains(r##"<a class="mx-ref" href="#block-corollary-1">Corollary 1.4</a>"##)
    );
    assert!(proof.html.contains(
        r##"Proof of <a class="mx-ref" href="#block-proposition-1">Proposition 1.5</a>"##
    ));
    // A block's own label is never a link.
    let theorem = doc.blocks_of(BlockType::Theorem).next().unwrap();
    assert!(!theorem.html.contains("mx-ref"));
}

#[test]
fn proofs_are_linked_to_statements() {
    let doc = analyze(ALGEBRA_EN, &ScoringConfig::default());
    let proves: Vec<_> = doc
        .blocks_of(BlockType::Proof)
        .map(|b| b.proves.as_deref())
        .collect();
    assert_eq!(
        proves,
        [Some("block-theorem-1"), Some("block-proposition-1")]
    );
    assert!(
        doc.blocks
            .iter()
            .filter(|b| b.kind != BlockType::Proof)
            .all(|b| b.proves.is_none())
    );
}
