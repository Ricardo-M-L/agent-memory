# agent-memory

> 离线优先、零配置的 LLM Agent 记忆层（Rust 实现）

`agent-memory` 是一个为 LLM Agent 提供**持久化记忆**的轻量 Rust 库 + CLI。它把业界主流 AI 记忆系统（Mem0 / Letta / Zep 等）的设计精华压缩成一个**无需任何外部服务**就能跑起来的实现：内置 SQLite 存储、特征哈希嵌入与轻量知识图谱，无需向量数据库、无需图数据库（Neo4j）、无需 API key、无需联网。

它同时提供三条互补的记忆通道——**关键词（BM25）+ 向量（语义相似）+ 知识图谱（实体关系/多跳推理）**，覆盖"相似内容召回"与"实体关系推理"两类不同需求。

```
cargo add agent-memory   # 作为库
cargo run -- demo        # 作为 CLI 演示
```

## 它解决什么问题

LLM 的上下文窗口是它的"短期记忆"——窗口一满，之前的信息就丢了。Agent 要跨会话记住用户偏好、历史事件、领域知识，就必须把记忆**存到窗口之外**，并在需要时**检索回来**。这就是 Agentic Memory 的核心工程问题：存什么、何时取、如何更新、如何遗忘。本项目用一套小而完整的实现回答这些问题。

## 核心特性

| 特性 | 说明 |
|---|---|
| **三类记忆** | `working`（工作/短期，滚动淘汰）· `episodic`（情景，事件历史）· `semantic`（语义，事实/偏好） |
| **三级作用域** | `user` / `session` / `agent` 隔离，不同主体互不串记忆 |
| **混合检索打分** | `时效 × 相关 × 重要` 加权（源自 *Generative Agents* 论文）；相关度 = BM25 关键词 + 可选向量余弦 |
| **轻量知识图谱** | 实体 + 关系三元组（`subject -predicate-> object`），支持无向邻居、多跳路径推理；**不依赖 Neo4j**，纯 SQLite 实现（对齐 Zep/Graphiti 图记忆） |
| **实体关系抽取** | `Extractor` trait + 离线中英规则抽取器兜底；写入时可自动落三元组，也可换 LLM 抽取 |
| **图上时间冲突** | 单值关系（住在/任职于）变更时旧边标记 `invalidated_at` 不删除、可追溯；多值关系（喜欢/使用）共存 |
| **冲突解决** | 新事实取代旧事实时，旧条目标记 `superseded` 而非静默覆盖（可追溯，对齐 Zep 时序图谱思想） |
| **遗忘机制** | TTL 过期自动清理 + 手动 `prune`，防止记忆无限膨胀（对齐 Letta 归档层） |
| **整合去重** | `consolidate` 按向量相似度合并重复语义记忆（对齐 LangMem 蒸馏层级） |
| **离线开箱即用** | 内置 `HashEmbedder`（特征哈希嵌入）+ `RuleExtractor`（规则抽取），无模型、无网络、无配置 |
| **可插拔** | `Embedder` / `MemoryStore` / `Summarizer` / `Extractor` / `GraphStore` 五个 trait，可无缝替换为真实模型/存储/LLM/图数据库 |
| **纯 Rust + SQLite** | 单二进制、零运行时依赖、线程安全（`Arc` 共享）；记忆与图谱同库、边可溯源到来源记忆 |

## 快速开始

### 作为库

```toml
[dependencies]
agent-memory = { path = "agent记忆" }
```

```rust
use agent_memory::AgentMemory;
use agent_memory::types::{MemoryType, Scope};
use agent_memory::RuleExtractor;

// 打开（或创建）记忆库；":memory:" 使用内存库
let mem = AgentMemory::open("agent-memory.db")?;

// 写入
mem.remember_fact(Scope::User, "u-1", "用户喜欢用 Rust 写 Agent")?;
mem.remember_event(Scope::User, "u-1", "用户上周完成了知识图谱学习")?;
mem.add(Scope::User, "u-1", MemoryType::Working, "当前任务：设计记忆系统")?;

// 检索（按 时效×相关×重要 打分）
let hits = mem.recall(Scope::User, "u-1", "用户喜欢什么语言")?;
for h in &hits {
    println!("{} (score={:.3})", h.item.content, h.score);
}

// 冲突解决：用户改主意了
mem.supersede(Scope::User, "u-1", old_id, "用户现在更喜欢 Go")?;

// 遗忘管理 & 整合
mem.prune()?;                                    // 清过期
mem.consolidate(Scope::User, "u-1", 0.82)?;      // 语义去重

// ===== 知识图谱 =====
// 手动写入关系（多值关系可共存）
mem.remember_relation("用户", "使用", "Rust")?;
mem.remember_relation("Rust", "擅长", "系统编程")?;
// 单值关系变更：旧边自动失效、可追溯
mem.replace_relation("用户", "住在", "北京")?;
mem.replace_relation("用户", "住在", "上海")?;
// 多跳推理：用户 -> Rust -> 系统编程
for path in mem.graph_paths("用户", "系统编程", 3)? {
    println!("{}", path.iter().map(|e| e.display()).collect::<Vec<_>>().join(" => "));
}

// 写入时自动抽取三元组（离线规则抽取器，也可换成 LLM 抽取器）
let mem = AgentMemory::open("agent-memory.db")?
    .with_extractor(std::sync::Arc::new(RuleExtractor::new()));
mem.remember_fact(Scope::User, "u-1", "用户喜欢 Rust")?;  // 自动落 用户-喜欢->Rust

// 检索记忆的同时带出查询中实体的图谱关系
let (hits, edges) = mem.recall_with_graph(Scope::User, "u-1", "用户会什么")?;
```

