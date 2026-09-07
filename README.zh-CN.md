# agent-memory

**给 Rust Agent 加上持久化记忆：默认 SQLite，可选 Neo4j。**

[English](README.md) · [简体中文](README.zh-CN.md)

[![CI](https://github.com/Ricardo-M-L/agent-memory/actions/workflows/ci.yml/badge.svg)](https://github.com/Ricardo-M-L/agent-memory/actions/workflows/ci.yml)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

不用先搭数据库服务，就能保存事实、检索记忆、查询实体关系。事实变化时，由应用显式更新，
并保留被取代的记忆和已失效的关系供检查。默认运行无需 API key、模型下载或联网。

这是**早期 Rust 库 + CLI**，不是托管平台或自主 Agent。
默认向量是特征哈希，**不是模型提供的语义嵌入**。

[![SQLite 事实更新的真实输出摘录，以及两跳实体关系示意图](https://raw.githubusercontent.com/Ricardo-M-L/agent-memory/main/videos/agent-memory-launch/snapshots/frame-03-at-43.3s.png)](docs/fact-history.md)

图中是可运行示例的真实输出摘录与 SQLite 关系示意，不是 Neo4j Browser 截图。

## 直接试用

需要 Rust stable 和用于编译 bundled SQLite 的 C/C++ 工具链。
第一次构建会下载 Cargo 依赖，默认程序编译后可离线运行。

```bash
git clone https://github.com/Ricardo-M-L/agent-memory.git
cd agent-memory
cargo run --example quickstart
cargo run --example fact_history
```

[事实更新演示](examples/fact_history.rs)会启动三个独立进程：

1. 保存 `alice -lives_in-> beijing`。
2. 重新打开 SQLite，把居住地更新为 `shanghai`，北京关系标记失效。
3. 再次重启，验证当前记忆、旧关系和 `alice → shanghai → china` 两跳路径。

结果均有断言检查，使用独立临时数据库，结束后清理自身数据。
这是应用显式调用 API 的真实输出，没有使用 LLM。

## 接入 Rust 项目

目前尚未发布到 crates.io，请先使用已测试的源码 Release：

```toml
[dependencies]
agent-memory = { git = "https://github.com/Ricardo-M-L/agent-memory", tag = "v0.1.1" }
```

```rust
use agent_memory::{types::Scope, AgentMemory, StoreResult};

fn main() -> StoreResult<()> {
    let memory = AgentMemory::open("agent-memory.db")?;
    let old = memory.remember_fact(Scope::User, "alice", "Alice lives in Beijing")?;
    memory.supersede(Scope::User, "alice", old.id, "Alice lives in Shanghai")?;
    for hit in memory.recall(Scope::User, "alice", "Alice lives")? {
        println!("{}", hit.item.content);
    }
    Ok(())
}
```

把检索结果作为不可信上下文交给你的模型，不要把记忆中的文本当作系统指令。
完整的[最小接入示例](examples/quickstart.rs)同时演示图谱更新，CI 会实际执行它。

从上面的仓库目录安装 CLI：

```bash
cargo install --path .
agent-memory --db ./agent-memory.db add user alice semantic "Alice likes Rust"
agent-memory --db ./agent-memory.db recall user alice "Rust"
agent-memory help
```

## 已有能力

- 工作 / 情景 / 语义三类记忆，SQLite 持久化。
- BM25 与特征哈希向量混合检索；时效、相关度和重要性**加权求和**，并非三项相乘。
- 显式记忆替换、TTL 过期过滤、手动 `prune`、基于相似度的整合。
- 实体、关系、多跳路径、无向邻居、手动实体合并、连通分量和图统计。
- 单值关系用 `replace_relation` 更新并失效旧边；多值关系用 `remember_relation` 共存。
- 可选规则抽取器 / 注入式 LLM 抽取器，以及 `http` 模型嵌入特性。
- 可选 `neo4j` 后端，实体与关系真正存入 Neo4j。见[启动、可视化与边界](docs/neo4j.md)。

## 使用边界

- 默认 SQLite 无外部服务。Neo4j 需要显式启用并启动独立服务；正文和向量仍由记忆后端存储。
- 图谱不自动继承 user/session/agent 记忆作用域。同一 graph store 共享图；多租户需要独立后端/
  namespace 和应用鉴权，namespace 本身不是权限系统。
- `remember_fact` 不会自动检测矛盾。正文用 `supersede`，单值关系用 `replace_relation`；
  自动抽取只新增三元组，不自动决定旧关系是否失效。
- 正文、嵌入、图谱写入不构成端到端事务；SQLite 与 Neo4j 没有跨库事务，应用应处理部分失败。
- 关系重新激活会复用三元组记录，不是完整事件日志或任意历史时刻回放。
- 两个后端的图算法都在 Rust 侧加载边后计算；暂无大图性能保证、分页或原生 Cypher 遍历。
- 不自动迁移旧 SQLite 图谱，CLI 暂不选择 Neo4j；没有现成 MCP 服务或 Python/TypeScript SDK。
- 默认特征哈希 / 规则抽取不等于模型理解（详见[规则抽取行为与示例矩阵](docs/guide.md#rule-extraction-behavior-matrix)）；启用外部模型时数据会按配置发送。

## 开发与参与

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run --example quickstart
cargo run --example fact_history
cargo doc --all-features --no-deps
```

普通测试不要求 Neo4j，真实服务测试有独立 CI 任务。

[贡献指南](CONTRIBUTING.md) · [路线图](ROADMAP.md) · [更新日志](CHANGELOG.md) ·
[接入/API 指南](docs/guide.md) · [参与任务](https://github.com/Ricardo-M-L/agent-memory/issues?q=is%3Aissue%20is%3Aopen%20label%3A%22help%20wanted%22)

如果项目对你有用，欢迎 Star 方便之后找到；也欢迎提交真实接入反馈或最小问题复现。
许可证：[MIT](LICENSE)。
