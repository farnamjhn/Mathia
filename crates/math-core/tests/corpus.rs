//! Corpus tests over `tests/fixtures/*.md`.
//!
//! * **Snapshots** (`tests/snapshots/NAME.snap`) pin the full classification
//!   of every fixture: ids, types, scopes, end reasons and proof links. After
//!   an intended change, regenerate them with
//!   `MATHIA_UPDATE_SNAPSHOTS=1 cargo test -p math-core --test corpus` and
//!   review the diff.
//! * **Gold labels** (`tests/fixtures/NAME.gold`, written by hand, one
//!   `kind | opener prefix` per line) measure precision and recall per block
//!   type, independently of what the classifier currently outputs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use math_core::{Analysis, BlockType, DecisionTree, Scorer, ScoringConfig, analyze_detailed};

fn fixtures() -> Vec<(String, PathBuf)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut found: Vec<(String, PathBuf)> = fs::read_dir(&dir)
        .expect("tests/fixtures")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            (name, path)
        })
        .collect();
    found.sort();
    assert!(!found.is_empty());
    found
}

fn analyze_file(path: &Path) -> Analysis {
    analyze_file_with(path, &ScoringConfig::default())
}

fn analyze_file_with(path: &Path, config: &ScoringConfig) -> Analysis {
    let source = fs::read_to_string(path).unwrap();
    analyze_detailed(&source, config)
}

fn snapshot(analysis: &Analysis) -> String {
    let mut out = String::new();
    for block in &analysis.document.blocks {
        out.push_str(&format!(
            "{:<22} nodes {:>2}..{:<2} score {:.2} end={:<15} proves={:<18} {}\n",
            block.id,
            block.span.start,
            block.span.end,
            block.score,
            block.end.as_str(),
            block.proves.as_deref().unwrap_or("-"),
            block.title,
        ));
    }
    out
}

#[test]
fn classifications_match_their_snapshots() {
    let update = std::env::var_os("MATHIA_UPDATE_SNAPSHOTS").is_some();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    let mut stale = Vec::new();
    for (name, path) in fixtures() {
        let actual = snapshot(&analyze_file(&path));
        let file = dir.join(format!("{name}.snap"));
        if update {
            fs::create_dir_all(&dir).unwrap();
            fs::write(&file, &actual).unwrap();
            continue;
        }
        let expected = fs::read_to_string(&file).unwrap_or_default();
        if expected != actual {
            stale.push(format!(
                "--- {name}: expected\n{expected}+++ {name}: actual\n{actual}"
            ));
        }
    }
    assert!(
        stale.is_empty(),
        "snapshots differ (MATHIA_UPDATE_SNAPSHOTS=1 regenerates them):\n{}",
        stale.join("\n")
    );
}

fn slug_to_kind(slug: &str) -> BlockType {
    BlockType::SEMANTIC
        .into_iter()
        .find(|kind| kind.slug() == slug)
        .unwrap_or_else(|| panic!("unknown block type in gold file: {slug}"))
}

fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Default, Clone, Copy)]
struct Tally {
    hits: usize,
    predicted: usize,
    gold: usize,
}

impl Tally {
    fn precision(self) -> f32 {
        if self.predicted == 0 {
            1.0
        } else {
            self.hits as f32 / self.predicted as f32
        }
    }
    fn recall(self) -> f32 {
        if self.gold == 0 {
            1.0
        } else {
            self.hits as f32 / self.gold as f32
        }
    }
}

#[test]
fn precision_and_recall_per_type() {
    gold_precision_and_recall("linear", &ScoringConfig::default());
}

/// The decision-tree scorer must hold the same floor on the same corpus.
#[test]
fn decision_tree_precision_and_recall_per_type() {
    let config = ScoringConfig {
        scorer: Scorer::Tree(DecisionTree::default()),
        ..Default::default()
    };
    gold_precision_and_recall("tree", &config);
}

fn gold_precision_and_recall(scorer: &str, config: &ScoringConfig) {
    let mut tallies: BTreeMap<BlockType, Tally> = BTreeMap::new();
    let mut misses = Vec::new();

    for (name, path) in fixtures() {
        let gold_file = path.with_extension("gold");
        let gold_text = fs::read_to_string(&gold_file)
            .unwrap_or_else(|_| panic!("{name}.md has no hand-labelled {name}.gold"));
        let mut gold: Vec<(BlockType, String, bool)> = gold_text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                let (kind, prefix) = line.split_once('|').expect("kind | prefix");
                (slug_to_kind(kind.trim()), squash(prefix), false)
            })
            .collect();

        let analysis = analyze_file_with(&path, config);
        let doc = &analysis.document;
        for block in doc.blocks.iter().filter(|b| b.kind.is_semantic()) {
            let opener = squash(&doc.nodes[block.span.start].text);
            let tally = tallies.entry(block.kind).or_default();
            tally.predicted += 1;
            match gold.iter_mut().find(|(kind, prefix, used)| {
                !used && *kind == block.kind && opener.starts_with(prefix.as_str())
            }) {
                Some(entry) => {
                    entry.2 = true;
                    tally.hits += 1;
                }
                None => misses.push(format!("{name}: spurious {} — {opener}", block.kind.slug())),
            }
        }
        for (kind, prefix, used) in &gold {
            tallies.entry(*kind).or_default().gold += 1;
            if !used {
                misses.push(format!("{name}: missed {} — {prefix}", kind.slug()));
            }
        }
    }

    let mut total = Tally::default();
    println!("scorer: {scorer}");
    println!(
        "{:<12} {:>4} {:>4} {:>9} {:>6}",
        "type", "gold", "pred", "precision", "recall"
    );
    for (kind, tally) in &tallies {
        println!(
            "{:<12} {:>4} {:>4} {:>9.2} {:>6.2}",
            kind.slug(),
            tally.gold,
            tally.predicted,
            tally.precision(),
            tally.recall()
        );
        total.hits += tally.hits;
        total.predicted += tally.predicted;
        total.gold += tally.gold;
    }
    println!(
        "{:<12} {:>4} {:>4} {:>9.2} {:>6.2}",
        "all",
        total.gold,
        total.predicted,
        total.precision(),
        total.recall()
    );
    for miss in &misses {
        println!("{miss}");
    }

    // The floor every type must hold on the corpus. Raise it as the corpus
    // grows; never lower it to make a regression pass.
    const FLOOR: f32 = 0.9;
    for (kind, tally) in &tallies {
        assert!(
            tally.precision() >= FLOOR && tally.recall() >= FLOOR,
            "[{scorer}] {} fell below {FLOOR}: precision {:.2}, recall {:.2}\n{}",
            kind.slug(),
            tally.precision(),
            tally.recall(),
            misses.join("\n")
        );
    }
}