### 作为 CLI

```bash
export AGENT_MEMORY_DB=./agent-memory.db
agent-memory add user dev semantic 用户喜欢用 Rust 写 Agent 应用
agent-memory recall user dev 用户喜欢什么语言
agent-memory list user dev
agent-memory stats user dev
agent-memory supersede user dev 1 用户现在喜欢用 Go
agent-memory consolidate user dev 0.8
agent-memory prune
# 知识图谱
agent-memory graph add 用户 使用 Rust
agent-memory graph add Rust 擅长 系统编程
agent-memory graph replace 用户 住在 上海      # 旧边失效
agent-memory graph neighbors 用户 2           # 2 跳邻居
agent-memory graph path 用户 系统编程 3       # 多跳路径
agent-memory graph edges --all                # 含已失效边
agent-memory demo      # 端到端演示
```

## 架构

```
┌─────────────────────────────────────────────────────────────────┐
│                       你的 Agent / CLI                            │
│                  (通过 Arc<AgentMemory> 多线程共享)                │
└───────────────┬───────────────────────────┬─────────────────────┘
                │ add / remember_*          │ recall / recall_with_graph
                ▼                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                      AgentMemory（门面/编排）                       │
│   写入: 评估重要性 → 计算嵌入 → 持久化 → (working滚动淘汰)          │
│         → Extractor 抽取三元组 → 知识图谱(可溯源到来源记忆)         │
│   检索: 候选过滤 → BM25+向量 → 时效×相关×重要 → Top-K             │
│   图谱: 邻居扩展 / 多跳路径 / 单值关系时间失效                      │
│   生命周期: supersede / forget / prune / consolidate              │
└──────┬──────────────┬──────────────┬──────────────┬──────────────┘
       │              │              │              │
┌──────▼───────┐ ┌────▼──────┐ ┌─────▼──────┐ ┌─────▼─────────┐
│ MemoryStore  │ │ Embedder  │ │ Extractor  │ │  GraphStore   │
│  (trait)     │ │ (trait)   │ │  (trait)   │ │   (trait)     │
│ SQLite/内存/ │ │ HashEmbed │ │ Rule抽取/  │ │ SQLite三元组/ │
│ 自定义实现    │ │ /真实模型 │ │ LLM抽取    │ │ Neo4j(可替换) │
└──────┬───────┘ └───────────┘ └────────────┘ └──────┬────────┘
       │                                             │
       └──────────────► 同一个 SQLite 文件 ◄──────────┘
        memories 表（向量 BLOB）   graph_entities / graph_edges 表
```

## 设计决策（对比主流项目）

| 维度 | 本项目 | Mem0 | Letta (MemGPT) | Zep / Graphiti |
|---|---|---|---|---|
| 定位 | 轻量记忆层 | 通用记忆 SDK | 状态化 Agent 平台 | 时序知识图谱 |
| 记忆分类 | working/episodic/semantic | semantic/episodic | core/recall/archival | episodic/semantic |
| 存储 | SQLite 内嵌（记忆+图谱同库） | 向量库+图谱 | 文件/Postgres | 图数据库(Neo4j 等) |
| 检索 | BM25+向量+**内置图谱多跳** | 向量+图谱 | 工具分页 | 图谱遍历 |
| 图谱依赖 | **无**（SQLite 三元组即可） | 外部图库可选 | 无 | 必须图数据库 |
| 冲突解决 | superseded 标记 + 图边时间失效 | 更新覆盖 | 自编辑 | 双时间窗口 |
| 关系抽取 | 规则离线兜底 / LLM 可换 | LLM | LLM | LLM |
| 离线可用 | ✅ 零配置 | ❌ 需向量库/LLM | ⚠️ 需配置 | ❌ 需图库 |
| 语言 | Rust | Python | Python | Python |

> 选型建议：想要"即插即用、开箱即跑"选本项目；要跑生产级长时 Agent 看 Letta；需要企业级时间推理看 Zep/Graphiti；深度集成 LangChain 生态看 LangMem。

