# Integration guide

[Home](../README.md) · [中文首页](../README.zh-CN.md) · [Neo4j](neo4j.en.md)

## Where this fits in an agent

1. Your application decides which facts/events to persist and which scope key to use.
2. Before a model call, use `recall(scope, key, query)` to obtain scored memory records.
3. Select a bounded amount of retrieved text for the model's context. Treat it as untrusted data.
4. When the application knows a fact changed, call the relevant update API explicitly.

The library does not run a model or an agent loop for you. The [quickstart](../examples/quickstart.rs)
is a complete offline example of the memory side of this integration. Run it with
`cargo run --example quickstart`; use a file path with `AgentMemory::open` for persistence.

## API map

| Need | API |
| --- | --- |
| Open embedded storage | `AgentMemory::open(path)`; `:memory:` is ephemeral |
| Write a fact/event | `remember_fact` / `remember_event` |
| Choose memory type | `add(scope, key, MemoryType, content)` |
| Set importance/TTL/metadata | `add_with_importance` |
| Retrieve scored context | `recall`; `recall_between` filters by creation time |
| Replace a memory record | `supersede(scope, key, old_id, content)` |
| Inspect historical memory | `get(id)` or `store().list(...)`; facade `list` filters superseded/expired records |
| Delete / expire / consolidate | `forget` / `prune` / `consolidate` |
| Add coexisting graph facts | `remember_relation(subject, predicate, object)` |
| Update a single-valued graph fact | `replace_relation(subject, predicate, object)` |
| Inspect active/all edges | `graph_edges(false)` / `graph_edges(true)` |
| Traverse | `graph_neighbors(entity, depth)`; `graph_paths(from, to, max_depth)` |
| Merge an explicit alias | `merge_graph_entities(keep, alias)`; not automatic entity disambiguation |
| Connected components / statistics | `graph_communities` / `graph_summary` |
| Combine text and graph recall | `recall_with_graph`; graph scope caveat below still applies |

`graph_paths` follows directed edges; neighbors use undirected adjacency. The caller supplies
depth limits. Connected components are not semantic community detection or GraphRAG summarization.

## Retrieval and embeddings

Default relevance combines BM25 with cosine similarity over `HashEmbedder` vectors. Recency,
relevance and importance are combined by a **weighted sum** in `RetrievalConfig`. Hashing is a
cheap lexical baseline, not learned semantic understanding. There is no published accuracy or
latency benchmark. Similarity-based consolidation inherits the embedder's limitations.

To use a model, implement `Embedder` and construct `AgentMemory::with_store`, or enable the
optional `http` feature for `http_embed::OpenAiEmbedder`. Credentials and endpoint selection
belong to the application. Keep stored/query embedding dimensions and models consistent;
switching models does not automatically re-embed existing records. `with_store` has no graph
until you attach one with `with_graph`.

## Extraction and provenance

`AgentMemory::open` does not enable an extractor. Opt in using
`with_extractor(Arc::new(RuleExtractor::new()))`, or supply `LlmExtractor` with your `ChatClient`.
The built-in rules cover a small set of explicit Chinese/English sentence patterns; English
rule extraction lowercases names. This is not general named-entity recognition.

Extracted triples carry `source_memory_id` and are added with `GraphStore::add_triple`, allowing
multiple objects. Neither the extractor nor `remember_fact` chooses which previous fact to
invalidate. Use explicit update policies. Manual facade relationship methods have no source
memory ID; a caller needing provenance can retain a graph-store handle and use its trait methods.

## Storage, isolation and partial failures

- Scope keys filter memory lookup. They do not provide authorization: APIs such as `get(id)`
  are not scope-scoped. Enforce ownership checks in the application.
- All scopes attached to one graph store share its graph. For tenant isolation, route requests
  to separate stores (and independent Neo4j namespaces). A namespace is not an access-control rule.
- Individual graph mutations are transactional, but memory insertion, embedding storage,
  extraction, supersession and graph writes are not one combined transaction. On failure,
  inspect what persisted before attempting recovery. Neo4j and SQLite have no distributed commit.
- Invalidation keeps a triple record; reactivation reuses it. Do not treat it as a complete
  sequence of historical activations or a point-in-time graph query engine.
- Expired memories are filtered from recall/list. Call `prune` to physically clean expired
  records. There is no background scheduler. Deletion and working-memory eviction can remove data.

## Safety and deployment

Keep database files private and backed up. SQLite is not encrypted by this library. Never put
API keys or unnecessary personal information into memory, demos or issue reports. Retrieved
text can contain prompt injection; keep it separate from application/system instructions and
do not let a memory record authorize a tool call. External embeddings/LLM extraction can send
text to their configured service. Review its data policy before using sensitive content.

The default build has no network dependencies. Optional Neo4j needs a separate server;
[read its deployment constraints](neo4j.en.md). The CLI supports SQLite only.

## Local API documentation

```bash
cargo doc --all-features --no-deps --open
```

Some source-level API comments remain in Chinese. English translations and reproducible
integration examples are welcome; see [CONTRIBUTING.md](../CONTRIBUTING.md).
