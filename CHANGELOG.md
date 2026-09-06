# 更新日志（Changelog）

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 图谱后端
- 新增 `neo4j` feature：实现可选 `Neo4jGraphStore`，通过 Neo4j HTTP Query API
  提供完整 `GraphStore` 实现（add/replace/list/邻居/路径/社区/合并/统计），用于替换默认
  SQLite 图谱后端。
- 增加命名空间隔离、数据库端写锁、持久化 ID、原子关系更新与实体合并。
- 增加 `Neo4jGraphConfig`（Basic/Bearer 认证、数据库、超时、响应大小限制）与凭据脱敏。
- 增加真实 Neo4j 集成测试、独立 CI 服务任务、可运行示例和启动/可视化文档。

### 修复
- SQLite 的实体更新、关系失效与插入改为同一个事务，避免失败或并发下部分写入。
- `replace_triple` 可恢复已失效的同一事实；重复有效事实保留 ID 与来源，修复实体计数翻倍。
- 两个后端统一拒绝空三元组和非有限/越界置信度，重复有效事实可提升置信度。

## [0.1.0] - 2026-09-03

首个开源版本：离线优先、零外部服务、零 API key 即可运行的 LLM Agent 记忆层。

### 新增

#### 记忆核心
- 三类记忆：工作（working，容量上限自动淘汰）、情景（episodic）、语义（semantic）。
- 三级作用域隔离：user / session / agent。
- 混合检索：BM25 关键词 + 特征哈希向量余弦，按 `时效 × 相关 × 重要` 打分排序。
- 冲突解决：新事实写入时旧条目标记 `superseded`，不静默覆盖、可追溯。
- 遗忘机制：TTL 过期、主动 `prune` 清理。
- 整合机制：语义去重合并（consolidate）。
- 时序检索：`recall_between` / `list_between` 配合 `TimeRange` 按创建时间过滤。

#### 轻量知识图谱（不依赖 Neo4j）
- 实体 / 关系三元组与记忆同库（SQLite），边可溯源到来源记忆。
- `add_triple`（多值共存、幂等）与 `replace_triple`（单值事实变更，旧边失效留痕）。
- 邻居查询（BFS）、两实体间多跳路径（DFS）。
- 社区发现：弱连通分量（并查集）。
- 实体消歧：`merge_entities` 合并别名实体，自动改接边、去自环、合并重复边。
- 图谱统计摘要：实体 / 有效边 / 失效边 / 社区数 / 最大社区。

#### 可插拔抽象
- `Embedder`：内置离线 `HashEmbedder`（特征哈希 + L2 归一化），可替换为真实模型。
- `MemoryStore`：内置 `SqliteStore`（rusqlite bundled，内嵌 SQLite），可替换后端。
- `Summarizer`：内置抽取式摘要，可替换为 LLM 摘要。
- `Extractor`：内置中英规则 `RuleExtractor`；新增注入式 `LlmExtractor`（网络由 `ChatClient`
  注入，健壮解析 JSON 三元组，离线可 mock 测试，不绑定具体 HTTP 库）。

#### 可选 `http` feature
- `OpenAiEmbedder`：兼容 OpenAI `/v1/embeddings` 格式的远端嵌入器（OpenAI / vLLM /
  text-embeddings-inference / 各类网关）。
- 传输层抽象 `HttpTransport`，默认 `ureq` + rustls（无需系统 OpenSSL），可注入自定义客户端。
- 默认构建不启用、不引入任何网络依赖。

#### 工程化
- 命令行工具 `agent-memory`：记忆增删查、图谱增改查、社区/合并/统计、端到端 demo。
- 完整单元 / 集成测试与示例 `examples/demo.rs`。
- GitHub Actions CI：fmt 检查、clippy `-D warnings`（含 `--all-features`）、全量测试、docs 构建。
- MIT 协议、中文 README、贡献指南。

[0.1.0]: https://github.com/Ricardo-M-L/agent-memory/releases/tag/v0.1.0
