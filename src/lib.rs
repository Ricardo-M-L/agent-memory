//! # agent-memory
//!
//! 离线优先、零配置的 LLM Agent 记忆层（Rust 实现）。
//!
//! 设计综合了业界主流 AI 记忆系统的成熟思路：
//!
//! - **记忆分类**：工作 / 情景 / 语义三层（对齐 CoALA / Mem0）；
//! - **作用域**：user / session / agent 三级隔离（对齐 Mem0）；
//! - **检索打分**：`时效 × 相关 × 重要`（对齐 *Generative Agents* 论文）；
//! - **相关度**：BM25 关键词 + 可选向量余弦的混合（对齐向量 RAG 方案）；
//! - **冲突解决**：新事实写入时旧条目标记 `superseded`，不静默覆盖（对齐 Zep/Graphiti）；
//! - **遗忘**：TTL 过期 + 主动清理（对齐 Letta 归档层级）；
//! - **整合**：语义去重合并（对齐 LangMem 蒸馏层级）。
//!
//! ## 开箱即用
//!
//! 内置 [`embed::HashEmbedder`]（特征哈希嵌入），无需任何 API key / 向量数据库即可离线运行；
//! 高级用户可实现 [`embed::Embedder`] trait 替换为真实语义模型。
//!
//! ## 快速开始
//!
//! ```no_run
//! use agent_memory::AgentMemory;
//! use agent_memory::types::Scope;
//!
//! let mem = AgentMemory::open("agent-memory.db").unwrap();
//! mem.remember_fact(Scope::User, "u-1", "用户喜欢用 Rust 写 Agent").unwrap();
//!
//! let hits = mem.recall(Scope::User, "u-1", "用户喜欢什么语言").unwrap();
//! for h in &hits {
//!     println!("{} ({:.2})", h.item.content, h.score);
//! }
//! ```

pub mod db;
pub mod embed;
pub mod extract;
pub mod graph;
pub mod memory;
#[cfg(feature = "neo4j")]
pub mod neo4j;
pub mod retrieval;
pub mod sqlite_store;
pub mod store;
pub mod summarizer;
pub mod text;
pub mod types;

/// OpenAI 兼容的 HTTP 嵌入器（需启用 `http` feature）。
#[cfg(feature = "http")]
pub mod http_embed;

pub use embed::{Embedder, HashEmbedder};
pub use extract::{
    extraction_prompt, parse_triples_json, ChatClient, Extractor, LlmExtractor, RuleExtractor,
};
pub use graph::{Edge, Entity, GraphStats, GraphStore, SqliteGraphStore, Triple};
pub use memory::{AgentMemory, MemoryStats, TimeRange};
#[cfg(feature = "neo4j")]
pub use neo4j::{Neo4jAuth, Neo4jGraphConfig, Neo4jGraphStore};
pub use retrieval::{RetrievalConfig, ScoredMemory};
pub use store::{MemoryStore, StoreError, StoreResult};
