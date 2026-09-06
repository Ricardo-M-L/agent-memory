#![cfg(feature = "neo4j")]
//! 真实服务集成测试，按文档设置环境变量并用 --ignored 显式运行。
use std::sync::{Arc, Barrier};
use std::time::{SystemTime, UNIX_EPOCH};

use agent_memory::types::Scope;
use agent_memory::{
    AgentMemory, GraphStore, Neo4jGraphConfig, Neo4jGraphStore, RuleExtractor, Triple,
};
use base64::Engine as _;
use serde_json::{json, Value};

struct Fixture {
    config: Neo4jGraphConfig,
    graph: Neo4jGraphStore,
}

impl Fixture {
    fn new(label: &str) -> Self {
        assert_eq!(
            std::env::var("NEO4J_TEST_ALLOW_WRITE").as_deref(),
            Ok("1"),
            "set NEO4J_TEST_ALLOW_WRITE=1 to run isolated write tests"
        );
        let endpoint =
            std::env::var("NEO4J_ENDPOINT").expect("set NEO4J_ENDPOINT to a test server root URL");
        let password = std::env::var("NEO4J_PASSWORD").expect("set NEO4J_PASSWORD");
        let username = std::env::var("NEO4J_USERNAME").unwrap_or_else(|_| "neo4j".into());
        let namespace = format!(
            "agent-memory-test-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let config = Neo4jGraphConfig::basic(endpoint, username, password)
            .with_namespace(namespace)
            .with_database(std::env::var("NEO4J_DATABASE").unwrap_or_else(|_| "neo4j".into()));
        let graph = Neo4jGraphStore::connect(config.clone()).unwrap();
        Self { config, graph }
    }

    fn raw(&self, statement: &str) -> Value {
        let agent_memory::Neo4jAuth::Basic { username, password } = &self.config.auth else {
            unreachable!()
        };
        let auth =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        let url = format!(
            "{}/db/{}/query/v2",
            self.config.endpoint.trim_end_matches('/'),
            self.config.database.as_deref().unwrap()
        );
        let response = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .post(&url)
            .set("Authorization", &format!("Basic {auth}"))
            .send_json(
                json!({"statement": statement, "parameters":{"namespace":self.config.namespace}}),
            )
            .unwrap();
        let payload: Value = response.into_json().unwrap();
        assert!(
            payload
                .get("errors")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty),
            "{payload}"
        );
        payload
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // 仅清理本测试生成的命名空间，不清空数据库。
        if !std::thread::panicking() {
            self.raw("MATCH (n:AgentMemoryEntity {namespace: $namespace}) DETACH DELETE n");
            self.raw("MATCH (g:AgentMemoryGraph {name: $namespace}) DELETE g");
        }
    }
}

#[test]
#[ignore = "requires Neo4j Query API; see docs/neo4j.md"]
fn fact_lifecycle_and_namespace_isolation() {
    let f = Fixture::new("lifecycle");
    let g = &f.graph;
    let shanghai = Triple::new(" Alice ", "lives_in", "Shanghai").with_confidence(0.6);
    let id = g.add_triple(&shanghai, Some(1), 10).unwrap();
    assert_eq!(
        g.add_triple(&shanghai.clone().with_confidence(0.8), Some(2), 20)
            .unwrap(),
        id
    );
    let e = &g.edges(false).unwrap()[0];
    assert_eq!(
        (e.created_at, e.source_memory_id, e.confidence),
        (10, Some(1), 0.8)
    );
    assert_eq!(g.replace_triple(&shanghai, Some(3), 21).unwrap(), id);
    assert_eq!(g.edges(false).unwrap().len(), 1);
    let beijing = Triple::new("Alice", "lives_in", "Beijing");
    g.replace_triple(&beijing, Some(3), 30).unwrap();
    assert_eq!(g.edges(false).unwrap()[0].object, "Beijing");
    assert_eq!(
        g.edges(true)
            .unwrap()
            .iter()
            .find(|e| e.id == id)
            .unwrap()
            .invalidated_at,
        Some(30)
    );
    assert_eq!(g.replace_triple(&shanghai, Some(4), 40).unwrap(), id);
    let e = &g.edges(false).unwrap()[0];
    assert_eq!((e.id, e.created_at, e.source_memory_id), (id, 40, Some(4)));
    g.add_triple(&beijing, None, 45).unwrap();
    assert_eq!(
        g.edges(false).unwrap().len(),
        1,
        "add must not reactivate an invalidated fact"
    );
    let second_client = Neo4jGraphStore::new(f.config.clone()).unwrap();
    assert_eq!(second_client.add_triple(&shanghai, None, 50).unwrap(), id);

    let other = Fixture::new("isolation");
    assert!(other.graph.entities().unwrap().is_empty());
    other.graph.add_triple(&beijing, None, 50).unwrap();
    assert_eq!(other.graph.edges(false).unwrap()[0].object, "Beijing");
    assert_eq!(g.edges(false).unwrap()[0].object, "Shanghai");
    assert_eq!(g.invalidate("Alice", "lives_in", 60).unwrap(), 1);
    assert_eq!(g.invalidate("Alice", "lives_in", 61).unwrap(), 0);
    assert_eq!(g.invalidate("missing", "p", 61).unwrap(), 0);
    let stats = g.graph_stats().unwrap();
    assert_eq!(
        (
            stats.valid_edges,
            stats.invalid_edges,
            stats.entities,
            stats.communities
        ),
        (0, 2, 3, 3)
    );
}

