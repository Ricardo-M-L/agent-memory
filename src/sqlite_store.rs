//! SQLite 持久化实现（默认记忆存储后端）。
//!
//! - 使用 rusqlite 的 bundled 特性，无需单独安装 SQLite；
//! - 嵌入向量以 f32 小端 BLOB 存储，紧凑且可直接读回；
//! - 共享 [`crate::db::Db`] 连接（与知识图谱表同库），内部加锁保证线程安全；
//! - 打开 `:memory:` 即得到纯内存库，便于测试。

use std::str::FromStr;

use rusqlite::{params, OptionalExtension};

use crate::db::Db;
use crate::store::{MemoryStore, StoreResult};
use crate::types::{MemoryItem, MemoryType, NewMemory, Scope};

/// SQLite 记忆后端。
pub struct SqliteStore {
    db: Db,
}

impl SqliteStore {
    /// 打开（或创建）数据库文件。`path` 可为 `:memory:`。
    pub fn open(path: &str) -> StoreResult<Self> {
        Ok(Self {
            db: Db::open(path)?,
        })
    }

    /// 复用一个已打开的共享连接（与图谱存储同库）。
    pub fn from_db(db: Db) -> Self {
        Self { db }
    }
}

fn f32s_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

fn bytes_to_f32s(b: &[u8]) -> Option<Vec<f32>> {
    if !b.len().is_multiple_of(4) {
        return None;
    }
    let (chunks, _rem) = b.as_chunks::<4>();
    Some(chunks.iter().map(|c| f32::from_le_bytes(*c)).collect())
}

fn scope_str(s: Scope) -> &'static str {
    s.as_str()
}

fn type_str(t: MemoryType) -> &'static str {
    t.as_str()
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryItem> {
    let scope_s: String = row.get("scope")?;
    let type_s: String = row.get("memory_type")?;
    let emb_blob: Option<Vec<u8>> = row.get("embedding")?;
    Ok(MemoryItem {
        id: row.get("id")?,
        scope: Scope::from_str(&scope_s).unwrap_or(Scope::Agent),
        scope_key: row.get("scope_key")?,
        memory_type: MemoryType::from_str(&type_s).unwrap_or(MemoryType::Semantic),
        content: row.get("content")?,
        importance: row.get("importance")?,
        created_at: row.get("created_at")?,
        last_access_at: row.get("last_access_at")?,
        access_count: row.get("access_count")?,
        superseded_by: row.get("superseded_by")?,
        ttl_secs: row.get("ttl_secs")?,
        embedding: emb_blob.and_then(|b| bytes_to_f32s(&b)),
        meta: row.get("meta")?,
    })
}

const SELECT_ALL: &str = "SELECT id, scope, scope_key, memory_type, content, importance, \
     created_at, last_access_at, access_count, superseded_by, ttl_secs, embedding, meta \
     FROM memories";

