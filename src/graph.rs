//! 知识图谱记忆层：实体 + 关系边，SQLite 存储，支持溯源与时间失效。
//!
//! 设计对齐 Zep/Graphiti 的图记忆思想，但**不依赖外部图数据库**：用两张关系表
//! （`graph_entities` / `graph_edges`）表达有向三元组 `subject -predicate-> object`，
//! 并在 Rust 侧做邻居扩展与多跳路径搜索。
//!
//! - **时间冲突解决**：同一主体的同一关系指向新客体时，旧边标记 `invalidated_at`，
//!   不物理删除（可追溯），与记忆层的 `superseded` 思路一致；
//! - **溯源**：每条边可记录来源记忆 `source_memory_id`，回溯到原始文本；
//! - **可插拔抽取**：三元组既可由 [`crate::extract::Extractor`] 自动抽取，也可手动写入。

use std::collections::{HashMap, HashSet, VecDeque};

use rusqlite::params;

use crate::db::Db;
use crate::store::StoreResult;

/// 一个待写入的三元组 `(subject, predicate, object)`。
#[derive(Debug, Clone, PartialEq)]
pub struct Triple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub confidence: f32,
}

impl Triple {
    pub fn new(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
            confidence: 1.0,
        }
    }

    /// 设置置信度 [0,1]。
    pub fn with_confidence(mut self, c: f32) -> Self {
        self.confidence = c.clamp(0.0, 1.0);
        self
    }
}

/// 实体节点。
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: i64,
    pub name: String,
    pub entity_type: Option<String>,
    pub mention_count: i64,
    pub first_seen: i64,
    pub last_seen: i64,
}

/// 关系边（已展开实体名，便于直接展示）。
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub id: i64,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub source_memory_id: Option<i64>,
    pub confidence: f32,
    pub created_at: i64,
    pub invalidated_at: Option<i64>,
}

impl Edge {
    /// 格式化为 `主体 -关系-> 客体`。
    pub fn display(&self) -> String {
        format!("{} -{}-> {}", self.subject, self.predicate, self.object)
    }

    pub fn is_valid(&self) -> bool {
        self.invalidated_at.is_none()
    }
}

/// 图存储抽象（可替换为 Neo4j 等真实图数据库后端）。
pub trait GraphStore: Send + Sync {
    /// 新增一条三元组（多值关系，如「喜欢」「使用」可同时指向多个客体）。
    ///
    /// 同 `(s,p,o)` 已存在则幂等返回原 id；**不会**失效其他客体的边。
    fn add_triple(
        &self,
        triple: &Triple,
        source_memory_id: Option<i64>,
        now: i64,
    ) -> StoreResult<i64>;

    /// 用新三元组「替换」某主体的某单值关系（如「住在」「任职于」）。
    ///
    /// 同一 `(subject,predicate)` 指向其他客体的旧有效边会被标记失效（可追溯），
    /// 再写入新边。适用于事实随时间变化的场景。
    fn replace_triple(
        &self,
        triple: &Triple,
        source_memory_id: Option<i64>,
        now: i64,
    ) -> StoreResult<i64>;

    /// 实体的无向邻居（扩展 `depth` 跳内涉及的全部有效边）。
    fn neighbors(&self, entity: &str, depth: usize) -> StoreResult<Vec<Edge>>;

    /// 从 `from` 到 `to` 的全部不超过 `max_depth` 跳的有向简单路径。
    fn find_paths(&self, from: &str, to: &str, max_depth: usize) -> StoreResult<Vec<Vec<Edge>>>;

    /// 列出全部实体。
    fn entities(&self) -> StoreResult<Vec<Entity>>;

    /// 列出边；`include_invalid` 为 true 时包含已失效边。
    fn edges(&self, include_invalid: bool) -> StoreResult<Vec<Edge>>;

    /// 主动失效某主体的某关系，返回失效条数。
    fn invalidate(&self, subject: &str, predicate: &str, now: i64) -> StoreResult<usize>;
}

/// 基于共享 SQLite 连接的图存储实现。
pub struct SqliteGraphStore {
    db: Db,
}

