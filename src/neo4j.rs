//! 可选 Neo4j 5.26+ 后端，使用 HTTP Query API（`/db/{db}/query/v2`）。
//!
//! 使用 [`Neo4jGraphStore::connect`] 初始化约束和命名空间；若管理员已完成初始化，
//! 可使用 [`Neo4jGraphStore::new`]。每次修改为单个事务，数据库端的命名空间写锁
//! 串行化本适配器的写入，跨进程同样有效。图算法目前加载数据后在 Rust 内存中计算。
//! 此同步适配器面向单机服务，不实现集群路由、bookmark 传递或自动写入重试。

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::io::Read;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use url::Url;

use crate::graph::{Edge, Entity, GraphStats, GraphStore, Triple};
use crate::store::{StoreError, StoreResult};

const LOCK: &str = "MATCH (g:AgentMemoryGraph {name: $namespace})
    SET g._lock = true REMOVE g._lock WITH g ";
const WRITE_TRIPLE: &str = include_str!("neo4j/write_triple.cypher");
const MERGE_ENTITIES: &str = include_str!("neo4j/merge_entities.cypher");
const EDGE_ROW: &str = "[r.id, s.name, r.predicate, o.name,
    r.source_memory_id, r.confidence, r.created_at, r.invalidated_at]";
const ENTITY_ROW: &str = "[n.id, n.name, n.entity_type,
    n.mention_count, n.first_seen, n.last_seen]";

fn protocol(message: impl Into<String>) -> StoreError {
    StoreError::Http(format!("Neo4j: {}", message.into()))
}

/// HTTP 认证方式；Debug 输出对凭据脱敏。
#[derive(Clone)]
pub enum Neo4jAuth {
    /// 用户名/密码认证。
    Basic { username: String, password: String },
    /// 令牌认证（服务器须支持相应认证提供方）。
    Bearer(String),
}

impl fmt::Debug for Neo4jAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Basic { .. } => "Basic([REDACTED])",
            Self::Bearer(_) => "Bearer([REDACTED])",
        })
    }
}

impl Neo4jAuth {
    fn header(&self) -> String {
        match self {
            Self::Basic { username, password } => {
                format!(
                    "Basic {}",
                    STANDARD.encode(format!("{username}:{password}"))
                )
            }
            Self::Bearer(token) => format!("Bearer {token}"),
        }
    }
}

/// 连接配置。凭据放在 `auth` 中，不能嵌入 URL。
#[derive(Debug, Clone)]
pub struct Neo4jGraphConfig {
    /// 服务根地址、`/db/name` 或完整 `/db/name/query/v2` 地址。
    pub endpoint: String,
    /// 数据库名；优先于 URL 中的名称，默认 `neo4j`。
    pub database: Option<String>,
    /// 显式认证信息。
    pub auth: Neo4jAuth,
    /// 应用/租户独立图谱，默认 `agent-memory`。
    /// 不会自动继承记忆的 scope 或 scope key。
    pub namespace: String,
    /// 每次请求超时（包含读取响应），默认 30 秒。
    pub timeout: Duration,
    /// 响应体大小上限，默认 16 MiB，可配置范围 1 字节至 256 MiB。
    pub max_response_bytes: usize,
}

impl Neo4jGraphConfig {
    /// 使用用户名和密码创建配置。
    pub fn basic(
        endpoint: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self::with_auth(
            endpoint,
            Neo4jAuth::Basic {
                username: username.into(),
                password: password.into(),
            },
        )
    }

    /// 使用 Bearer 令牌创建配置。
    pub fn bearer(endpoint: impl Into<String>, token: impl Into<String>) -> Self {
        Self::with_auth(endpoint, Neo4jAuth::Bearer(token.into()))
    }

    fn with_auth(endpoint: impl Into<String>, auth: Neo4jAuth) -> Self {
        Self {
            endpoint: endpoint.into(),
            database: None,
            auth,
            namespace: "agent-memory".into(),
            timeout: Duration::from_secs(30),
            max_response_bytes: 16 * 1024 * 1024,
        }
    }