#[test]
#[ignore = "requires Neo4j Query API; see docs/neo4j.md"]
fn traversal_and_memory_extraction() {
    let f = Fixture::new("traversal");
    let g = &f.graph;
    assert_eq!(g.graph_stats().unwrap(), Default::default());
    assert!(g.communities().unwrap().is_empty());
    for (s, p, o) in [
        ("A", "p", "B"),
        ("B", "p", "C"),
        ("C", "p", "A"),
        ("A", "direct", "C"),
        ("D", "p", "E"),
    ] {
        g.add_triple(&Triple::new(s, p, o), None, 1).unwrap();
    }
    assert_eq!(g.neighbors("A", 1).unwrap().len(), 3);
    assert_eq!(g.neighbors("A", 2).unwrap().len(), 4);
    assert_eq!(g.find_paths("A", "C", 1).unwrap().len(), 1);
    assert_eq!(g.find_paths("A", "C", usize::MAX).unwrap().len(), 2);
    assert!(g.find_paths("C", "D", 10).unwrap().is_empty());
    assert!(g.find_paths("A", "A", 10).unwrap().is_empty());
    assert_eq!(
        g.communities().unwrap(),
        vec![vec!["A", "B", "C"], vec!["D", "E"]]
    );
    let stats = g.graph_stats().unwrap();
    assert_eq!(
        (
            stats.entities,
            stats.valid_edges,
            stats.communities,
            stats.largest_community
        ),
        (5, 5, 2, 3)
    );
    let memory = AgentMemory::open(":memory:")
        .unwrap()
        .with_graph(Arc::new(g.clone()))
        .with_extractor(Arc::new(RuleExtractor::new()));
    let item = memory
        .remember_fact(Scope::User, "alice", "Alice lives in Shanghai")
        .unwrap();
    let extracted = g
        .edges(false)
        .unwrap()
        .into_iter()
        .find(|e| e.subject == "alice")
        .unwrap();
    assert_eq!(extracted.object, "shanghai");
    assert_eq!(extracted.source_memory_id, Some(item.id));
    assert!(memory
        .get(extracted.source_memory_id.unwrap())
        .unwrap()
        .is_some());
}

