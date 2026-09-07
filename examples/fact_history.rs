//! A real three-process SQLite demonstration, with checked results.
//! Run without arguments: cargo run --example fact_history
//! A fresh temporary database is cleaned up afterward. No LLM is involved:
//! the application supplies facts and graph updates explicitly.
use agent_memory::{types::Scope, AgentMemory};
use std::{env, fs, path::PathBuf, process::Command, time::SystemTime};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct DemoDir(PathBuf);
impl DemoDir {
    fn create() -> Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let path =
            env::temp_dir().join(format!("agent-memory-demo-{}-{nonce}", std::process::id()));
        // Exclusive creation: never reuse or clean up someone else's directory.
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for DemoDir {
    fn drop(&mut self) {
        for name in [
            "memory.db",
            "memory.db-wal",
            "memory.db-shm",
            "memory.db-journal",
        ] {
            let _ = fs::remove_file(self.0.join(name));
        }
        let _ = fs::remove_dir(&self.0);
    }
}

fn step(mode: &str, db: &str) -> Result<()> {
    let memory = AgentMemory::open(db)?;
    match mode {
        "seed" => {
            assert!(memory.list(Scope::User, "alice", None)?.is_empty());
            memory.remember_fact(Scope::User, "alice", "Alice lives in Beijing")?;
            memory.replace_relation("alice", "lives_in", "beijing")?;
            memory.remember_relation("shanghai", "located_in", "china")?;
            println!("PROCESS 1 | Store a fact");
            println!("  memory: Alice lives in Beijing");
            println!("  active: alice -lives_in-> beijing");
        }
        "move" => {
            let old = memory.recall(Scope::User, "alice", "Alice lives")?;
            assert_eq!(old.len(), 1);
            assert_eq!(old[0].item.content, "Alice lives in Beijing");
            memory.supersede(
                Scope::User,
                "alice",
                old[0].item.id,
                "Alice lives in Shanghai",
            )?;
            memory.replace_relation("alice", "lives_in", "shanghai")?;
            println!("PROCESS 2 | Reopen the database and update");
            println!("  memory: Alice lives in Shanghai");
            for edge in memory
                .graph_edges(true)?
                .iter()
                .filter(|e| e.predicate == "lives_in")
            {
                println!(
                    "  {}: {}",
                    if edge.is_valid() {
                        "active"
                    } else {
                        "invalidated"
                    },
                    edge.display()
                );
            }
        }
        "inspect" => {
            let hits = memory.recall(Scope::User, "alice", "Alice lives")?;
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].item.content, "Alice lives in Shanghai");
            let history = memory.store().list(Scope::User, "alice", None)?;
            assert_eq!(history.len(), 2);
            assert_eq!(history.iter().filter(|m| m.is_superseded()).count(), 1);
            let edges = memory.graph_edges(true)?;
            assert_eq!(edges.len(), 3);
            assert!(edges.iter().any(|e| e.object == "beijing" && !e.is_valid()));
            assert!(edges.iter().any(|e| e.object == "shanghai" && e.is_valid()));
            let paths = memory.graph_paths("alice", "china", 2)?;
            assert_eq!(paths.len(), 1);
            assert_eq!(paths[0].len(), 2);
            assert_eq!(paths[0][0].object, "shanghai");
            assert_eq!(paths[0][1].object, "china");
            assert!(memory.recall(Scope::User, "bob", "Alice lives")?.is_empty());
            println!("PROCESS 3 | Reopen again and verify");
            println!("  recall: {}", hits[0].item.content);
            println!(
                "  path: {}",
                paths[0]
                    .iter()
                    .map(|e| e.display())
                    .collect::<Vec<_>>()
                    .join(" / ")
            );
            println!("  history: 1 superseded memory, 1 invalidated relation");
            println!("  scope check: bob recalls 0 memories (the graph is shared per store)");
            println!("PASS | Persistence, replacement, history and two-hop traversal verified");
        }
        _ => return Err("internal demo mode must be seed, move or inspect".into()),
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if let [mode, db] = args.as_slice() {
        return step(mode, db);
    }
    if !args.is_empty() {
        return Err("Run without arguments: cargo run --example fact_history".into());
    }
    let dir = DemoDir::create()?;
    let db = dir.0.join("memory.db");
    println!("agent-memory | SQLite fact-history demo");
    println!("Real program output. Explicit application updates. No LLM or network calls.\n");
    for mode in ["seed", "move", "inspect"] {
        let status = Command::new(env::current_exe()?)
            .arg(mode)
            .arg(&db)
            .status()?;
        if !status.success() {
            return Err(format!("demo process {mode} failed: {status}").into());
        }
        println!();
    }
    Ok(())
}