    /// 覆盖数据库名。
    pub fn with_database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    /// 设置独立图谱命名空间。
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    /// 设置非零请求超时。
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// 同步、可克隆的图存储；克隆实例共享 HTTP 连接池。
#[derive(Debug, Clone)]
pub struct Neo4jGraphStore {
    query_url: String,
    auth: Neo4jAuth,
    namespace: String,
    client: ureq::Agent,
    max_response_bytes: usize,
}

impl Neo4jGraphStore {
    /// 校验配置，不发起网络请求。
    /// 写入前须调用 `initialize`，或由管理员先用 `connect` 初始化约束和命名空间。
    pub fn new(config: Neo4jGraphConfig) -> StoreResult<Self> {
        let mut url = Url::parse(&config.endpoint).map_err(|_| protocol("invalid endpoint URL"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(protocol(
                "endpoint must be HTTP(S), without credentials, query or fragment",
            ));
        }
        let path = url.path().trim_matches('/');
        let parts: Vec<_> = path.split('/').collect();
        let endpoint_db = match parts.as_slice() {
            [""] => None,
            ["db", db] | ["db", db, "query", "v2"] => Some((*db).to_string()),
            _ => {
                return Err(protocol(
                    "expected server root, /db/name, or /db/name/query/v2",
                ))
            }
        };
        let db = config
            .database
            .or(endpoint_db)
            .unwrap_or_else(|| "neo4j".into());
        if db.is_empty()
            || db == "."
            || db == ".."
            || !db
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err(protocol("invalid database name"));
        }
        if config.namespace.trim().is_empty()
            || config.namespace.len() > 200
            || config.timeout.is_zero()
            || config.max_response_bytes == 0
            || config.max_response_bytes > 256 * 1024 * 1024
        {
            return Err(protocol(
                "invalid namespace, timeout, or response size limit",
            ));
        }
        match &config.auth {
            Neo4jAuth::Basic { username, .. } if username.contains(':') => {
                return Err(protocol("Basic username cannot contain ':'"))
            }
            Neo4jAuth::Bearer(token) if token.is_empty() || token.chars().any(char::is_control) => {
                return Err(protocol("invalid bearer token"))
            }
            _ => {}
        }
        url.set_path(&format!("/db/{db}/query/v2"));
        Ok(Self {
            query_url: url.into(),
            auth: config.auth,
            namespace: config.namespace,
            client: ureq::AgentBuilder::new()
                .timeout(config.timeout)
                .redirects(0)
                .build(),
            max_response_bytes: config.max_response_bytes,
        })
    }

    /// 连接并幂等创建约束与命名空间元数据，需要建约束权限。
    /// 普通操作不重复创建约束。
    pub fn connect(config: Neo4jGraphConfig) -> StoreResult<Self> {
        let store = Self::new(config)?;
        store.initialize()?;
        Ok(store)
    }

    /// 幂等初始化服务端约束和命名空间；失败时可以修复权限/连接后再次调用。
    /// 并发初始化的服务端死锁最多重试三次；其他错误和普通数据写入不自动重试。
    pub fn initialize(&self) -> StoreResult<()> {
        self.initialize_statement("CREATE CONSTRAINT agent_memory_graph_unique IF NOT EXISTS FOR (g:AgentMemoryGraph) REQUIRE g.name IS UNIQUE")?;
        self.initialize_statement("CREATE CONSTRAINT agent_memory_entity_unique IF NOT EXISTS FOR (n:AgentMemoryEntity) REQUIRE (n.namespace, n.name) IS UNIQUE")?;
        self.initialize_statement("MERGE (g:AgentMemoryGraph {name: $namespace}) ON CREATE SET g.next_id = 0 RETURN g.next_id")?;
        Ok(())
    }

