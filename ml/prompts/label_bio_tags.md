# Label Math Notes with BIO Tags

You are labeling a corpus of real mathematical notes for a sequence tagger.
Each input line is one JSON document:

```json
{"doc_id": "...", "language": "en" | "fa", "title": "...",
 "nodes": [{"index": 0, "kind": "heading", "flat_text": "...", "env_span": null}]}
```

`nodes` are in reading order. `kind` is one of `paragraph`, `heading`,
`display_math`, `list`, `list_item`, `quote`, `rule`, `table`, `code`, `other`.
`env_span`, when present, is an explicit LaTeX environment boundary
(`{"kind": ..., "name": ..., "len": ...}`). `flat_text` is the node's text with
LaTeX preserved.

## Task

Assign exactly one BIO tag per node, for every document. Tags describe the
semantic block each node belongs to: a theorem, lemma, proof, definition, etc.,
including its multi-paragraph body and any display math that belongs to it.

## Valid tag vocabulary (exactly these 25 tags)

`O`, and `B-<TYPE>` / `I-<TYPE>` for each of:

`THEOREM`, `LEMMA`, `PROPOSITION`, `COROLLARY`, `DEFINITION`, `PROOF`,
`EXAMPLE`, `REMARK`, `EXERCISE`, `SOLUTION`, `CLAIM`, `NOTATION`.

Use uppercase type names exactly as written. No other tags exist.

## Hard structural rules

1. **Alignment.** `len(bio_tags)` must equal `len(nodes)` for every document.
   Tag `i` belongs to the node whose `index` is `i`; never skip or reorder.
2. **Legal transitions only.** `I-X` may immediately follow only `B-X` or
   `I-X`. It may never follow `O` or a different type (`B-Y` / `I-Y`).
   Start every block with `B-X`.
3. **Non-openable kinds.** Nodes with `kind` in
   `display_math`, `table`, `code`, `rule`, `list`, `other` must **never** be
   tagged `B-*`. They may only be `I-*` (continuing an active block) or `O`.
   A `list_item` may open a block when it is the labeled opener.
4. **References are not blocks.** Prose that mentions a block is `O`, never
   `B-*`. This includes `"Theorem 2 implies ..."`, `"By Lemma 3 ..."`,
   `"طبق قضیه ۳.۲ ..."`, `"نتیجه می‌گیریم ..."`, `"لم ۲ را ثابت می‌کنیم"`.
   The block opens at the node that *states* the result, not at a node that
   cites it.
5. **Headings are `O`** unless the heading itself names a single semantic
   block and the following node is that block's statement, e.g.
   `## Theorem 2.1 (Rolle)` followed by the statement. A section title such as
   `## Compactness` is always `O`.
6. **Scope by mathematical meaning.** Multi-paragraph statements, display math
   belonging to a block, keywordless proofs immediately following a
   theorem/lemma, and proofs without a QED marker all belong to their block.
   End the block where the mathematics ends: do not let a proof swallow
   unrelated exposition, a new example, a new section, or a trailing
   reference that follows it.
7. **Respect `env_span`.** When a node has `env_span`, the explicit LaTeX
   environment boundary is authoritative: nodes inside it belong to that
   block.

## Few-shot examples

### English

Input:

```json
{"doc_id": "rolle_en", "language": "en", "title": "Rolle's Theorem", "nodes": [
  {"index": 0, "kind": "heading", "flat_text": "Rolle's Theorem", "env_span": null},
  {"index": 1, "kind": "paragraph", "flat_text": "**Theorem 2.1 (Rolle).** Let $f$ be continuous on $[a,b]$, differentiable on $(a,b)$, and $f(a)=f(b)$. Then there is $c \\in (a,b)$ with", "env_span": null},
  {"index": 2, "kind": "display_math", "flat_text": "$$f'(c) = 0$$", "env_span": null},
  {"index": 3, "kind": "paragraph", "flat_text": "**Proof.** Since $[a,b]$ is compact, $f$ attains its extrema. If both are equal, $f$ is constant. Otherwise an interior extremum exists, where the derivative vanishes. $\\blacksquare$", "env_span": null},
  {"index": 4, "kind": "paragraph", "flat_text": "Theorem 2.1 implies the mean value theorem after subtracting the chord.", "env_span": null}
]}
```

Output:

```json
{"doc_id": "rolle_en", "bio_tags": ["O", "B-THEOREM", "I-THEOREM", "B-PROOF", "O"]}
```

### Persian

Input:

```json
{"doc_id": "rolle_fa", "language": "fa", "title": "قضیه رول", "nodes": [
  {"index": 0, "kind": "heading", "flat_text": "قضیه رول", "env_span": null},
  {"index": 1, "kind": "paragraph", "flat_text": "**قضیه ۳.۲ (رول).** فرض کنید $f$ روی $[a,b]$ پیوسته و روی $(a,b)$ مشتق‌پذیر باشد و $f(a)=f(b)$. آنگاه نقطه‌ای مانند $c \\in (a,b)$ وجود دارد که", "env_span": null},
  {"index": 2, "kind": "display_math", "flat_text": "$$f'(c) = 0$$", "env_span": null},
  {"index": 3, "kind": "paragraph", "flat_text": "**اثبات.** چون $[a,b]$ فشرده است، $f$ اکسترمم مطلق خود را می‌گیرد. اگر ماکزیمم و مینیمم برابر باشند تابع ثابت است؛ در غیر این صورت در نقطه‌ای درونی مشتق صفر می‌شود. $\\blacksquare$", "env_span": null},
  {"index": 4, "kind": "paragraph", "flat_text": "طبق قضیه ۳.۲ نتیجه می‌گیریم که قضیه مقدار میانگین برقرار است.", "env_span": null}
]}
```

Output:

```json
{"doc_id": "rolle_fa", "bio_tags": ["O", "B-THEOREM", "I-THEOREM", "B-PROOF", "O"]}
```

## Output format

Return **only** valid JSONL — no prose, no explanations, no markdown fences.
One line per input document, in the same order as the input, using exactly this
compact shape:

```json
{"doc_id": "<exact doc_id from the input>", "bio_tags": ["O", "B-THEOREM", "I-THEOREM", "..."]}
```

Every input document must appear exactly once, with its `doc_id` copied
verbatim and one tag per node.
