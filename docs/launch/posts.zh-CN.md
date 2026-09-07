# 中文首发材料

状态：待发布。以下文案面向 Rust 中文社区和 V2EX 个人作品分享；发布前检查登录状态与当日版规。
不得把本文文件存在、或 GitHub 推送完成，记为社区帖子已发布。

## V2EX：分享创造

标题：做了一个 Rust Agent 记忆库：默认 SQLite，支持旧事实失效和可选 Neo4j

正文：

分享一个早期 MIT 开源项目 `agent-memory`，主要给用 Rust 写 Agent、聊天助手或 CLI 的开发者用。

想解决的具体问题是：Agent 记住了“用户住北京”，后来用户搬到上海，如何更新记忆，同时还能
检查旧事实？这里提供显式的正文替换和关系失效 API。默认把正文、向量和实体关系保存在 SQLite，
不要求先运行向量库、图数据库或申请模型 API key。

可以直接试这个演示：

```bash
git clone https://github.com/Ricardo-M-L/agent-memory.git
cd agent-memory
cargo run --example fact_history
```

它启动三个独立进程：写入北京、重启更新上海、再重启验证当前记忆与旧关系，并查询
`alice → shanghai → china` 两跳路径。数据是合成示例，结果有断言；不调用 LLM。

其他能力包括 BM25 + 特征哈希向量检索、规则/注入式 LLM 抽取、实体合并和可选 Neo4j 后端。
启用 Neo4j 后节点与关系真正在 Neo4j 中，正文仍在 SQLite。

也把边界讲清楚：默认特征哈希不是语义模型；正文/图谱更新由应用显式决定，不自动判断矛盾；
图谱不会自动继承 user/session/agent 作用域；没有跨库事务、大图性能承诺或现成 MCP/Python SDK。

项目：https://github.com/Ricardo-M-L/agent-memory

比较想听正在做 Rust Agent 的朋友说说：接入时最缺哪一块？是模型嵌入示例、图谱隔离接口，
还是跨语言接入？欢迎提交可复现问题；如果对你的项目有用，也欢迎收藏仓库关注后续。

披露：开发和这份介绍使用了 AI 编程助手，具体实现、示例与 CI 可在仓库核对。

## Rust 中文社区：技术介绍

标题：Rust 本地记忆层中的事实更新：SQLite、关系失效与可选 Neo4j

使用上面的可运行介绍，增加以下技术段落，避免同站重复发布近似内容：

在 API 上区分 `remember_relation`（多值共存）和 `replace_relation`（同主体/谓词的单值替换）。
SQLite 图谱的实体 upsert、旧边失效、新边写入放在一个事务中；Neo4j 适配器用命名空间元数据节点
协调其客户端的写入。但一组记忆写入与图谱调用并不是端到端原子事务，这部分限制有独立文档。

检索打分是时效、相关度、重要性的加权求和。图算法目前在 Rust 侧加载有效边后执行，适合先做
小规模接入验证；没有拿一个两跳演示来宣称大图性能。

技术说明：https://github.com/Ricardo-M-L/agent-memory/blob/main/docs/fact-history.md
贡献指南：https://github.com/Ricardo-M-L/agent-memory/blob/main/CONTRIBUTING.md