## API 参考

- `AgentMemory::open(path)` — 打开记忆库（`":memory:"` 为内存库）
- `add(scope, key, type, content)` — 写入，自动评估重要性
- `remember_fact(scope, key, content)` / `remember_event(...)` — 便捷入口
- `add_with_importance(...)` — 显式重要性 / TTL / 元数据
- `recall(scope, key, query)` → `Vec<ScoredMemory>` — 混合检索
- `recall_with_graph(scope, key, query)` → `(Vec<ScoredMemory>, Vec<Edge>)` — 记忆 + 实体关系一并召回
- `supersede(scope, key, old_id, new_content)` — 冲突解决
- `forget(id)` / `prune()` / `consolidate(scope, key, threshold)`
- `list(scope, key, type)` / `stats(scope, key)` / `get(id)`
- **图谱**：`remember_relation(s,p,o)`（多值）/ `replace_relation(s,p,o)`（单值变更）/ `graph_neighbors(entity, depth)` / `graph_paths(from,to,depth)` / `graph_entities()` / `graph_edges(include_invalid)` / `invalidate_relation(s,p)`
- `with_store(...)` — 注入自定义存储/嵌入器/检索配置
- `with_graph(Arc<dyn GraphStore>)` / `with_extractor(Arc<dyn Extractor>)` — 挂载自定义图谱后端 / 抽取器

类型：`MemoryType { Working, Episodic, Semantic }`、`Scope { User, Session, Agent }`、`MemoryItem`、`ScoredMemory`、`Triple`、`Entity`、`Edge`。

## 可插拔扩展

```rust
// 换一个真实语义嵌入器（例如 OpenAI 兼容接口）
struct MyEmbedder;
impl Embedder for MyEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> { /* 调用你的模型 */ }
    fn dim(&self) -> usize { 1024 }
}

// 换一个 LLM 摘要器
struct LlmSummarizer;
impl Summarizer for LlmSummarizer {
    fn summarize(&self, text: &str, max: usize) -> String { /* LLM 调用 */ }
}

// 用 LLM 做实体关系抽取（输出 JSON 三元组），替换离线规则抽取器
struct LlmExtractor;
impl Extractor for LlmExtractor {
    fn extract(&self, text: &str) -> Vec<Triple> { /* 让模型抽取 subject/predicate/object */ vec![] }
}

// 换成真实图数据库后端（如 Neo4j），实现 GraphStore trait 即可
struct Neo4jGraph;
impl GraphStore for Neo4jGraph {
    fn add_triple(&self, t: &Triple, src: Option<i64>, now: i64) -> StoreResult<i64> {
        // 用 Cypher 写节点和边
        todo!()
    }
    fn replace_triple(&self, t: &Triple, src: Option<i64>, now: i64) -> StoreResult<i64> { todo!() }
    fn neighbors(&self, entity: &str, depth: usize) -> StoreResult<Vec<Edge>> { todo!() }
    fn find_paths(&self, from: &str, to: &str, max: usize) -> StoreResult<Vec<Vec<Edge>>> { todo!() }
    fn entities(&self) -> StoreResult<Vec<Entity>> { todo!() }
    fn edges(&self, include_invalid: bool) -> StoreResult<Vec<Edge>> { todo!() }
    fn invalidate(&self, s: &str, p: &str, now: i64) -> StoreResult<usize> { todo!() }
}
```

> 内置 `RuleExtractor` 只覆盖「X 喜欢 Y」「X lives in Y」这类显式句式（置信度 0.6），用于离线兜底；真实业务建议实现 `Extractor` 接 LLM，图谱质量会显著提升，但存储与多跳检索逻辑无需改动。

## 开发与测试

```bash
cargo build          # 编译（首次会编译 bundled SQLite）
cargo test           # 53 个测试（单元 + 集成 + 文档）
cargo clippy --all-targets   # 无警告（-D warnings）
cargo fmt
cargo run --example demo     # 端到端演示（含知识图谱多跳推理）
```

## Roadmap

- [x] 三类记忆 + 三级作用域 + 混合检索打分
- [x] 冲突解决 / TTL 遗忘 / 语义整合
- [x] 轻量知识图谱（SQLite 三元组、多跳路径、时间失效）
- [x] 离线中英规则实体关系抽取（LLM 抽取 trait 可换）
- [ ] HTTP 嵌入后端（OpenAI / BGE 兼容，`features=["http"]`）
- [ ] LLM 抽取器/摘要器接入（更高质量的三元组与蒸馏）
- [ ] 社区发现 / 实体消歧 / 图谱摘要
- [ ] 时序检索（按时间范围过滤、最新事实优先）
- [ ] 持久化并发优化（连接池 / WAL 调优）
- [ ] `cargo publish` 到 crates.io

## License

[MIT](./LICENSE)
