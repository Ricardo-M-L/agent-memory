## Why

The offline examples should be verifiable by contributors on Windows and macOS as well as Linux.
Currently the CI test job runs on Ubuntu only. This is a real missing adoption check, not a claim
that other systems are broken.

**Difficulty: easy–medium.** Suitable for a first CI contribution; some Rust/toolchain familiarity helps.

## Scope

Add a small Windows/macOS CI matrix for default-feature tests and the documented offline examples.
Keep the existing Linux all-feature and real Neo4j jobs intact. Do not start Neo4j on every OS or
add paid services. Prefer explicit shell handling and temporary-directory APIs over `/tmp` assumptions.

## Acceptance criteria

- Windows and macOS run `cargo test`, `cargo run --example quickstart`, and
  `cargo run --example fact_history` successfully.
- Existing Linux formatting, Clippy, package and live-Neo4j checks remain in place.
- Document any compiler prerequisites discovered. Do not claim a platform passed without CI evidence.
- No credentials, persistent test databases or machine-specific paths are committed.

Start in `.github/workflows/ci.yml`, `tests/cli.rs` and `examples/fact_history.rs`.
Comment if you would like to work on this so efforts do not overlap.

[Contribution instructions](https://github.com/Ricardo-M-L/agent-memory/blob/main/CONTRIBUTING.md).
Project license: MIT; no additional contributor agreement.
