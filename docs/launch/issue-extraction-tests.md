## Why

Rule extraction is an intentionally limited offline fallback. Existing tests cover common Chinese
and English patterns, but users need a clear matrix of case, punctuation and whitespace behavior.

**Difficulty: easy.** A bounded tests-and-documentation task; no model, network or Neo4j required.

## Scope

Add table-driven regression tests around `RuleExtractor` for leading/trailing whitespace, mixed
English case, sentence-final punctuation, repeated punctuation, and non-ASCII names. First record
the current expected behavior. Document unsupported forms instead of silently expanding the parser.
Discuss behavior changes separately if a genuine bug is uncovered.

## Acceptance criteria

- At least one positive and one negative/unsupported example for relevant categories, with exact
  expected triples and case normalization assertions.
- Tests are deterministic and offline; `cargo test` and all-feature Clippy remain green.
- Add a small supported/unsupported example table in `docs/guide.md` and link it from the Chinese README.
- Do not describe pattern matching as general NER or model-level semantic understanding.

Start in `src/extract.rs` and `docs/guide.md`. Comment before working to avoid duplication.

[Contribution instructions](https://github.com/Ricardo-M-L/agent-memory/blob/main/CONTRIBUTING.md).
Project license: MIT; no additional contributor agreement.