impl SqliteGraphStore {
    /// 幂等插入一条边（不失效其他边）。
    fn insert_triple(
        &self,
        triple: &Triple,
        source_memory_id: Option<i64>,
        now: i64,
    ) -> StoreResult<i64> {
        let sid = self.upsert_entity(&triple.subject, now)?;
        let oid = self.upsert_entity(&triple.object, now)?;
        let conn = self.db.lock();
        conn.execute(
            "INSERT OR IGNORE INTO graph_edges \
             (subject_id, predicate, object_id, source_memory_id, confidence, created_at, invalidated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![sid, triple.predicate, oid, source_memory_id, triple.confidence, now],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM graph_edges WHERE subject_id = ?1 AND predicate = ?2 AND object_id = ?3",
            params![sid, triple.predicate, oid],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    pub fn from_db(db: Db) -> Self {
        Self { db }
    }

    /// upsert 实体并返回其 id（重复出现时 mention_count + 1）。
    fn upsert_entity(&self, name: &str, now: i64) -> StoreResult<i64> {
        let name = name.trim();
        let conn = self.db.lock();
        conn.execute(
            "INSERT INTO graph_entities(name, entity_type, first_seen, last_seen) \
             VALUES (?1, NULL, ?2, ?2) \
             ON CONFLICT(name) DO UPDATE SET mention_count = mention_count + 1, last_seen = excluded.last_seen",
            params![name, now],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM graph_entities WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    /// 读取全部边（可选是否含已失效），并展开实体名。
    fn load_edges(&self, include_invalid: bool) -> StoreResult<Vec<Edge>> {
        let conn = self.db.lock();
        let sql = "SELECT e.id, s.name, e.predicate, o.name, e.source_memory_id, \
                          e.confidence, e.created_at, e.invalidated_at \
                   FROM graph_edges e \
                   JOIN graph_entities s ON s.id = e.subject_id \
                   JOIN graph_entities o ON o.id = e.object_id \
                   {filter} \
                   ORDER BY e.created_at ASC, e.id ASC";
        let sql = sql.replace(
            "{filter}",
            if include_invalid {
                ""
            } else {
                "WHERE e.invalidated_at IS NULL"
            },
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_edge)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

fn row_to_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<Edge> {
    Ok(Edge {
        id: row.get(0)?,
        subject: row.get(1)?,
        predicate: row.get(2)?,
        object: row.get(3)?,
        source_memory_id: row.get(4)?,
        confidence: row.get(5)?,
        created_at: row.get(6)?,
        invalidated_at: row.get(7)?,
    })
}

impl GraphStore for SqliteGraphStore {
    fn add_triple(
        &self,
        triple: &Triple,
        source_memory_id: Option<i64>,
        now: i64,
    ) -> StoreResult<i64> {
        self.insert_triple(triple, source_memory_id, now)
    }

    fn replace_triple(
        &self,
        triple: &Triple,
        source_memory_id: Option<i64>,
        now: i64,
    ) -> StoreResult<i64> {
        let sid = self.upsert_entity(&triple.subject, now)?;
        let oid = self.upsert_entity(&triple.object, now)?;
        // 时间冲突：同一主体同一单值关系、客体不同的旧有效边失效。
        {
            let conn = self.db.lock();
            conn.execute(
                "UPDATE graph_edges SET invalidated_at = ?1 \
                 WHERE subject_id = ?2 AND predicate = ?3 AND object_id <> ?4 AND invalidated_at IS NULL",
                params![now, sid, triple.predicate, oid],
            )?;
        }
        self.insert_triple(triple, source_memory_id, now)
    }

    fn neighbors(&self, entity: &str, depth: usize) -> StoreResult<Vec<Edge>> {
        let start = entity.trim();
        let edges = self.load_edges(false)?;
        if depth == 0 {
            return Ok(vec![]);
        }
        // 无向邻接表：节点名 -> 相连边
        let mut adj: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &edges {
            adj.entry(e.subject.as_str()).or_default().push(e);
            adj.entry(e.object.as_str()).or_default().push(e);
        }
        let mut visited_nodes: HashSet<String> = HashSet::new();
        visited_nodes.insert(start.to_string());
        let mut visited_edges: HashSet<i64> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        queue.push_back((start.to_string(), 0));
        let mut out = Vec::new();

        while let Some((node, d)) = queue.pop_front() {
            if d >= depth {
                continue;
            }
            if let Some(linked) = adj.get(node.as_str()) {
                for e in linked.iter().copied() {
                    if visited_edges.insert(e.id) {
                        out.push(e.clone());
                    }
                    let next = if e.subject == node {
                        e.object.clone()
                    } else {
                        e.subject.clone()
                    };
                    if visited_nodes.insert(next.clone()) {
                        queue.push_back((next, d + 1));
                    }
                }
            }
        }
        Ok(out)
    }

    fn find_paths(&self, from: &str, to: &str, max_depth: usize) -> StoreResult<Vec<Vec<Edge>>> {
        let from = from.trim();
        let to = to.trim();
        let edges = self.load_edges(false)?;
        let mut out_adj: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &edges {
            out_adj.entry(e.subject.as_str()).or_default().push(e);
        }

        let mut results = Vec::new();
        let mut path: Vec<Edge> = Vec::new();
        let mut on_path: HashSet<String> = HashSet::new();
        on_path.insert(from.to_string());

        fn dfs<'a>(
            current: &str,
            target: &str,
            depth_left: usize,
            adj: &HashMap<&'a str, Vec<&'a Edge>>,
            path: &mut Vec<Edge>,
            on_path: &mut HashSet<String>,
            results: &mut Vec<Vec<Edge>>,
        ) {
            if current == target && !path.is_empty() {
                results.push(path.clone());
                return;
            }
            if depth_left == 0 {
                return;
            }
            if let Some(nexts) = adj.get(current) {
                for e in nexts.iter().copied() {
                    if on_path.contains(&e.object) {
                        continue; // 简单路径，避免成环
                    }
                    on_path.insert(e.object.clone());
                    path.push(e.clone());
                    dfs(
                        &e.object,
                        target,
                        depth_left - 1,
                        adj,
                        path,
                        on_path,
                        results,
                    );
                    path.pop();
                    on_path.remove(&e.object);
                }
            }
        }

        dfs(
            from,
            to,
            max_depth,
            &out_adj,
            &mut path,
            &mut on_path,
            &mut results,
        );
        Ok(results)
    }

    fn entities(&self) -> StoreResult<Vec<Entity>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, entity_type, mention_count, first_seen, last_seen \
             FROM graph_entities ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Entity {
                id: r.get(0)?,
                name: r.get(1)?,
                entity_type: r.get(2)?,
                mention_count: r.get(3)?,
                first_seen: r.get(4)?,
                last_seen: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    fn edges(&self, include_invalid: bool) -> StoreResult<Vec<Edge>> {
        self.load_edges(include_invalid)
    }

    fn invalidate(&self, subject: &str, predicate: &str, now: i64) -> StoreResult<usize> {
        let conn = self.db.lock();
        let n = conn.execute(
            "UPDATE graph_edges SET invalidated_at = ?1 \
             WHERE invalidated_at IS NULL AND predicate = ?3 \
               AND subject_id = (SELECT id FROM graph_entities WHERE name = ?2)",
            params![now, subject.trim(), predicate],
        )?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn store() -> SqliteGraphStore {
        SqliteGraphStore::from_db(Db::open(":memory:").unwrap())
    }

    #[test]
    fn add_triple_and_list_entities() {
        let g = store();
        g.add_triple(&Triple::new("用户", "喜欢", "Rust"), None, 1)
            .unwrap();
        g.add_triple(&Triple::new("用户", "在", "北京"), None, 2)
            .unwrap();
        let ents = g.entities().unwrap();
        let names: Vec<&str> = ents.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"用户"));
        assert!(names.contains(&"Rust"));
        assert!(names.contains(&"北京"));
        assert_eq!(g.edges(false).unwrap().len(), 2);
    }

    #[test]
    fn idempotent_same_triple() {
        let g = store();
        let a = g
            .add_triple(&Triple::new("a", "knows", "b"), None, 1)
            .unwrap();
        let b = g
            .add_triple(&Triple::new("a", "knows", "b"), None, 2)
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(g.edges(true).unwrap().len(), 1);
    }

    #[test]
    fn temporal_invalidation_on_change() {
        let g = store();
        g.replace_triple(&Triple::new("用户", "住在", "北京"), None, 1)
            .unwrap();
        g.replace_triple(&Triple::new("用户", "住在", "上海"), None, 100)
            .unwrap();
        // 有效边只剩新的；旧边仍在库但已失效
        let valid = g.edges(false).unwrap();
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].object, "上海");
        let all = g.edges(true).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all
            .iter()
            .any(|e| e.object == "北京" && e.invalidated_at.is_some()));
    }

    #[test]
    fn add_triple_keeps_multi_valued_relations() {
        let g = store();
        g.add_triple(&Triple::new("用户", "使用", "Rust"), None, 1)
            .unwrap();
        g.add_triple(&Triple::new("用户", "使用", "Go"), None, 2)
            .unwrap();
        // 多值关系：两条都保留
        assert_eq!(g.edges(false).unwrap().len(), 2);
    }

    #[test]
    fn neighbors_undirected() {
        let g = store();
        g.add_triple(&Triple::new("用户", "喜欢", "Rust"), None, 1)
            .unwrap();
        g.add_triple(&Triple::new("Rust", "适合", "系统编程"), None, 2)
            .unwrap();
        g.add_triple(&Triple::new("无关", "是", "孤立点"), None, 3)
            .unwrap();

        let one_hop = g.neighbors("用户", 1).unwrap();
        assert_eq!(one_hop.len(), 1);
        assert_eq!(one_hop[0].object, "Rust");

        let two_hop = g.neighbors("用户", 2).unwrap();
        assert_eq!(two_hop.len(), 2);
    }

    #[test]
    fn multi_hop_paths() {
        let g = store();
        // 用户 -> Rust -> 系统编程 ; 用户 -> Go -> 系统编程
        g.add_triple(&Triple::new("用户", "使用", "Rust"), None, 1)
            .unwrap();
        g.add_triple(&Triple::new("Rust", "属于", "系统编程"), None, 2)
            .unwrap();
        g.add_triple(&Triple::new("用户", "使用", "Go"), None, 3)
            .unwrap();
        g.add_triple(&Triple::new("Go", "属于", "系统编程"), None, 4)
            .unwrap();

        let paths = g.find_paths("用户", "系统编程", 2).unwrap();
        assert_eq!(paths.len(), 2); // 两条两跳路径
        assert!(paths.iter().all(|p| p.len() == 2));

        // 一跳内不可达
        assert!(g.find_paths("用户", "系统编程", 1).unwrap().is_empty());
    }

    #[test]
    fn manual_invalidate() {
        let g = store();
        g.add_triple(&Triple::new("用户", "任职", "A公司"), None, 1)
            .unwrap();
        let n = g.invalidate("用户", "任职", 50).unwrap();
        assert_eq!(n, 1);
        assert!(g.edges(false).unwrap().is_empty());
    }
}
