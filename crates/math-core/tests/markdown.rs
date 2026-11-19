//! Exporting a document to Markdown must not change how Mathia reads it back.

use std::fs;
use std::path::Path;

use math_core::{BlockType, ScoringConfig, analyze, markdown, normalize_markdown};

/// (type, scope length) of every block, in order.
fn shape(markdown: &str) -> Vec<(BlockType, usize)> {
    analyze(markdown, &ScoringConfig::default())
        .blocks
        .into_iter()
        .map(|block| (block.kind, block.span.len()))
        .collect()
}

#[test]
fn every_fixture_survives_the_round_trip() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut checked = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        let doc = analyze(&source, &ScoringConfig::default());
        let exported = markdown::render(&doc, Some(&normalize_markdown(&source)));
        assert_eq!(
            shape(&exported),
            shape(&source),
            "{} changed shape after export:\n{exported}",
            path.display()
        );
        checked += 1;
    }
    assert!(
        checked >= 4,
        "expected the corpus fixtures, found {checked}"
    );
}
