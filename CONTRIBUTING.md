# 贡献指南（Contributing）

感谢你对 `agent-memory` 感兴趣！这是一个离线优先的 Rust Agent 记忆库，欢迎提 issue、
修 bug、加特性、改进文档。

## 开发环境

- Rust stable（建议通过 [rustup](https://rustup.rs/) 安装；CI 始终使用最新 stable）。
- 无需数据库、无需 API key：默认构建零外部服务，SQLite 由 `rusqlite` 的 `bundled`
  feature 静态编译，开箱即跑。

## 常用命令

```bash
cargo build                       # 默认构建（无网络依赖）
cargo test                        # 全部测试（单元 + 集成）
cargo test --all-features         # 含可选 http feature 的测试
cargo run -- demo                 # 端到端演示
cargo run -- example              # 快速示例
cargo run -- graph stats          # CLI：图谱统计

cargo fmt                         # 格式化（提交前必跑）
cargo clippy --all-targets --all-features -- -D warnings   # 零警告是硬标准
cargo doc --all-features --no-deps --open                   # 本地预览文档
```

## 提交前检查清单

PR 必须在**最新 stable** 上同时满足：

1. `cargo fmt --all -- --check` 无 diff；
2. `cargo clippy --all-targets --all-features -- -D warnings` 零警告；
3. `cargo test --all-features` 全绿；
4. 为新逻辑补测试（纯函数单测 + 涉及 SQLite 的逻辑走集成测试 / `:memory:`）；
5. 公共项都有 `///` 文档注释，`cargo doc` 无警告。

## 架构速览

| 模块 | 职责 |
| --- | --- |
| `types.rs` | 记忆类型、作用域、写入/读取结构 |
| `text.rs` | 分词与 BM25 |
| `embed.rs` | `Embedder` trait 与离线 `HashEmbedder` |
| `retrieval.rs` | `时效 × 相关 × 重要` 融合打分 |
| `store.rs` | `MemoryStore` trait 与错误类型 |
| `db.rs` | 共享 SQLite 连接与建表（记忆表 + 图谱表同库） |
| `sqlite_store.rs` | SQLite 记忆后端（向量以 f32 BLOB 存储） |
| `graph.rs` | 知识图谱：三元组、邻居、路径、社区、实体合并、统计 |
| `extract.rs` | 三元组抽取：规则版 + 注入式 LLM 版 |
| `summarizer.rs` | 抽取式摘要 |
| `memory.rs` | `AgentMemory` 门面，串联以上全部能力 |
| `http_embed.rs` | **可选 `http` feature**：OpenAI 兼容嵌入器 |

### 设计红线（务必遵守）

- **离线优先**：默认构建不得引入任何网络 / 外部服务 / API key。任何联网能力必须放进
  可选 feature（如 `http`），或通过注入式 trait（如 `ChatClient`、`HttpTransport`）
  由调用方提供，保证核心库零网络依赖。
- **不引入重型图数据库**：知识图谱保持在嵌入式 SQLite 内实现。
- **可追溯而非静默覆盖**：事实变更用 `superseded` / `invalidated_at` 留痕，不物理删除历史。
- **多值与单值关系分开**：`add_triple` 允许多值共存，`replace_triple` 才表示单值事实变更。

## 代码风格

- 面向 trait 设计，新增后端优先实现既有 trait，而不是改门面签名。
- 错误统一走 `StoreError`，不要 `unwrap`/`panic` 出现在公共 API 路径上。
- 注释与文档使用简体中文，标识符使用英文。

## 提交与 PR

- 一个 PR 聚焦一件事，commit message 建议用
  [Conventional Commits](https://www.conventionalcommits.org/)（`feat:` / `fix:` /
  `docs:` / `test:` / `refactor:` / `chore:`）。
- 涉及行为变更请同步更新 `README.md` 与 `CHANGELOG.md`。
- 提 PR 时说明：动机、方案、测试方式；如有破坏性变更请显式标注 `BREAKING CHANGE`。

## 报告问题

提 issue 时请附上：Rust 版本（`rustc -V`）、操作系统、复现步骤、期望与实际结果、
最小复现代码或报错日志。
