//! 集成测试：以真实 SQLite 文件为后端，端到端验证记忆库行为。

use std::path::PathBuf;
use std::sync::Arc;

use agent_memory::types::{MemoryType, Scope};
use agent_memory::{AgentMemory, RuleExtractor};

fn temp_db(name: &str) -> String {
    let dir = std::env::temp_dir().join("agent-memory-it");
    std::fs::create_dir_all(&dir).unwrap();
    let path: PathBuf = dir.join(format!("{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path.to_str().unwrap().to_string()
}

#[test]
fn end_to_end_memory_lifecycle_on_disk() {
    let db = temp_db("e2e");
    let mem = AgentMemory::open(&db).unwrap();

    // 写入
    mem.remember_fact(Scope::User, "u", "用户喜欢喝冷萃咖啡")
        .unwrap();
    mem.remember_fact(Scope::User, "u", "用户是后端工程师")
        .unwrap();
    mem.remember_event(Scope::User, "u", "今天用户完成了记忆系统 demo")
        .unwrap();

    // 检索应命中语义事实
    let hits = mem.recall(Scope::User, "u", "咖啡偏好").unwrap();
    assert!(hits.iter().any(|h| h.item.content.contains("冷萃咖啡")));

    // 关闭后重开，验证持久化
    drop(mem);
    let mem2 = AgentMemory::open(&db).unwrap();
    let hits2 = mem2.recall(Scope::User, "u", "职业").unwrap();
    assert!(hits2.iter().any(|h| h.item.content.contains("后端工程师")));
    assert_eq!(mem2.stats(Scope::User, "u").unwrap().total, 3);

    std::fs::remove_file(&db).ok();
}

#[test]
fn working_memory_eviction_integration() {
    let db = temp_db("evict");
    let mem = AgentMemory::open(&db).unwrap().with_working_capacity(4);
    for i in 0..10 {
        mem.add(
            Scope::Session,
            "s",
            MemoryType::Working,
            &format!("turn {i}"),
        )
        .unwrap();
    }
    let alive = mem
        .list(Scope::Session, "s", Some(MemoryType::Working))
        .unwrap();
    assert_eq!(alive.len(), 4);
    // 只保留最新的 7 8 9 和 6
    let contents: Vec<String> = alive.iter().map(|m| m.content.clone()).collect();
    for c in &contents {
        assert!(
            !c.contains("turn 0")
                && !c.contains("turn 1")
                && !c.contains("turn 2")
                && !c.contains("turn 3")
                && !c.contains("turn 4")
                && !c.contains("turn 5")
        );
    }
    std::fs::remove_file(&db).ok();
}

#[test]
fn supersede_is_not_hard_delete() {
    let db = temp_db("sup");
    let mem = AgentMemory::open(&db).unwrap();
    let old = mem.remember_fact(Scope::User, "u", "用户住在北京").unwrap();
    let new = mem
        .supersede(Scope::User, "u", old.id, "用户搬到上海")
        .unwrap();

    // 旧记录仍在库中（可追溯），但被标记
    let old_after = mem.get(old.id).unwrap().unwrap();
    assert!(old_after.is_superseded());
    assert_eq!(old_after.superseded_by, Some(new.id));

    // 检索只返回有效记忆
    let hits = mem.recall(Scope::User, "u", "城市").unwrap();
    assert!(hits.iter().all(|h| !h.item.is_superseded()));
    std::fs::remove_file(&db).ok();
}

#[test]
fn retrieval_ranks_relevant_first() {
    let db = temp_db("rank");
    let mem = AgentMemory::open(&db).unwrap();
    mem.remember_fact(Scope::User, "u", "用户在用 Rust 做 gRPC 网关")
        .unwrap();
    mem.remember_fact(Scope::User, "u", "用户昨天去逛了公园")
        .unwrap();
    mem.remember_fact(Scope::User, "u", "Rust 的异步运行时是 tokio")
        .unwrap();

    let hits = mem.recall(Scope::User, "u", "Rust 网关").unwrap();
    assert!(!hits.is_empty());
    // 最相关的那条应该排第一
    assert!(hits[0].item.content.contains("gRPC 网关"));
    std::fs::remove_file(&db).ok();
}

#[test]
fn graph_persists_and_multi_hop_on_disk() {
    let db = temp_db("graph");
    let mem = AgentMemory::open(&db).unwrap();
    mem.remember_relation("用户", "使用", "Rust").unwrap();
    mem.remember_relation("Rust", "运行在", "Linux").unwrap();
    mem.replace_relation("用户", "住在", "北京").unwrap();
    mem.replace_relation("用户", "住在", "上海").unwrap();

    // 关闭重开，图谱持久化
    drop(mem);
    let mem2 = AgentMemory::open(&db).unwrap();
    let paths = mem2.graph_paths("用户", "Linux", 2).unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].len(), 2);
    // 单值关系只保留最新值，旧边失效但可查
    assert_eq!(
        mem2.graph_edges(false)
            .unwrap()
            .iter()
            .filter(|e| e.predicate == "住在")
            .count(),
        1
    );
    assert_eq!(
        mem2.graph_edges(true)
            .unwrap()
            .iter()
            .filter(|e| e.predicate == "住在")
            .count(),
        2
    );
    std::fs::remove_file(&db).ok();
}

#[test]
fn auto_extract_links_edge_to_source_memory() {
    let db = temp_db("extract");
    let mem = AgentMemory::open(&db)
        .unwrap()
        .with_extractor(Arc::new(RuleExtractor::new()));
    let item = mem
        .remember_fact(Scope::User, "u", "用户喜欢 Rust")
        .unwrap();
    let edges = mem.graph_edges(false).unwrap();
    assert_eq!(edges.len(), 1);
    // 边可溯源到来源记忆
    assert_eq!(edges[0].source_memory_id, Some(item.id));
    std::fs::remove_file(&db).ok();
}
