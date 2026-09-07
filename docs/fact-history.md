# Keeping changed facts inspectable in a local Rust memory layer

An agent can retrieve a statement that was correct yesterday and wrong today. Storing another
paragraph is easy; deciding which statement is current is a separate problem. `agent-memory`
exposes explicit memory supersession and relationship replacement so the application can make
that decision and inspect what remains afterward.

This walkthrough uses structured application decisions, not automatic contradiction detection.
It is based on the runnable [fact-history example](../examples/fact_history.rs).

## 1. Keep memory records and graph triples distinct

A memory record contains text, a memory type, a scope/key and retrieval metadata. A graph
triple contains a subject, predicate and object. Those are different representations:

```rust
let old = memory.remember_fact(Scope::User, "alice", "Alice lives in Beijing")?;
memory.replace_relation("alice", "lives_in", "beijing")?;
```

The memory API stores text for recall. The second call expresses a single-valued relationship.
No extractor is enabled here, so the graph update is deliberately explicit. Manual facade
relationship calls do not assign a source-memory ID. For automatic extraction, attach an
`Extractor`; extracted triples use `GraphStore::add_triple` and carry the source memory ID.
That automatic path adds relationships rather than deciding which old one to replace.

## 2. State the application's update decision

Suppose an application has established that Alice moved to Shanghai:

```rust
memory.supersede(Scope::User, "alice", old.id, "Alice lives in Shanghai")?;
memory.replace_relation("alice", "lives_in", "shanghai")?;
```

`supersede` links the old text record to its replacement; normal recall filters the old record.
`replace_relation` invalidates active edges for the same subject/predicate pointing elsewhere,
then writes or reactivates the specified triple. `remember_relation`, in contrast, permits
coexisting objects, useful for a person who uses several programming languages.

This distinction belongs in the application, not in a similarity threshold. A nearby vector
does not establish that one fact contradicts another. The default feature-hash embedder is
especially not a learned semantic model.

## 3. Make each graph change atomic, without overclaiming the whole pipeline

In the SQLite implementation, entity upserts, invalidation and triple upsert are performed in
one database transaction in [`SqliteGraphStore::write_triple`](../src/graph.rs). Readers should
not observe the middle of that individual graph mutation.

That does **not** make the two application calls above one transaction. `supersede` itself
involves multiple memory-store operations. A failure can leave partial application state.
Keeping memory and graph tables in one file is not the same as sharing one transaction across
the entire public API call sequence. Applications must inspect errors and define recovery.

The optional Neo4j adapter uses a namespace metadata-node write lock within its mutation
transactions to coordinate adapter clients. It does not create a distributed transaction
with SQLite. Direct external Cypher writes also do not participate in that adapter protocol.
See the [Neo4j deployment notes](neo4j.en.md).

## 4. Test reopening in separate processes

An in-memory assertion can miss a persistence bug. The example runs three independent copies
of itself against one freshly created temporary SQLite file:

1. `seed`: store the initial text and relationships, then exit.
2. `move`: reopen the file, verify the original memory, update text/graph, then exit.
3. `inspect`: reopen once more and assert the current record, superseded record and graph path.

```bash
git clone https://github.com/Ricardo-M-L/agent-memory.git
cd agent-memory
cargo run --example fact_history
```

The final process checks that the current memory says Shanghai, one Beijing relationship is
invalidated, and the active path is `alice → shanghai → china`. It also checks that a different
memory scope key retrieves no Alice records. **That last assertion is about memory, not graph
isolation:** all scopes attached to a graph store still share its graph.

The demo uses a unique directory created exclusively for that run. Cleanup removes only its
known SQLite files. Program assertions make an incorrect demo fail instead of printing success.
The CI workflow executes this example as well as the ordinary test suite.

## 5. Say what the retained history cannot answer

Use `get(old_id)` or `store().list(...)` to inspect superseded memory records; facade `list`
filters them. Use `graph_edges(true)` to include invalidated edges.

The graph stores one record per distinct triple. Reactivating a previous triple reuses it and
updates fields, so this does not preserve every historical activation interval. It is not
point-in-time graph replay, a complete audit log, or a bitemporal database.

Path search follows directed edges and runs in Rust after loading the graph's active edges,
even when storage is Neo4j. A two-hop demo is evidence of the API contract, not evidence that
the implementation scales to millions of edges.

## Try a small integration

Start with the [quickstart](../examples/quickstart.rs) and [integration guide](guide.md). The
default runtime needs no model, API key or external server; the first Cargo build still needs
its dependencies. The project is an early-stage MIT-licensed Rust library and CLI. Reports
from real integrations, regression tests and documentation improvements are welcome through
the [contribution guide](../CONTRIBUTING.md).

---

**Authorship disclosure:** this article was written by an AI coding assistant as part of the
maintainer-authorized project launch work. Code references and the runnable demonstration are
provided so readers can inspect and reproduce the claims. No independent human review is implied.
