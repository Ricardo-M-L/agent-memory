# v0.1.1 — local-first memory, optional Neo4j, reproducible examples

Early-stage Rust library and CLI. Default runtime: embedded SQLite, no model/API key/server.
Optional Neo4j stores real entities and relationships; memory text/vectors remain in the memory store.

## Try it from source

```bash
git clone --branch v0.1.1 https://github.com/Ricardo-M-L/agent-memory.git
cd agent-memory
cargo run --example quickstart
cargo run --example fact_history
```

Rust stable and a C/C++ toolchain are needed; the first build downloads Cargo dependencies.
The three-process example verifies persistence, explicit replacement, retained invalidated edges
and a two-hop path. It does not use an LLM. Its database is temporary and cleaned up.

## Changes

- Optional Neo4j Query API graph store with namespace coordination, atomic graph mutations,
  entity merge, relationship invalidation and live CI contract tests.
- SQLite graph replacement/restore transaction fixes and bounded confirmed-deadlock retry
  during Neo4j schema initialization.
- English/Chinese READMEs, runnable quickstart and fact-history examples, English deployment
  guide, contribution tasks and package/example verification in CI.

## Read before integrating

Default vectors are feature hashes, not model embeddings. Graph namespaces do not automatically
inherit memory scopes or supply authorization. Memory and graph operations are not a distributed
transaction. No automatic graph migration, complete event log, native Cypher traversal, large-graph
performance claim or built-in MCP server is included. CLI graph operations still use SQLite.

The package is not yet published to crates.io; source installation is the supported launch path.
See [the integration guide](https://github.com/Ricardo-M-L/agent-memory/blob/v0.1.1/docs/guide.md)
and [Neo4j setup](https://github.com/Ricardo-M-L/agent-memory/blob/v0.1.1/docs/neo4j.en.md).

欢迎提交真实接入反馈和最小问题复现。[中文首页](https://github.com/Ricardo-M-L/agent-memory/blob/v0.1.1/README.zh-CN.md)。
