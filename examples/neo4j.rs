//! 设置 NEO4J_PASSWORD 后运行：cargo run --features neo4j --example neo4j
use agent_memory::types::Scope;
use agent_memory::{
    AgentMemory, GraphStore, Neo4jGraphConfig, Neo4jGraphStore, RuleExtractor, Triple,
};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint =
        std::env::var("NEO4J_ENDPOINT").unwrap_or_else(|_| "http://localhost:7474".into());
    let username = std::env::var("NEO4J_USERNAME").unwrap_or_else(|_| "neo4j".into());
    let password = std::env::var("NEO4J_PASSWORD").map_err(|_| "set NEO4J_PASSWORD first")?;
    let database = std::env::var("NEO4J_DATABASE").unwrap_or_else(|_| "neo4j".into());
    let namespace = std::env::var("NEO4J_NAMESPACE").unwrap_or_else(|_| "agent-memory-demo".into());
    let graph = Arc::new(Neo4jGraphStore::connect(
        Neo4jGraphConfig::basic(endpoint, username, password)
            .with_database(database)
            .with_namespace(namespace),
    )?);
    let path = std::env::var("MEMORY_DB").unwrap_or_else(|_| "neo4j-demo.db".into());
    let memory = AgentMemory::open(&path)?
        .with_graph(graph.clone())
        .with_extractor(Arc::new(RuleExtractor::new()));
    let item = memory.remember_fact(Scope::User, "alice", "Alice lives in Shanghai")?;
    graph.add_triple(
        &Triple::new("shanghai", "located_in", "china"),
        Some(item.id),
        item.created_at,
    )?;
    // RuleExtractor 将英文实体归一为小写，手工补边使用同一命名。
    for path in graph.find_paths("alice", "china", 2)? {
        println!(
            "{}",
            path.iter()
                .map(|e| e.display())
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
    println!("{:?}", graph.graph_stats()?);
    Ok(())
}