impl MemoryStore for SqliteStore {
    fn insert(&self, item: &NewMemory, created_at: i64) -> StoreResult<i64> {
        let conn = self.db.lock();
        conn.execute(
            "INSERT INTO memories \
             (scope, scope_key, memory_type, content, importance, created_at, last_access_at, access_count, ttl_secs, meta) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 0, ?7, ?8)",
            params![
                scope_str(item.scope),
                item.scope_key,
                type_str(item.memory_type),
                item.content,
                item.importance,
                created_at,
                item.ttl_secs,
                item.meta,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    fn get(&self, id: i64) -> StoreResult<Option<MemoryItem>> {
        let conn = self.db.lock();
        let row = conn
            .query_row(
                &format!("{SELECT_ALL} WHERE id = ?1"),
                params![id],
                row_to_item,
            )
            .optional()?;
        Ok(row)
    }

    fn list(
        &self,
        scope: Scope,
        scope_key: &str,
        memory_type: Option<MemoryType>,
    ) -> StoreResult<Vec<MemoryItem>> {
        let conn = self.db.lock();
        let mut out = Vec::new();
        match memory_type {
            Some(t) => {
                let mut stmt = conn.prepare(&format!(
                    "{SELECT_ALL} WHERE scope = ?1 AND scope_key = ?2 AND memory_type = ?3 \
                     ORDER BY created_at ASC, id ASC"
                ))?;
                let rows = stmt.query_map(
                    params![scope_str(scope), scope_key, type_str(t)],
                    row_to_item,
                )?;
                for r in rows {
                    out.push(r?);
                }
            }
            None => {
                let mut stmt = conn.prepare(&format!(
                    "{SELECT_ALL} WHERE scope = ?1 AND scope_key = ?2 \
                     ORDER BY created_at ASC, id ASC"
                ))?;
                let rows = stmt.query_map(params![scope_str(scope), scope_key], row_to_item)?;
                for r in rows {
                    out.push(r?);
                }
            }
        }
        Ok(out)
    }

    fn mark_superseded(&self, id: i64, by_id: i64) -> StoreResult<()> {
        let conn = self.db.lock();
        conn.execute(
            "UPDATE memories SET superseded_by = ?1 WHERE id = ?2",
            params![by_id, id],
        )?;
        Ok(())
    }

    fn delete(&self, id: i64) -> StoreResult<()> {
        let conn = self.db.lock();
        conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn touch(&self, id: i64, now: i64) -> StoreResult<()> {
        let conn = self.db.lock();
        conn.execute(
            "UPDATE memories SET last_access_at = ?1, access_count = access_count + 1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }

    fn set_embedding(&self, id: i64, vec: Vec<f32>) -> StoreResult<()> {
        let conn = self.db.lock();
        conn.execute(
            "UPDATE memories SET embedding = ?1 WHERE id = ?2",
            params![f32s_to_bytes(&vec), id],
        )?;
        Ok(())
    }

    fn prune_expired(&self, now: i64) -> StoreResult<usize> {
        let conn = self.db.lock();
        let deleted = conn.execute(
            "DELETE FROM memories WHERE ttl_secs IS NOT NULL AND (created_at + ttl_secs * 1000) < ?1",
            params![now],
        )?;
        Ok(deleted)
    }

    fn count(
        &self,
        scope: Scope,
        scope_key: &str,
        memory_type: Option<MemoryType>,
    ) -> StoreResult<usize> {
        let conn = self.db.lock();
        let n: i64 = match memory_type {
            Some(t) => conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE scope = ?1 AND scope_key = ?2 AND memory_type = ?3",
                params![scope_str(scope), scope_key, type_str(t)],
                |r| r.get(0),
            )?,
            None => conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE scope = ?1 AND scope_key = ?2",
                params![scope_str(scope), scope_key],
                |r| r.get(0),
            )?,
        };
        Ok(n as usize)
    }

    fn oldest_working(&self, scope: Scope, scope_key: &str) -> StoreResult<Option<MemoryItem>> {
        let conn = self.db.lock();
        let row = conn
            .query_row(
                &format!("{SELECT_ALL} WHERE scope = ?1 AND scope_key = ?2 AND memory_type = 'working' AND superseded_by IS NULL ORDER BY created_at ASC, id ASC LIMIT 1"),
                params![scope_str(scope), scope_key],
                row_to_item,
            )
            .optional()?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get_roundtrip() {
        let store = SqliteStore::open(":memory:").unwrap();
        let item = NewMemory {
            scope: Scope::User,
            scope_key: "u1".into(),
            memory_type: MemoryType::Semantic,
            content: "用户喜欢 Rust".into(),
            importance: 0.8,
            ttl_secs: None,
            meta: None,
        };
        let id = store.insert(&item, 1000).unwrap();
        let got = store.get(id).unwrap().unwrap();
        assert_eq!(got.id, id);
        assert_eq!(got.content, "用户喜欢 Rust");
        assert_eq!(got.scope, Scope::User);
        assert!((got.importance - 0.8).abs() < 1e-6);
        assert_eq!(got.created_at, 1000);
        assert_eq!(got.memory_type, MemoryType::Semantic);
    }

    #[test]
    fn list_filters_by_scope_and_type() {
        let store = SqliteStore::open(":memory:").unwrap();
        let mk = |scope: Scope, key: &str, ty: MemoryType, content: &str| NewMemory {
            scope,
            scope_key: key.into(),
            memory_type: ty,
            content: content.into(),
            importance: 0.5,
            ttl_secs: None,
            meta: None,
        };
        store
            .insert(&mk(Scope::User, "u1", MemoryType::Semantic, "a"), 1)
            .unwrap();
        store
            .insert(&mk(Scope::User, "u1", MemoryType::Episodic, "b"), 2)
            .unwrap();
        store
            .insert(&mk(Scope::User, "u2", MemoryType::Semantic, "c"), 3)
            .unwrap();
        store
            .insert(&mk(Scope::Agent, "ag", MemoryType::Semantic, "d"), 4)
            .unwrap();

        assert_eq!(store.list(Scope::User, "u1", None).unwrap().len(), 2);
        assert_eq!(
            store
                .list(Scope::User, "u1", Some(MemoryType::Semantic))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(store.list(Scope::User, "u2", None).unwrap().len(), 1);
        assert_eq!(store.list(Scope::Agent, "ag", None).unwrap().len(), 1);
    }

    #[test]
    fn supersede_and_prune() {
        let store = SqliteStore::open(":memory:").unwrap();

        // 无 TTL 的记忆用于 supersede 测试
        let item_no_ttl = NewMemory {
            scope: Scope::User,
            scope_key: "u".into(),
            memory_type: MemoryType::Semantic,
            content: "x".into(),
            importance: 0.5,
            ttl_secs: None,
            meta: None,
        };
        let id = store.insert(&item_no_ttl, 1_000).unwrap();
        store.mark_superseded(id, 99).unwrap();
        assert!(store.get(id).unwrap().unwrap().is_superseded());

        // 有 TTL 的记忆用于过期清理测试
        let item_ttl = NewMemory {
            scope: Scope::User,
            scope_key: "u".into(),
            memory_type: MemoryType::Semantic,
            content: "y".into(),
            importance: 0.5,
            ttl_secs: Some(10),
            meta: None,
        };
        let id2 = store.insert(&item_ttl, 1_000).unwrap();
        assert_eq!(store.prune_expired(1_000 + 11_000).unwrap(), 1);
        assert!(store.get(id2).unwrap().is_none());
        // 无 TTL 的记忆不受影响
        assert!(store.get(id).unwrap().is_some());
    }

    #[test]
    fn embedding_blob_roundtrip() {
        let store = SqliteStore::open(":memory:").unwrap();
        let item = NewMemory {
            scope: Scope::Agent,
            scope_key: "a".into(),
            memory_type: MemoryType::Semantic,
            content: "v".into(),
            importance: 0.5,
            ttl_secs: None,
            meta: None,
        };
        let id = store.insert(&item, 1).unwrap();
        let v = vec![0.1, 0.2, 0.3, -0.4];
        store.set_embedding(id, v.clone()).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().embedding, Some(v));
    }

    #[test]
    fn touch_updates_access() {
        let store = SqliteStore::open(":memory:").unwrap();
        let item = NewMemory {
            scope: Scope::User,
            scope_key: "u".into(),
            memory_type: MemoryType::Working,
            content: "w".into(),
            importance: 0.5,
            ttl_secs: None,
            meta: None,
        };
        let id = store.insert(&item, 1).unwrap();
        store.touch(id, 2).unwrap();
        store.touch(id, 3).unwrap();
        let got = store.get(id).unwrap().unwrap();
        assert_eq!(got.access_count, 2);
        assert_eq!(got.last_access_at, 3);
    }

    #[test]
    fn oldest_working_returns_oldest() {
        let store = SqliteStore::open(":memory:").unwrap();
        let mk = |content: &str, _t: i64| NewMemory {
            scope: Scope::User,
            scope_key: "u".into(),
            memory_type: MemoryType::Working,
            content: content.into(),
            importance: 0.5,
            ttl_secs: None,
            meta: None,
        };
        store.insert(&mk("first", 1), 1).unwrap();
        store.insert(&mk("second", 2), 2).unwrap();
        let oldest = store.oldest_working(Scope::User, "u").unwrap().unwrap();
        assert_eq!(oldest.content, "first");
    }
}
