# Neo4j 图谱后端

启用 `neo4j` feature 后，`Neo4jGraphStore` 实现完整的 `GraphStore` 接口。
实体是 Neo4j 节点，三元组是节点之间的有向关系；记忆正文和向量仍由原记忆后端存储。
使用 [Query API](https://neo4j.com/docs/query-api/current/query/) 的
`/db/{database}/query/v2`，适配 Neo4j 5.26+；本地回归基线为 Community 5.26.12。
无需 APOC、GDS 插件或 Bolt 客户端。当前支持单服务地址，不实现集群路由或 bookmarks。

## 本地运行

下面启动一个仅绑定本机端口、使用临时容器数据的测试服务；停止容器后数据会被删除。
先设置测试密码（示例值只用于本地演示）：

```bash
export NEO4J_PASSWORD=agent-memory-demo-only
docker run --rm --name agent-memory-neo4j \
  -p 127.0.0.1:7474:7474 -p 127.0.0.1:7687:7687 \
  -e NEO4J_AUTH="neo4j/$NEO4J_PASSWORD" \
  neo4j:5.26.30
```

等待日志出现 `Started`，在另一个设置了相同密码的终端运行：

```bash
export NEO4J_PASSWORD=agent-memory-demo-only
cargo run --features neo4j --example neo4j
```

示例将英文事实抽取为 `alice -lives_in-> shanghai`，补充
`shanghai -located_in-> china`，查询两跳路径并打印图统计。
英文规则抽取会转小写；直接写入图谱的名称仅去首尾空白，大小写仍有区别。

可配置环境变量：`NEO4J_ENDPOINT`（默认 `http://localhost:7474`）、
`NEO4J_USERNAME`、`NEO4J_PASSWORD`、`NEO4J_DATABASE`、`NEO4J_NAMESPACE`
（示例默认 `agent-memory-demo`）、`MEMORY_DB`（默认 `neo4j-demo.db`）。
库本身不读取环境变量，配置由调用者显式传入。远程连接使用 HTTPS 和独立凭据。

## 初始化与数据模型

`Neo4jGraphStore::connect(config)` 校验配置并幂等创建两条唯一约束和命名空间元数据，
因此需要建约束权限。部署时也可由管理员先调用 `connect`，运行账号使用
`new(config)` 连接同一已初始化命名空间；普通读写不会重复执行建约束语句。
`new` 仅校验配置，不验证服务器是否可达。
并发冷启动时，Neo4j 可能因建约束锁竞争而中止一个事务；初始化遇到明确的
`DeadlockDetected` 最多退避重试三次。认证失败、网络结果未知和普通数据写入不重试。

- `(:AgentMemoryGraph {name, next_id})`：命名空间元数据与 ID 分配器。
- `(:AgentMemoryEntity {namespace, name, id, entity_type, mention_count, first_seen, last_seen})`：实体。
- `(s)-[:AGENT_MEMORY_RELATION {id, predicate, source_memory_id, confidence, created_at, invalidated_at}]->(o)`：关系。

关系种类保存在 `predicate` 属性中，例如 `lives_in`、`使用`；不把不受信任的文本拼接进
Cypher 关系类型。`entity_type` 当前可为空，现有 `Triple` 抽取接口尚不传递实体类型。
ID 由命名空间内计数器生成，可能有间隙，不依赖 Neo4j 的内部节点/关系 ID。

打开 Neo4j Browser（本机默认 `http://localhost:7474`），登录后执行：

```cypher
MATCH (s:AgentMemoryEntity {namespace: 'agent-memory-demo'})
      -[r:AGENT_MEMORY_RELATION]->(o:AgentMemoryEntity {namespace: 'agent-memory-demo'})
WHERE r.invalidated_at IS NULL
RETURN s, r, o
```

图形结果中可将节点标题设为 `name`、关系标题设为 `predicate`；也可在表格视图查看属性。

## 事务与边界

每次新增、替换、失效或合并在一个数据库事务中完成；通过命名空间元数据节点的
[写锁](https://neo4j.com/docs/operations-manual/current/database-internals/concurrent-data-access/)
串行化本适配器的并发写入，跨进程也有效。不同命名空间使用不同锁。
直接在外部用 Cypher 修改这些节点不遵守该锁协议，不能享有相同的并发保证。

`add_triple` 支持多值关系，同一三元组返回相同 ID，并保留已经失效的状态；
`replace_triple` 失效其他客体并恢复/写入指定客体。重新恢复旧三元组时保留 ID，
更新来源、创建时间与置信度。每个三元组只有一条记录，**不保存每次激活/失效的完整事件日志**。
实体合并只改接别名关联边、合并重复边、移除因此产生的自环，保留其他边；
若别名还有本适配器之外的关系，合并报错并回滚，不删除那些外部关系。

命名空间不是权限系统，也不会自动映射 `Scope::User/Session/Agent`。
多租户应用应把请求路由到各自的 `AgentMemory`/图谱后端；共享同一 store 的各个 scope
仍共享图。一个 SQLite 记忆库应对应独立命名空间，避免 `source_memory_id` 跨库重号。

SQLite 记忆写入与 Neo4j 写入不构成跨库事务。自动抽取时若图谱写入失败，记忆正文可能
已经保存，且部分三元组可能已成功；调用方应根据错误检查并补写图谱。超时或响应中断后
写入结果可能未知，库不自动重试写请求。默认请求超时 30 秒、响应上限 16 MiB，禁止
HTTP 重定向；错误不回显凭据、查询参数或服务器原始错误正文。

邻居、路径、社区算法当前在 Rust 内存中计算，会读取命名空间的全部有效边；统计/社区
会加载实体与边。密集图的“全部简单路径”可能非常多，应限制深度和图规模。
读取遵循 Neo4j 的 read-committed 隔离级别，并发写入时多项统计不承诺严格快照。
这版尚无分页、原生 Cypher 图算法、大图性能基准、自动迁移或 CLI 后端选择。

## 测试

普通测试无须 Neo4j。真实测试使用随机命名空间，成功后只清理自身数据，不清空数据库；
失败时保留该测试命名空间供诊断。必须显式指定可写测试服务：

```bash
export NEO4J_ENDPOINT=http://localhost:7474
export NEO4J_PASSWORD=agent-memory-demo-only
export NEO4J_TEST_ALLOW_WRITE=1
cargo test --all-features --test neo4j_integration -- --ignored
```

覆盖：幂等写入、事实恢复、命名空间隔离、溯源、路径/社区/统计、实体合并（重复边、
孤立别名、自环）、故障回滚和独立客户端并发写入。CI 的 Neo4j 服务任务执行同一套测试。
