# Optional Neo4j graph backend

[中文完整说明](neo4j.md) · [Integration guide](guide.md)

The `neo4j` feature provides `Neo4jGraphStore`, implementing `GraphStore` through Neo4j's
[HTTP Query API](https://neo4j.com/docs/query-api/current/query/), `/db/{database}/query/v2`.
Entities and relationships are stored in Neo4j. Memory text and vectors remain in the memory
store (SQLite by default). No APOC, GDS or Bolt client is required.

The integration targets Neo4j 5.26+; CI uses Community 5.26.30. It connects to a single service
address, without cluster routing or bookmarks. Default builds do not enable this backend.

## Start a disposable local service

In a terminal, with Docker installed:

```bash
export NEO4J_PASSWORD=agent-memory-demo-only
docker run --rm --name agent-memory-neo4j \
  -p 127.0.0.1:7474:7474 -p 127.0.0.1:7687:7687 \
  -e NEO4J_AUTH="neo4j/$NEO4J_PASSWORD" \
  neo4j:5.26.30
```

This password is only for a local disposable demo. Ports bind to loopback. Container data is
discarded when the container stops; use a separately planned volume/backup policy for durable use.
After the server reports `Started`, run this in another terminal inside the repository:

```bash
export NEO4J_PASSWORD=agent-memory-demo-only
cargo run --features neo4j --example neo4j
```

The example accepts `NEO4J_ENDPOINT`, `NEO4J_USERNAME`, `NEO4J_PASSWORD`, `NEO4J_DATABASE`,
`NEO4J_NAMESPACE` and `MEMORY_DB`. Defaults are localhost:7474, username/database `neo4j`,
namespace `agent-memory-demo`, and local memory file `neo4j-demo.db`. The library itself does
not read environment variables. Use HTTPS and dedicated credentials for remote services.

## Attach the store from Rust

Enable `features = ["neo4j"]` on the Git dependency shown in the [README](../README.md).

```rust
use std::sync::Arc;
use agent_memory::{AgentMemory, Neo4jGraphConfig, Neo4jGraphStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph = Neo4jGraphStore::connect(
        Neo4jGraphConfig::basic(
            "http://localhost:7474", "neo4j", std::env::var("NEO4J_PASSWORD")?
        ).with_database("neo4j").with_namespace("my-agent"),
    )?;
    let memory = AgentMemory::open("agent-memory.db")?.with_graph(Arc::new(graph));
    memory.replace_relation("alice", "lives_in", "shanghai")?;
    Ok(())
}
```

`connect` creates uniqueness constraints and namespace metadata idempotently, requiring schema
permissions. An administrator can initialize first; runtime callers can use `new(config)` on
the initialized namespace. `new` validates configuration, not connectivity. Initialization
retries only confirmed deadlock errors, up to three times; ordinary data writes are not retried.

## View the graph

Open Neo4j Browser at `http://localhost:7474`, sign in, and run:

```cypher
MATCH (s:AgentMemoryEntity {namespace: 'agent-memory-demo'})
      -[r:AGENT_MEMORY_RELATION]->(o:AgentMemoryEntity {namespace: 'agent-memory-demo'})
WHERE r.invalidated_at IS NULL
RETURN s, r, o
```

Use `name` for node captions and `predicate` for edge captions. Relation kinds are stored as
properties, not interpolated into Cypher syntax. Entity types may be empty: `Triple` does not
currently carry typed-entity extraction. IDs are application IDs, not Neo4j internal IDs.

## Important boundaries

- Mutations use database transactions and a namespace metadata-node lock. Concurrent writers
  using this adapter follow that lock; direct external Cypher mutations do not.
- Namespaces do not automatically follow memory user/session/agent scopes and are not a
  permission system. Map each SQLite memory database to an independent graph namespace to
  avoid source-memory-ID collisions.
- Memory and graph writes are not a distributed transaction. Text and some extracted triples
  may already be saved when a later write fails. A timeout can leave the write outcome unknown.
- `add_triple` leaves an invalidated duplicate invalidated. `replace_triple` can reactivate it,
  retaining its ID and updating its source/time/confidence. This is not a complete event log.
- Traversal loads the namespace's active edges and computes in Rust. Statistics/components
  load entities and edges; these are not large-graph or strict-snapshot guarantees.
- No automatic SQLite graph migration, graph pagination, native Cypher traversal, or CLI backend
  selection. Keep depth/graph size bounded. Default HTTP timeout is 30 seconds and response cap
  is 16 MiB; redirects are rejected and credentials/raw server errors are redacted.

## Tests

Normal `cargo test --all-features` does not contact Neo4j; live tests are explicitly ignored.
To run the live tests against a disposable writable server:

```bash
export NEO4J_ENDPOINT=http://localhost:7474
export NEO4J_PASSWORD=agent-memory-demo-only
export NEO4J_TEST_ALLOW_WRITE=1
cargo test --all-features --test neo4j_integration -- --ignored
```

Tests use random namespaces, clean only their own data on success, and retain a failing
namespace for diagnosis. CI runs these in a separate Neo4j service job.

When finished with the named disposable container, `docker stop agent-memory-neo4j` stops and
removes it. It does not remove the example's separate local SQLite file.
