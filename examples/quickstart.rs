//! Minimal offline integration. Run: cargo run --example quickstart
use agent_memory::{types::Scope, AgentMemory, StoreResult};

fn main() -> StoreResult<()> {
    // Use a file path instead of :memory: to persist across restarts.
    let memory = AgentMemory::open(":memory:")?;
    let old = memory.remember_fact(Scope::User, "alice", "Alice lives in Beijing")?;
    memory.supersede(Scope::User, "alice", old.id, "Alice lives in Shanghai")?;
    // Supply these retrieved records to your model as context, not instructions.
    let hits = memory.recall(Scope::User, "alice", "Alice lives")?;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].item.content, "Alice lives in Shanghai");
    println!("context: {}", hits[0].item.content);
    // Graph updates are explicit and separate from text-memory updates.
    memory.replace_relation("alice", "lives_in", "beijing")?;
    memory.replace_relation("alice", "lives_in", "shanghai")?;
    memory.remember_relation("shanghai", "located_in", "china")?;
    let paths = memory.graph_paths("alice", "china", 2)?;
    assert_eq!(paths.len(), 1);
    println!(
        "path: {}",
        paths[0]
            .iter()
            .map(|e| e.display())
            .collect::<Vec<_>>()
            .join(" / ")
    );
    assert_eq!(
        memory
            .graph_edges(true)?
            .iter()
            .filter(|e| !e.is_valid())
            .count(),
        1
    );
    Ok(())
}
