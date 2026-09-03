//! 共享 SQLite 连接。
//!
//! 记忆表（`memories`）与知识图谱表（`graph_entities` / `graph_edges`）建在同一个
//! 数据库文件、同一条连接上，便于通过 `source_memory_id` 把图谱边溯源回原始记忆。
//! [`Db`] 内部是 `Arc<Mutex<Connection>>`，可被记忆存储与图谱存储同时克隆共享。

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::store::StoreResult;

/// 可克隆的共享数据库句柄。
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// 打开（或创建）数据库并初始化全部表结构。`path` 可为 `:memory:`。
    pub fn open(path: &str) -> StoreResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 获取连接锁（与 `Mutex::lock` 等价，内部不允许中毒传播为 panic 以外的错误）。
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("database mutex poisoned")
    }
}

/// 全部表结构：记忆 + 知识图谱。
const SCHEMA: &str = "
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

-- 记忆表 ----------------------------------------------------------------
CREATE TABLE IF NOT EXISTS memories (
   id             INTEGER PRIMARY KEY AUTOINCREMENT,
   scope          TEXT    NOT NULL,
   scope_key      TEXT    NOT NULL,
   memory_type    TEXT    NOT NULL,
   content        TEXT    NOT NULL,
   importance     REAL    NOT NULL DEFAULT 0.5,
   created_at     INTEGER NOT NULL,
   last_access_at INTEGER NOT NULL,
   access_count   INTEGER NOT NULL DEFAULT 0,
   superseded_by  INTEGER,
   ttl_secs       INTEGER,
   embedding      BLOB,
   meta           TEXT
);
CREATE INDEX IF NOT EXISTS idx_scope      ON memories(scope, scope_key);
CREATE INDEX IF NOT EXISTS idx_type       ON memories(memory_type);
CREATE INDEX IF NOT EXISTS idx_superseded ON memories(superseded_by);
CREATE INDEX IF NOT EXISTS idx_created    ON memories(created_at);

-- 知识图谱：实体 ---------------------------------------------------------
CREATE TABLE IF NOT EXISTS graph_entities (
   id            INTEGER PRIMARY KEY AUTOINCREMENT,
   name          TEXT    NOT NULL UNIQUE,
   entity_type   TEXT,
   mention_count INTEGER NOT NULL DEFAULT 1,
   first_seen    INTEGER NOT NULL,
   last_seen     INTEGER NOT NULL
);

-- 知识图谱：关系边（三元组 subject -predicate-> object）------------------
CREATE TABLE IF NOT EXISTS graph_edges (
   id               INTEGER PRIMARY KEY AUTOINCREMENT,
   subject_id       INTEGER NOT NULL REFERENCES graph_entities(id),
   predicate        TEXT    NOT NULL,
   object_id        INTEGER NOT NULL REFERENCES graph_entities(id),
   source_memory_id INTEGER REFERENCES memories(id),
   confidence       REAL    NOT NULL DEFAULT 1.0,
   created_at       INTEGER NOT NULL,
   invalidated_at   INTEGER,
   UNIQUE(subject_id, predicate, object_id)
);
CREATE INDEX IF NOT EXISTS idx_edge_subject ON graph_edges(subject_id);
CREATE INDEX IF NOT EXISTS idx_edge_object  ON graph_edges(object_id);
CREATE INDEX IF NOT EXISTS idx_edge_pred    ON graph_edges(predicate);
CREATE INDEX IF NOT EXISTS idx_edge_valid   ON graph_edges(invalidated_at);
";