#[test]
#[ignore = "requires Neo4j Query API; see docs/neo4j.md"]
fn merge_rewires_deduplicates_and_preserves_unrelated_edges() {
    let f = Fixture::new("merge");
    let g = &f.graph;
    let keep_id = g
        .add_triple(
            &Triple::new("keep", "uses", "Rust").with_confidence(0.6),
            None,
            20,
        )
        .unwrap();
    g.add_triple(
        &Triple::new("alias", "uses", "Rust").with_confidence(0.9),
        Some(1),
        10,
    )
    .unwrap();
    for (s, p, o) in [
        ("Z", "knows", "alias"),
        ("alias", "self", "alias"),
        ("alias", "link", "keep"),
        ("Q", "self", "Q"),
        ("keep", "self", "keep"),
    ] {
        g.add_triple(&Triple::new(s, p, o), None, 15).unwrap();
    }
    let mentions: i64 = g
        .entities()
        .unwrap()
        .iter()
        .filter(|e| ["keep", "alias"].contains(&e.name.as_str()))
        .map(|e| e.mention_count)
        .sum();
    g.invalidate("keep", "uses", 25).unwrap();
    assert_eq!(g.merge_entities("keep", "alias").unwrap(), 4);
    let edges = g.edges(true).unwrap();
    assert_eq!(edges.len(), 4);
    assert!(edges
        .iter()
        .all(|e| e.subject != "alias" && e.object != "alias"));
    assert!(edges.iter().any(|e| e.subject == "Q" && e.object == "Q"));
    assert!(edges
        .iter()
        .any(|e| e.subject == "keep" && e.object == "keep"));
    let e = edges.iter().find(|e| e.predicate == "uses").unwrap();
    assert_eq!(
        (
            e.id,
            e.created_at,
            e.confidence,
            e.source_memory_id,
            e.invalidated_at
        ),
        (keep_id, 10, 0.9, Some(1), None)
    );
    let entities = g.entities().unwrap();
    let keep = entities.iter().find(|e| e.name == "keep").unwrap();
    assert_eq!(keep.mention_count, mentions);
    assert!(!entities.iter().any(|e| e.name == "alias"));
    assert!(g.merge_entities("keep", "missing").is_err());
    assert!(g.merge_entities("keep", "keep").is_err());
    f.raw("CREATE (:AgentMemoryEntity {namespace:$namespace, name:'isolated', id:9999, mention_count:1, first_seen:1, last_seen:1})");
    assert_eq!(g.merge_entities("keep", "isolated").unwrap(), 0);
    assert!(!g.entities().unwrap().iter().any(|e| e.name == "isolated"));
}

#[test]
#[ignore = "requires Neo4j Query API; see docs/neo4j.md"]
fn failed_merge_rolls_back_all_changes() {
    let f = Fixture::new("rollback");
    let g = &f.graph;
    g.add_triple(&Triple::new("keep", "uses", "Rust"), None, 1)
        .unwrap();
    g.add_triple(&Triple::new("alias", "uses", "Go"), None, 2)
        .unwrap();
    f.raw("MATCH (a:AgentMemoryEntity {namespace:$namespace, name:'alias'}), (k:AgentMemoryEntity {namespace:$namespace, name:'keep'}) CREATE (a)-[:FOREIGN]->(k)");
    let before = (g.entities().unwrap(), g.edges(true).unwrap());
    assert!(g.merge_entities("keep", "alias").is_err());
    assert_eq!((g.entities().unwrap(), g.edges(true).unwrap()), before);
}

#[test]
#[ignore = "requires Neo4j Query API; see docs/neo4j.md"]
fn independent_clients_serialize_duplicate_and_conflicting_writes() {
    let f = Fixture::new("concurrent");
    let barrier = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let client = Neo4jGraphStore::new(f.config.clone()).unwrap();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                client
                    .add_triple(&Triple::new("Alice", "likes", "Rust"), None, 1)
                    .unwrap()
            })
        })
        .collect();
    let ids: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert!(ids.iter().all(|id| *id == ids[0]));
    assert_eq!(f.graph.edges(true).unwrap().len(), 1);
    assert_eq!(
        f.graph
            .entities()
            .unwrap()
            .iter()
            .find(|e| e.name == "Alice")
            .unwrap()
            .mention_count,
        8
    );
    let workers: Vec<_> = (0..8)
        .map(|i| {
            let client = Neo4jGraphStore::new(f.config.clone()).unwrap();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                for round in 0..4 {
                    client
                        .replace_triple(
                            &Triple::new("Alice", "lives_in", format!("city-{i}")),
                            None,
                            round,
                        )
                        .unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let edges = f.graph.edges(true).unwrap();
    assert_eq!(
        edges.iter().filter(|e| e.predicate == "lives_in").count(),
        8
    );
    assert_eq!(
        edges
            .iter()
            .filter(|e| e.predicate == "lives_in" && e.is_valid())
            .count(),
        1
    );
    let entity_ids: std::collections::HashSet<_> =
        f.graph.entities().unwrap().iter().map(|e| e.id).collect();
    assert_eq!(entity_ids.len(), f.graph.entities().unwrap().len());
}
