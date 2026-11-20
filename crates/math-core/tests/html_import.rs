//! End-to-end checks against the sample pages in `sample-data/`.
//!
//! The samples are optional: a checkout without them still runs the rest of
//! the suite. When they are present, these tests pin the behaviour that made
//! the importer necessary: no scripts or page chrome, visible proofs, and the
//! SVG figures kept intact.

use std::path::PathBuf;

use math_core::{BlockType, HtmlOptions, analyze_html, clean_html};

fn sample(name: &str) -> Option<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sample-data")
        .join(name);
    std::fs::read_to_string(path).ok()
}

#[test]
fn proof_sketch_labels_in_imported_html_become_proof_blocks() {
    let doc = analyze_html(
        "<main><ul><li><strong>ایده اثبات:</strong> دلیل اول.\
         <ul><li>گام دوم.</li></ul></li><li>مرور جلسهٔ بعد.</li></ul>\
         <p><strong>Proof sketch:</strong> Use induction.</p>\
         <p>Proof sketches are useful in lectures.</p></main>",
    );
    let proofs: Vec<_> = doc.blocks_of(BlockType::Proof).collect();
    assert_eq!(proofs.len(), 2);
    assert!(proofs[0].html.contains("گام دوم"));
    assert!(!proofs[0].html.contains("مرور جلسه"));
    assert!(proofs[1].html.contains("Use induction."));
    assert!(
        doc.blocks_of(BlockType::Text)
            .any(|b| b.html.contains("Proof sketches are useful"))
    );
}

#[test]
fn graph_theory_sample_keeps_only_content_and_svgs() {
    let Some(source) = sample("graph_theory.html") else {
        return;
    };
    let html = clean_html(&source, &HtmlOptions::default()).expect("renders");

    assert!(!html.contains("<script src="));
    assert!(!html.contains("onclick="));
    assert!(!html.contains("searchBox"));
    assert!(!html.contains("class=\"sidebar\""));
    assert!(!html.contains("class=\"chapter-meta\""));
    assert!(!html.contains("<details"));

    assert!(html.contains("<svg"));
    assert!(html.contains("<figure>"));
    assert!(html.contains("<figcaption>"));
    assert!(html.contains("data-mx-figure=\"true\""));
    assert!(html.contains("data-block-id="));
    // The content heading survives; the sidebar's brand heading does not.
    assert!(html.contains("<h1"));
    assert!(!html.contains("brand-title"));

    let doc = analyze_html(&source);
    assert!(
        doc.blocks
            .iter()
            .filter(|b| b.kind == math_core::BlockType::Heading)
            .count()
            > 10
    );
}

#[test]
fn linear_algebra_sample_removes_chrome_and_opens_proofs() {
    let Some(source) = sample("linear_algebra.html") else {
        return;
    };
    let html = clean_html(&source, &HtmlOptions::default()).expect("renders");

    assert!(!html.contains("<script src="));
    assert!(!html.contains("search-modal"));
    assert!(!html.contains("sidebar-filter"));
    assert!(!html.contains("copy-math-btn"));
    assert!(!html.contains("wiki-sidebar"));
    assert!(!html.contains("document-masthead"));
    assert!(!html.contains("contribution-banner"));
    assert!(!html.contains("<details"));
    assert!(!html.contains("proof-toggle-btn"));

    // The page's own headings survive, including the masthead <h1> that sits
    // outside the article but inside <main>.
    assert!(html.contains("<h1"));
    assert!(html.contains("مبانی ماتریس و جبر خطی"));
    assert!(html.matches("<h2").count() > 40);
    assert!(html.matches("<h3").count() > 30);
    assert!(!html.contains("masthead-meta"));
    assert!(!html.contains("contribution-banner"));

    // The proof bodies are now ordinary visible content.
    assert!(html.contains("برهان"));
    assert!(html.contains("class=\"math math-display\""));

    let doc = analyze_html(&source);
    assert!(
        doc.blocks
            .iter()
            .filter(|b| b.kind == math_core::BlockType::Heading)
            .count()
            > 50
    );
    assert!(
        doc.blocks
            .iter()
            .any(|b| b.kind == math_core::BlockType::Definition)
    );
    assert!(
        doc.blocks
            .iter()
            .any(|b| b.kind == math_core::BlockType::Theorem)
    );
}