    fn initialize_statement(&self, statement: &str) -> StoreResult<()> {
        let mut retries = 0;
        loop {
            match self.execute(statement, json!({})) {
                Ok(_) => return Ok(()),
                Err(StoreError::Http(message)) if retries < 3 && message ==
                    "Neo4j: query failed (Neo.TransientError.Transaction.DeadlockDetected)" => {
                    // 服务端已回滚死锁事务，且这里只执行幂等初始化语句。
                    std::thread::sleep(Duration::from_millis(50 << retries));
                    retries += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn execute(&self, statement: &str, mut parameters: Value) -> StoreResult<Vec<Vec<Value>>> {
        parameters["namespace"] = json!(self.namespace);
        let response = match self
            .client
            .post(&self.query_url)
            .set("Authorization", &self.auth.header())
            .set("Accept", "application/json")
            .send_json(json!({"statement": statement, "parameters": parameters}))
        {
            Ok(response) | Err(ureq::Error::Status(_, response)) => response,
            // 不回显 URL、凭据、查询参数或服务端原始数据。
            Err(ureq::Error::Transport(_)) => {
                return Err(protocol(
                    "transport failed (connection/TLS/timeout); write outcome may be unknown",
                ))
            }
        };
        let status = response.status();
        if status != 202 && status != 200 && status < 400 {
            return Err(protocol(format!(
                "unexpected HTTP status {}",
                response.status()
            )));
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(self.max_response_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| protocol("response read failed; write outcome may be unknown"))?;
        if bytes.len() > self.max_response_bytes {
            return Err(protocol(
                "response exceeds configured size limit; write outcome may be unknown",
            ));
        }
        let payload: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status >= 400 {
                protocol(format!("HTTP {status}"))
            } else {
                protocol("invalid JSON response")
            }
        })?;
        if status >= 400 {
            // Query API 的查询错误也可能使用 HTTP 400，保留已脱敏的错误码。
            if payload
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|e| !e.is_empty())
            {
                return Self::parse_response(&payload);
            }
            return Err(protocol(format!("HTTP {status}")));
        }
        Self::parse_response(&payload)
    }

    fn parse_response(payload: &Value) -> StoreResult<Vec<Vec<Value>>> {
        if let Some(errors) = payload.get("errors") {
            let errors = errors
                .as_array()
                .ok_or_else(|| protocol("invalid errors field"))?;
            if let Some(error) = errors.first() {
                let code = error["code"].as_str().unwrap_or("unknown error");
                let code = if code.len() <= 160
                    && code.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'.')
                {
                    code
                } else {
                    "unknown error"
                };
                return Err(protocol(format!("query failed ({code})")));
            }
        }
        let fields = payload["data"]["fields"]
            .as_array()
            .ok_or_else(|| protocol("missing data.fields"))?;
        if fields.iter().any(|v| !v.is_string()) {
            return Err(protocol("invalid field name"));
        }
        let rows = payload["data"]["values"]
            .as_array()
            .ok_or_else(|| protocol("missing data.values"))?;
        rows.iter()
            .map(|row| {
                let row = row
                    .as_array()
                    .filter(|r| r.len() == fields.len())
                    .ok_or_else(|| protocol("invalid row width"))?;
                Ok(row.clone())
            })
            .collect()
    }

    fn write_triple(
        &self,
        triple: &Triple,
        source: Option<i64>,
        now: i64,
        replace: bool,
    ) -> StoreResult<i64> {
        if [triple.subject.as_str(), &triple.predicate, &triple.object]
            .iter()
            .any(|s| s.trim().is_empty())
            || !triple.confidence.is_finite()
            || !(0.0..=1.0).contains(&triple.confidence)
        {
            return Err(StoreError::Other(
                "triple names must be nonempty and confidence finite in [0,1]".into(),
            ));
        }
        let rows = self.execute(
            &format!("{LOCK}{WRITE_TRIPLE}"),
            json!({
                "subject": triple.subject.trim(), "predicate": triple.predicate.trim(),
                "object": triple.object.trim(), "confidence": triple.confidence,
                "source": source, "now": now, "replace": replace
            }),
        )?;
        scalar(&rows)
    }

    fn snapshot(&self) -> StoreResult<(Vec<Entity>, Vec<Edge>)> {
        let rows = self.execute(&format!("CALL {{ MATCH (n:AgentMemoryEntity {{namespace: $namespace}})
            WITH n ORDER BY n.id RETURN collect({ENTITY_ROW}) AS nodes }}
            CALL {{ MATCH (s:AgentMemoryEntity {{namespace: $namespace}})-[r:AGENT_MEMORY_RELATION]->(o:AgentMemoryEntity {{namespace: $namespace}})
            WITH s, r, o ORDER BY r.id RETURN collect({EDGE_ROW}) AS edges }} RETURN nodes, edges"), json!({}))?;
        let row = rows
            .first()
            .filter(|r| r.len() == 2)
            .ok_or_else(|| protocol("missing graph snapshot"))?;
        let entities = array(&row[0])?
            .iter()
            .map(parse_entity)
            .collect::<StoreResult<Vec<_>>>()?;
        let edges = array(&row[1])?
            .iter()
            .map(parse_edge)
            .collect::<StoreResult<Vec<_>>>()?;
        Ok((entities, edges))
    }
}

fn array(v: &Value) -> StoreResult<&Vec<Value>> {
    v.as_array().ok_or_else(|| protocol("expected array"))
}
fn integer(v: &Value) -> StoreResult<i64> {
    v.as_i64()
        .ok_or_else(|| protocol("expected signed 64-bit integer"))
}
fn string(v: &Value) -> StoreResult<String> {
    v.as_str()
        .map(str::to_string)
        .ok_or_else(|| protocol("expected string"))
}
fn optional_integer(v: &Value) -> StoreResult<Option<i64>> {
    if v.is_null() {
        Ok(None)
    } else {
        integer(v).map(Some)
    }
}
fn scalar(rows: &[Vec<Value>]) -> StoreResult<i64> {
    if rows.len() != 1 || rows[0].len() != 1 {
        return Err(protocol(
            "expected one result; namespace/entity may not exist (call initialize first)",
        ));
    }
    integer(&rows[0][0])
}
fn count(rows: &[Vec<Value>]) -> StoreResult<usize> {
    usize::try_from(scalar(rows)?).map_err(|_| protocol("invalid count"))
}
fn parse_entity(value: &Value) -> StoreResult<Entity> {
    let r = array(value)?;
    if r.len() != 6 {
        return Err(protocol("invalid entity row"));
    }
    Ok(Entity {
        id: integer(&r[0])?,
        name: string(&r[1])?,
        entity_type: if r[2].is_null() {
            None
        } else {
            Some(string(&r[2])?)
        },
        mention_count: integer(&r[3])?,
        first_seen: integer(&r[4])?,
        last_seen: integer(&r[5])?,
    })
}
fn parse_edge(value: &Value) -> StoreResult<Edge> {
    let r = array(value)?;
    if r.len() != 8 {
        return Err(protocol("invalid edge row"));
    }
    let confidence = r[5]
        .as_f64()
        .filter(|c| c.is_finite() && (0.0..=1.0).contains(c))
        .ok_or_else(|| protocol("invalid confidence"))? as f32;
    Ok(Edge {
        id: integer(&r[0])?,
        subject: string(&r[1])?,
        predicate: string(&r[2])?,
        object: string(&r[3])?,
        source_memory_id: optional_integer(&r[4])?,
        confidence,
        created_at: integer(&r[6])?,
        invalidated_at: optional_integer(&r[7])?,
    })
}

impl GraphStore for Neo4jGraphStore {
    fn add_triple(&self, triple: &Triple, source: Option<i64>, now: i64) -> StoreResult<i64> {
        self.write_triple(triple, source, now, false)
    }
    fn replace_triple(&self, triple: &Triple, source: Option<i64>, now: i64) -> StoreResult<i64> {
        self.write_triple(triple, source, now, true)
    }

