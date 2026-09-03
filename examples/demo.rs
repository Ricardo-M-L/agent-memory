//! agent-memory 端到端示例。
//!
//! 运行：`cargo run --example demo` 或 `cargo run --example demo -- <db_path>`
//! 展示：写入、检索、作用域隔离、冲突解决、过期遗忘、语义去重、知识图谱多跳推理。

use std::sync::Arc;

use agent_memory::retrieval::RetrievalConfig;
use agent_memory::types::{MemoryType, Scope};
use agent_memory::{AgentMemory, Embedder, HashEmbedder, RuleExtractor};

fn main() {
    let db = std::env::args().nth(1).unwrap_or_else(|| "demo.db".into());
    let mem = AgentMemory::open(&db).expect("打开记忆库失败");
    println!("== agent-memory demo (db: {db}) ==\n");

    // 1) 写入多种类型记忆
    println!("[1] 写入记忆");
    mem.remember_fact(Scope::User, "demo", "用户喜欢用 Rust 写 CLI 工具")
        .unwrap();
    mem.remember_fact(Scope::User, "demo", "用户偏好本地优先的隐私方案")
        .unwrap();
    mem.remember_event(Scope::User, "demo", "上个月用户给开源项目提交了 PR")
        .unwrap();
    mem.add(
        Scope::User,
        "demo",
        MemoryType::Working,
        "当前任务：设计记忆系统",
    )
    .unwrap();
    println!("    ok\n");

    // 2) 检索
    println!("[2] 检索: '用户用什么语言'");
    for h in mem.recall(Scope::User, "demo", "用户用什么语言").unwrap() {
        println!(
            "    {:.3}  #{} [{}] {}",
            h.score,
            h.item.id,
            h.item.memory_type.as_str(),
            h.item.content
        );
    }
    println!();

    // 3) 作用域隔离
    println!("[3] 作用域隔离");
    mem.remember_fact(Scope::User, "alice", "Alice 喜欢咖啡")
        .unwrap();
    let hits = mem.recall(Scope::User, "alice", "喝什么").unwrap();
    println!(
        "    alice 检索到 {} 条，全部关于咖啡: {}",
        hits.len(),
        hits.iter().all(|h| h.item.content.contains("咖啡"))
    );
    println!();

    // 4) 冲突解决
    println!("[4] 冲突解决");
    let old = mem
        .recall(Scope::User, "demo", "语言")
        .unwrap()
        .remove(0)
        .item;
    let new = mem
        .supersede(Scope::User, "demo", old.id, "用户最近更喜欢用 Go")
        .unwrap();
    let after = mem.get(old.id).unwrap().unwrap();
    println!(
        "    #{} superseded={} (被 #{} 取代)",
        old.id,
        after.is_superseded(),
        new.id
    );
    println!();

    // 5) 语义去重合并
    println!("[5] 语义去重");
    mem.remember_fact(Scope::User, "demo", "用户偏好本地优先方案")
        .unwrap();
    let merged = mem.consolidate(Scope::User, "demo", 0.8).unwrap();
    println!("    合并 {} 条重复语义记忆\n", merged);

    // 6) 统计
    let s = mem.stats(Scope::User, "demo").unwrap();
    println!(
        "[6] 统计: working={} episodic={} semantic={} superseded={} total={}",
        s.working, s.episodic, s.semantic, s.superseded, s.total
    );

    // 7) 知识图谱：手动关系 + 自动抽取 + 多跳推理
    println!("\n[7] 知识图谱");
    mem.remember_relation("用户", "使用", "Rust").unwrap();
    mem.remember_relation("Rust", "擅长", "系统编程").unwrap();
    let paths = mem.graph_paths("用户", "系统编程", 2).unwrap();
    for p in &paths {
        let chain = p
            .iter()
            .map(|e| e.display())
            .collect::<Vec<_>>()
            .join(" => ");
        println!("    多跳路径: {chain}");
    }

    // 开启规则抽取器后，写入文本会自动落三元组
    let g = AgentMemory::open(":memory:")
        .unwrap()
        .with_extractor(Arc::new(RuleExtractor::new()));
    g.remember_fact(Scope::User, "demo", "用户住在杭州")
        .unwrap();
    println!(
        "    自动抽取边: {:?}",
        g.graph_edges(false)
            .unwrap()
            .iter()
            .map(|e| e.display())
            .collect::<Vec<_>>()
    );

    // 8) 自定义嵌入器示例（把内置哈希嵌入换成你自己实现）
    println!("\n[8] 自定义嵌入器（示例：更高维度哈希嵌入）");
    let custom = CustomEmbedder(HashEmbedder::new(1024));
    let m2 = AgentMemory::with_store(
        Box::new(agent_memory::sqlite_store::SqliteStore::open(":memory:").unwrap()),
        Arc::new(custom),
        RetrievalConfig::default(),
    );
    m2.remember_fact(Scope::User, "x", "自定义嵌入器也能工作")
        .unwrap();
    println!(
        "    自定义嵌入器检索命中 {} 条",
        m2.recall(Scope::User, "x", "嵌入").unwrap().len()
    );
}

/// 演示如何包装内置哈希嵌入器（你也可以实现自己的真实模型嵌入器）。
struct CustomEmbedder(HashEmbedder);

impl Embedder for CustomEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        self.0.embed(text)
    }
    fn dim(&self) -> usize {
        self.0.dim()
    }
}
