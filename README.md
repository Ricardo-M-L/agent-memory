# agent-memory

> 离线优先、零配置的 LLM Agent 记忆层（Rust 实现）

[![CI](https://github.com/Ricardo-M-L/agent-memory/actions/workflows/ci.yml/badge.svg)](https://github.com/Ricardo-M-L/agent-memory/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)
[![Crates.io](https://img.shields.io/crates/v/agent-memory.svg)](https://crates.io/crates/agent-memory)
[![Docs](https://img.shields.io/docsrs/agent-memory)](https://docs.rs/agent-memory)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)

`agent-memory` 是一个为 LLM Agent 提供**持久化记忆**的轻量 Rust 库 + CLI。它把业界主流 AI 记忆系统（Mem0 / Letta / Zep 等）的设计精华压缩成一个**无需任何外部服务**就能跑起来的实现：内置 SQLite 存储、特征哈希嵌入与轻量知识图谱，无需向量数据库、无需图数据库（Neo4j）、无需 API key、无需联网。

它同时提供三条互补的记忆通道——**关键词（BM25）+ 向量（语义相似）+ 知识图谱（实体关系/多跳推理）**，覆盖"相似内容召回"与"实体关系推理"两类不同需求。

```
cargo add agent-memory   # 作为库
cargo run -- demo        # 作为 CLI 演示
cargo run --example demo # 运行示例程序
```

## 它解决什么问题

LLM 的上下文窗口是它的"短期记忆"——窗口一满，之前的信息就丢了。Agent 要跨会话记住用户偏好、历史事件、领域知识，就必须把记忆**存到窗口之外**，并在需要时**检索回来**。这就是 Agentic Memory 的核心工程问题：存什么、何时取、如何更新、如何遗忘。本项目用一套小而完整的实现回答这些问题。

## 核心特性

| 特性 | 说明 |
|---|---|
| **三类记忆** | `working`（工作/短期，滚动淘汰）· `episodic`（情景，事件历史）· `semantic`（语义，事实/偏好） |
| **三级作用域** | `user` / `session` / `agent` 隔离，不同主体互不串记忆 |
| **混合检索打分** | `时效 × 相关 × 重要` 加权（源自 *Generative Agents* 论文）；相关度 = BM25 关键词 + 可选向量余弦 |
| **轻量知识图谱** | 实体 + 关系三元组（`subject -predicate-> object`），支持无向邻居、多跳路径、**社区发现（连通分量）、实体消歧合并、图谱统计**；**不依赖 Neo4j**，纯 SQLite 实现（对齐 Zep/Graphiti 图记忆） |
| **实体关系抽取** | `Extractor` trait + 离线中英规则抽取器兜底；内置注入式 `LlmExtractor`（网络由 `ChatClient` 提供，健壮解析 JSON 三元组） |
| **时序检索** | `recall_between` / `list_between` 配合 `TimeRange` 按创建时间窗口过滤 |
| **图上时间冲突** | 单值关系（住在/任职于）变更时旧边标记 `invalidated_at` 不删除、可追溯；多值关系（喜欢/使用）共存 |
| **冲突解决** | 新事实取代旧事实时，旧条目标记 `superseded` 而非静默覆盖（可追溯，对齐 Zep 时序图谱思想） |
| **遗忘机制** | TTL 过期自动清理 + 手动 `prune`，防止记忆无限膨胀（对齐 Letta 归档层） |
| **整合去重** | `consolidate` 按向量相似度合并重复语义记忆（对齐 LangMem 蒸馏层级） |
| **离线开箱即用** | 内置 `HashEmbedder`（特征哈希嵌入）+ `RuleExtractor`（规则抽取），无模型、无网络、无配置 |
| **可选联网能力** | `http` feature 提供 OpenAI 兼容 `OpenAiEmbedder`（ureq+rustls，传输层可注入）；**默认构建零网络依赖** |
| **可插拔** | `Embedder` / `MemoryStore` / `Summarizer` / `Extractor` / `GraphStore` / `ChatClient` / `HttpTransport` 等 trait，可无缝替换为真实模型/存储/LLM/图数据库 |
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
agent-memory --db ./agent-memory.db add user dev semantic 用户喜欢用 Rust 写 Agent 应用
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
agent-memory graph communities               # 社区发现（弱连通分量）
agent-memory graph merge Rust Rust语言       # 把别名「Rust语言」合并进「Rust」
agent-memory graph stats                     # 实体/边/社区统计
agent-memory graph edges --all                # 含已失效边
agent-memory demo      # 端到端演示
agent-memory --version
agent-memory help
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
│   检索: 候选过滤(含时间窗口) → BM25+向量 → 时效×相关×重要 → Top-K    │
│   图谱: 邻居扩展 / 多跳路径 / 社区发现 / 实体合并 / 单值关系时间失效  │
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
- `recall_between(scope, key, query, TimeRange)` — 限定创建时间窗口的检索（另配 `list_between`、`TimeRange::since/until/between`）
- `recall_with_graph(scope, key, query)` → `(Vec<ScoredMemory>, Vec<Edge>)` — 记忆 + 实体关系一并召回
- `supersede(scope, key, old_id, new_content)` — 冲突解决
- `forget(id)` / `prune()` / `consolidate(scope, key, threshold)`
- `list(scope, key, type)` / `stats(scope, key)` / `get(id)`
- **图谱**：`remember_relation(s,p,o)`（多值）/ `replace_relation(s,p,o)`（单值变更）/ `graph_neighbors(entity, depth)` / `graph_paths(from,to,depth)` / `graph_entities()` / `graph_edges(include_invalid)` / `invalidate_relation(s,p)` / `graph_communities()`（社区发现）/ `merge_graph_entities(keep, alias)`（实体消歧）/ `graph_summary()`（图谱统计）
- `with_store(...)` — 注入自定义存储/嵌入器/检索配置
- `with_graph(Arc<dyn GraphStore>)` / `with_extractor(Arc<dyn Extractor>)` — 挂载自定义图谱后端 / 抽取器

类型：`MemoryType { Working, Episodic, Semantic }`、`Scope { User, Session, Agent }`、`MemoryItem`、`ScoredMemory`、`TimeRange`、`Triple`、`Entity`、`Edge`、`GraphStats`。

## 可插拔扩展

### 自定义嵌入器（`Embedder` 现在是可失败签名，便于接网络模型）

```rust
use agent_memory::{Embedder, StoreResult};

struct MyEmbedder;
impl Embedder for MyEmbedder {
    fn embed(&self, text: &str) -> StoreResult<Vec<f32>> {
        // 调用你的模型；网络/鉴权失败可返回 StoreError
        Ok(vec![])
    }
    fn dim(&self) -> usize { 1024 }
}
```

### 内置 LLM 抽取器（注入 `ChatClient`，不绑定网络库）

```rust
use agent_memory::{LlmExtractor, Extractor};

// 闭包即可作为 ChatClient：在这里对接 OpenAI / Ollama / 内网网关
let llm = LlmExtractor::new(|prompt: &str| -> Result<String, String> {
    let body = my_http_post(prompt)?;   // 由你决定用什么 HTTP 栈
    Ok(body)
});
let triples = llm.extract("用户喜欢 Rust，住在上海"); // 自动解析 JSON 三元组
```

### 可选 `http` feature：OpenAI 兼容嵌入器

```toml
[dependencies]
agent-memory = { version = "0.1", features = ["http"] }
```

```rust
# #[cfg(feature = "http")] fn main() -> agent_memory::StoreResult<()> {
use agent_memory::http_embed::OpenAiEmbedder;
use agent_memory::Embedder;

// 兼容任何 /v1/embeddings 格式服务：OpenAI、vLLM、本地 TEI、各类网关
let emb = OpenAiEmbedder::new("sk-...", "text-embedding-3-small", 1536)
    .with_endpoint("https://你的网关/v1/embeddings"); // 可选：覆盖 endpoint
let v = emb.embed("一段文本")?;
# Ok(()) }
```

> 传输层抽象为 `HttpTransport`（默认 `ureq`+rustls，无需系统 OpenSSL），可注入 mock 或
> 其他 HTTP 客户端；**不启用 `http` feature 时，整个库零网络依赖**。

### 可选 `neo4j` feature：接入 Neo4j 图数据库

节点和关系存入独立的 Neo4j 服务，通过 HTTP Query API 连接；可以连接本机服务或远程
单机实例。默认仍使用 SQLite。此特性尚未发布到 crates.io，先使用 Git 依赖：

```toml
[dependencies]
agent-memory = { git = "https://github.com/Ricardo-M-L/agent-memory", features = ["neo4j"] }
```

```rust
use std::sync::Arc;
use agent_memory::{AgentMemory, Neo4jGraphConfig, Neo4jGraphStore, RuleExtractor};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph = Neo4jGraphStore::connect(
        Neo4jGraphConfig::basic("http://localhost:7474", "neo4j", std::env::var("NEO4J_PASSWORD")?)
            .with_database("neo4j")
            .with_namespace("my-agent"),
    )?;
    let _mem = AgentMemory::open("agent-memory.db")?
        .with_graph(Arc::new(graph))
        .with_extractor(Arc::new(RuleExtractor::new()));
    Ok(())
}
```

完整的 [启动、可视化、测试和使用边界](docs/neo4j.md)，以及可直接运行的
[Neo4j 示例](examples/neo4j.rs)。支持实体/关系、时间失效、多跳路径、社区、实体合并和统计。

> 图谱按 `namespace` 隔离，**不会自动继承记忆的 user/session/agent 作用域**；多租户需由
> 应用选择独立的后端实例与命名空间。记忆正文仍在 SQLite，与 Neo4j 写入不构成跨库事务。
> 切换后端不会自动迁移已有 SQLite 图谱；当前 CLI 的 `graph` 命令仍使用 SQLite。

### 自定义图谱后端 / 摘要器

实现 `GraphStore` trait 即可换成 Neo4j 等真实图数据库（邻居、路径、社区、合并、统计等
方法都需实现）；实现 `Summarizer` trait 可把内置抽取式摘要替换为 LLM 摘要。

> 内置 `RuleExtractor` 只覆盖「X 喜欢 Y」「X lives in Y」这类显式句式（置信度 0.6），用于离线兜底；真实业务建议用内置 `LlmExtractor` 接 LLM，图谱质量会显著提升，但存储与多跳检索逻辑无需改动。

## 开发与测试

```bash
cargo build                          # 编译（首次会编译 bundled SQLite）
cargo test                           # 全部测试（单元 + 集成 + 文档）
cargo test --all-features            # 含可选 HTTP/Neo4j 离线测试；真实 Neo4j 测试见 docs/neo4j.md
cargo clippy --all-targets --all-features -- -D warnings   # 零警告
cargo fmt
cargo run -- demo                     # CLI 端到端演示（含知识图谱多跳推理）
cargo run --example demo             # 示例程序演示（可选）
```

更多开发约定见 [CONTRIBUTING.md](./CONTRIBUTING.md)，版本变更见 [CHANGELOG.md](./CHANGELOG.md)。

## Roadmap

- [x] 三类记忆 + 三级作用域 + 混合检索打分
- [x] 冲突解决 / TTL 遗忘 / 语义整合
- [x] 轻量知识图谱（SQLite 三元组、多跳路径、时间失效）
- [x] 离线中英规则实体关系抽取（LLM 抽取 trait 可换）
- [x] 时序检索（按时间范围过滤：`recall_between` / `list_between` / `TimeRange`）
- [x] 社区发现 / 实体消歧合并 / 图谱统计摘要
- [x] 注入式 LLM 抽取器（`LlmExtractor` + `ChatClient`，不绑定网络库）
- [x] HTTP 嵌入后端（OpenAI / BGE 兼容，`features=["http"]`，默认不启用）
- [x] Neo4j 图谱后端（`features=["neo4j"]`，默认不启用）
- [ ] LLM 摘要器的内置实现（`Summarizer` trait 已就绪）
- [ ] 持久化并发优化（连接池 / WAL 调优）
- [ ] `cargo publish` 到 crates.io

## License

[MIT](./LICENSE)