    fn entities(&self) -> StoreResult<Vec<Entity>> {
        let rows = self.execute(&format!("MATCH (n:AgentMemoryEntity {{namespace: $namespace}}) RETURN {ENTITY_ROW} ORDER BY n.id"), json!({}))?;
        rows.iter()
            .map(|r| {
                r.first()
                    .ok_or_else(|| protocol("missing entity"))
                    .and_then(parse_entity)
            })
            .collect()
    }

    fn edges(&self, include_invalid: bool) -> StoreResult<Vec<Edge>> {
        let rows = self.execute(&format!("MATCH (s:AgentMemoryEntity {{namespace: $namespace}})-[r:AGENT_MEMORY_RELATION]->(o:AgentMemoryEntity {{namespace: $namespace}})
            WHERE $include_invalid OR r.invalidated_at IS NULL RETURN {EDGE_ROW} ORDER BY r.created_at, r.id"), json!({"include_invalid": include_invalid}))?;
        rows.iter()
            .map(|r| {
                r.first()
                    .ok_or_else(|| protocol("missing edge"))
                    .and_then(parse_edge)
            })
            .collect()
    }

    fn invalidate(&self, subject: &str, predicate: &str, now: i64) -> StoreResult<usize> {
        count(&self.execute(&format!("{LOCK}
            MATCH (s:AgentMemoryEntity {{namespace: $namespace, name: $subject}})-[r:AGENT_MEMORY_RELATION {{predicate: $predicate}}]->(o:AgentMemoryEntity {{namespace: $namespace}})
            WHERE r.invalidated_at IS NULL SET r.invalidated_at = $now RETURN count(r)"), json!({"subject": subject.trim(), "predicate": predicate.trim(), "now": now}))?)
    }

    fn merge_entities(&self, keep: &str, alias: &str) -> StoreResult<usize> {
        if keep.trim().is_empty() || alias.trim().is_empty() || keep.trim() == alias.trim() {
            return Err(StoreError::Other(
                "keep and alias must be nonempty, different entities".into(),
            ));
        }
        count(&self.execute(
            &format!("{LOCK}{MERGE_ENTITIES}"),
            json!({"keep": keep.trim(), "alias": alias.trim()}),
        )?)
    }

    fn neighbors(&self, entity: &str, depth: usize) -> StoreResult<Vec<Edge>> {
        if depth == 0 {
            return Ok(vec![]);
        }
        let edges = self.edges(false)?;
        let mut adj: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &edges {
            adj.entry(&e.subject).or_default().push(e);
            adj.entry(&e.object).or_default().push(e);
        }
        let mut nodes = HashSet::from([entity.trim()]);
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([(entity.trim(), 0)]);
        let mut out = Vec::new();
        while let Some((node, d)) = queue.pop_front() {
            if d >= depth {
                continue;
            }
            for e in adj.get(node).into_iter().flatten() {
                if seen.insert(e.id) {
                    out.push((*e).clone());
                }
                let next = if e.subject == node {
                    e.object.as_str()
                } else {
                    e.subject.as_str()
                };
                if nodes.insert(next) {
                    queue.push_back((next, d + 1));
                }
            }
        }
        Ok(out)
    }

    fn find_paths(&self, from: &str, to: &str, max_depth: usize) -> StoreResult<Vec<Vec<Edge>>> {
        if max_depth == 0 || from.trim() == to.trim() {
            return Ok(vec![]);
        }
        let edges = self.edges(false)?;
        let mut adj: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &edges {
            adj.entry(&e.subject).or_default().push(e);
        }
        // 显式栈避免调用方传入极大深度时耗尽调用栈。
        let mut stack = vec![(
            from.trim(),
            Vec::<Edge>::new(),
            HashSet::from([from.trim()]),
        )];
        let mut results = Vec::new();
        while let Some((node, path, visited)) = stack.pop() {
            if node == to.trim() {
                results.push(path);
                continue;
            }
            if path.len() >= max_depth {
                continue;
            }
            for e in adj.get(node).into_iter().flatten().rev() {
                if visited.contains(e.object.as_str()) {
                    continue;
                }
                let mut next_path = path.clone();
                next_path.push((*e).clone());
                let mut next_visited = visited.clone();
                next_visited.insert(&e.object);
                stack.push((&e.object, next_path, next_visited));
            }
        }
        Ok(results)
    }

    fn communities(&self) -> StoreResult<Vec<Vec<String>>> {
        let (entities, edges) = self.snapshot()?;
        Ok(components(&entities, &edges))
    }

    fn graph_stats(&self) -> StoreResult<GraphStats> {
        let (entities, edges) = self.snapshot()?;
        let groups = components(&entities, &edges);
        let valid = edges.iter().filter(|e| e.is_valid()).count();
        Ok(GraphStats {
            entities: entities.len(),
            valid_edges: valid,
            invalid_edges: edges.len() - valid,
            communities: groups.len(),
            largest_community: groups.first().map_or(0, Vec::len),
        })
    }
}

fn components(entities: &[Entity], edges: &[Edge]) -> Vec<Vec<String>> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in edges.iter().filter(|e| e.is_valid()) {
        adj.entry(&e.subject).or_default().push(&e.object);
        adj.entry(&e.object).or_default().push(&e.subject);
    }
    let mut visited = HashSet::new();
    let mut groups = Vec::new();
    for e in entities {
        if !visited.insert(e.name.as_str()) {
            continue;
        }
        let mut group = Vec::new();
        let mut stack = vec![e.name.as_str()];
        while let Some(node) = stack.pop() {
            group.push(node.to_string());
            for next in adj.get(node).into_iter().flatten() {
                if visited.insert(next) {
                    stack.push(next);
                }
            }
        }
        group.sort();
        groups.push(group);
    }
    groups.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    groups
}

#[cfg(test)]
#[path = "neo4j/tests.rs"]
mod tests;
