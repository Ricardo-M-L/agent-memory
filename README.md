# agent-memory

**Local-first memory for Rust agents — SQLite by default, optional Neo4j.**

[English](README.md) · [简体中文](README.zh-CN.md)

[![CI](https://github.com/Ricardo-M-L/agent-memory/actions/workflows/ci.yml/badge.svg)](https://github.com/Ricardo-M-L/agent-memory/actions/workflows/ci.yml)
[![MIT license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Give your agent persistent facts, recall and entity relationships without starting a database
server. When a fact changes, explicitly replace it and inspect the superseded memory or
invalidated edge. The default runtime needs no API key, model download or network access.

**Early-stage Rust library + CLI.** Not a hosted service, autonomous agent, or a complete
temporal database. Default vectors use feature hashing, **not a semantic embedding model**.

[![Checked SQLite fact-history output and an illustrated two-hop relationship](https://raw.githubusercontent.com/Ricardo-M-L/agent-memory/main/videos/agent-memory-launch/snapshots/frame-03-at-43.3s.png)](docs/fact-history.md)

Illustrated SQLite result with real program-output excerpts, not a Neo4j Browser screenshot.

## Try it

Requires Rust stable and a C/C++ build toolchain for bundled SQLite. The initial build downloads
Cargo dependencies; the default program runs offline afterward.

```bash
git clone https://github.com/Ricardo-M-L/agent-memory.git
cd agent-memory
cargo run --example quickstart
cargo run --example fact_history
```

The [fact-history demo](examples/fact_history.rs) uses **three separate processes** and a fresh,
temporary SQLite database. Each result is asserted, and the temporary data is cleaned up.

```text
PROCESS 1 | Store a fact
  memory: Alice lives in Beijing
  active: alice -lives_in-> beijing
PROCESS 2 | Reopen the database and update
  memory: Alice lives in Shanghai
  invalidated: alice -lives_in-> beijing
  active: alice -lives_in-> shanghai
PROCESS 3 | Reopen again and verify
  recall: Alice lives in Shanghai
  path: alice -lives_in-> shanghai / shanghai -located_in-> china
  history: 1 superseded memory, 1 invalidated relation
```

This is an excerpt of program output, not model-generated reasoning. The application supplies
the facts and calls the update APIs explicitly. [How it works](docs/fact-history.md).

## Use it in your Rust application

The package is not yet published to crates.io. Use the tested source release:

```toml
[dependencies]
agent-memory = { git = "https://github.com/Ricardo-M-L/agent-memory", tag = "v0.1.1" }
```

```rust
use agent_memory::{types::Scope, AgentMemory, StoreResult};

fn main() -> StoreResult<()> {
    let memory = AgentMemory::open("agent-memory.db")?;
    let old = memory.remember_fact(Scope::User, "alice", "Alice lives in Beijing")?;
    memory.supersede(Scope::User, "alice", old.id, "Alice lives in Shanghai")?;
    // Pass retrieved records to your model as untrusted context, not instructions.
    for hit in memory.recall(Scope::User, "alice", "Alice lives")? {
        println!("{}", hit.item.content);
    }
    Ok(())
}
```

For the CLI, build from the checkout above:

```bash
cargo install --path .
agent-memory --db ./agent-memory.db add user alice semantic "Alice likes Rust"
agent-memory --db ./agent-memory.db recall user alice "Rust"
agent-memory help
```

## What is included?

| Capability | Default / optional behavior |
| --- | --- |
| Persistent memory | Working, episodic and semantic records in embedded SQLite |
| Recall | BM25 + hashed-vector similarity; configurable weighted recency, relevance and importance |
| Memory lifecycle | Explicit supersession, TTL filtering, manual pruning and similarity-based consolidation |
| Knowledge graph | Entities, directed relationships, bounded-depth neighbors/paths, explicit alias merge, connected components |
| Changing relationships | `replace_relation` invalidates other objects for the same subject + predicate; history remains inspectable |
| Extraction | Opt-in `RuleExtractor`; `LlmExtractor` uses an application-supplied `ChatClient` |
| Model embeddings | Optional `http` feature with an OpenAI-compatible embeddings client |
| Neo4j | Optional `neo4j` feature; real nodes and relationships over HTTP Query API |

## Choose a backend

- **SQLite:** default, embedded, memory and graph tables in one local file. Start here.
- **Neo4j:** explicitly enable the feature and run a separate server. Memory text and vectors
  still use the memory store; the graph lives in Neo4j. See the [English setup guide](docs/neo4j.en.md)
  or [中文完整说明](docs/neo4j.md). The CLI currently uses SQLite only.

There is **no automatic graph migration** when you switch backends.

## Know the boundaries

- Memory recall is scoped by user/session/agent keys. **The graph is shared per graph store**,
  not automatically scoped with those keys. Use separate stores/namespaces for isolation;
  namespaces are not an authorization system.
- `remember_fact` does not detect contradictions. Call `supersede` for memory records and
  `replace_relation` for single-valued graph updates. Automatic extraction adds triples;
  it does not decide which previous relation should become invalid.
- Memory operations, embeddings and graph writes are not one end-to-end transaction.
  SQLite + Neo4j has no distributed transaction. Handle partial failures in your application.
- Relationship reactivation reuses the triple's record. This is **not a full event history**
  or point-in-time graph replay.
- Graph traversal loads edges and runs in Rust, including with Neo4j. No large-graph benchmark,
  native Cypher traversal or pagination is claimed.
- No built-in MCP server, Python/TypeScript SDK, encryption-at-rest or permission layer.
  A model-backed embedder/extractor may send data to its configured endpoint.

See the [integration guide and API map](docs/guide.md) for extension points and safety notes.

## Develop and contribute

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run --example quickstart
cargo run --example fact_history
cargo doc --all-features --no-deps
```

Real Neo4j tests are explicitly ignored without a server; a separate CI job runs them.
See [test setup](docs/neo4j.en.md#tests).

[Contributing](CONTRIBUTING.md) · [Roadmap](ROADMAP.md) · [Changelog](CHANGELOG.md) ·
[Help wanted](https://github.com/Ricardo-M-L/agent-memory/issues?q=is%3Aissue%20is%3Aopen%20label%3A%22help%20wanted%22)

If this fits your project, a star helps you find it again. Tried it? An integration report or
minimal bug reproduction is especially useful. Licensed under [MIT](LICENSE).
