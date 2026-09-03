//! 存储抽象：让记忆库可以替换后端（内存 / SQLite / 远端服务）。
//!
//! 默认实现是 [`crate::sqlite_store::SqliteStore`]。要实现自己的后端，只需实现本
//! trait 并调用 [`crate::AgentMemory::with_store`]。

use std::fmt;

use crate::types::{MemoryItem, MemoryType, NewMemory, Scope};

/// 存储操作结果。
pub type StoreResult<T> = Result<T, StoreError>;

/// 存储层错误。
#[derive(Debug)]
pub enum StoreError {
    /// 底层数据库错误。
    Db(String),
    /// 网络 / HTTP 调用错误（仅 `http` feature 下的远端后端会产生）。
    Http(String),
    /// 其他错误。
    Other(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Db(e) => write!(f, "database error: {e}"),
            StoreError::Http(e) => write!(f, "http error: {e}"),
            StoreError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Db(e.to_string())
    }
}

/// 记忆存储后端抽象。
///
/// 所有方法要求线程安全（内部实现负责同步），以便在 agent 中通过 `Arc` 共享。
pub trait MemoryStore: Send + Sync {
    /// 插入一条新记忆，返回其 id。
    fn insert(&self, item: &NewMemory, created_at: i64) -> StoreResult<i64>;
    /// 按 id 读取。
    fn get(&self, id: i64) -> StoreResult<Option<MemoryItem>>;
    /// 列出某作用域的记忆（可选按类型过滤）。
    ///
    /// 注意：本方法返回的是原始记录（含已取代、已过期），过滤逻辑由上层负责。
    fn list(
        &self,
        scope: Scope,
        scope_key: &str,
        memory_type: Option<MemoryType>,
    ) -> StoreResult<Vec<MemoryItem>>;
    /// 冲突解决：把 `id` 标记为被 `by_id` 取代（旧事实不静默删除）。
    fn mark_superseded(&self, id: i64, by_id: i64) -> StoreResult<()>;
    /// 删除一条记忆。
    fn delete(&self, id: i64) -> StoreResult<()>;
    /// 记录一次访问（更新 last_access_at 与 access_count）。
    fn touch(&self, id: i64, now: i64) -> StoreResult<()>;
    /// 写入/更新一条记忆的嵌入向量。
    fn set_embedding(&self, id: i64, vec: Vec<f32>) -> StoreResult<()>;
    /// 清理所有已过期记忆，返回清理条数。
    fn prune_expired(&self, now: i64) -> StoreResult<usize>;
    /// 统计某作用域记忆条数（可选按类型过滤）。
    fn count(
        &self,
        scope: Scope,
        scope_key: &str,
        memory_type: Option<MemoryType>,
    ) -> StoreResult<usize>;
    /// 取某作用域下最旧的一条 working 记忆（用于滚动淘汰）。
    fn oldest_working(&self, scope: Scope, scope_key: &str) -> StoreResult<Option<MemoryItem>>;
}

#[cfg(test)]
pub(crate) mod testutil {
    //! 测试用的内存后端（同时作为自定义存储的参考实现）。

    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::types::{MemoryItem, MemoryType, NewMemory, Scope};

    use super::{MemoryStore, StoreResult};

    pub struct MemStore {
        items: Mutex<HashMap<i64, MemoryItem>>,
        next_id: Mutex<i64>,
    }

    impl MemStore {
        pub fn new() -> Self {
            Self {
                items: Mutex::new(HashMap::new()),
                next_id: Mutex::new(1),
            }
        }
    }

    impl Default for MemStore {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MemoryStore for MemStore {
        fn insert(&self, item: &NewMemory, created_at: i64) -> StoreResult<i64> {
            let mut map = self.items.lock().unwrap();
            let mut nid = self.next_id.lock().unwrap();
            let id = *nid;
            *nid += 1;
            map.insert(
                id,
                MemoryItem {
                    id,
                    scope: item.scope,
                    scope_key: item.scope_key.clone(),
                    memory_type: item.memory_type,
                    content: item.content.clone(),
                    importance: item.importance,
                    created_at,
                    last_access_at: created_at,
                    access_count: 0,
                    superseded_by: None,
                    ttl_secs: item.ttl_secs,
                    embedding: None,
                    meta: item.meta.clone(),
                },
            );
            Ok(id)
        }

        fn get(&self, id: i64) -> StoreResult<Option<MemoryItem>> {
            Ok(self.items.lock().unwrap().get(&id).cloned())
        }

        fn list(
            &self,
            scope: Scope,
            scope_key: &str,
            memory_type: Option<MemoryType>,
        ) -> StoreResult<Vec<MemoryItem>> {
            let map = self.items.lock().unwrap();
            let mut v: Vec<MemoryItem> = map
                .values()
                .filter(|m| m.scope == scope && m.scope_key == scope_key)
                .filter(|m| memory_type.is_none_or(|t| m.memory_type == t))
                .cloned()
                .collect();
            v.sort_by_key(|m| m.created_at);
            Ok(v)
        }

        fn mark_superseded(&self, id: i64, by_id: i64) -> StoreResult<()> {
            if let Some(m) = self.items.lock().unwrap().get_mut(&id) {
                m.superseded_by = Some(by_id);
            }
            Ok(())
        }

        fn delete(&self, id: i64) -> StoreResult<()> {
            self.items.lock().unwrap().remove(&id);
            Ok(())
        }

        fn touch(&self, id: i64, now: i64) -> StoreResult<()> {
            if let Some(m) = self.items.lock().unwrap().get_mut(&id) {
                m.last_access_at = now;
                m.access_count += 1;
            }
            Ok(())
        }

        fn set_embedding(&self, id: i64, vec: Vec<f32>) -> StoreResult<()> {
            if let Some(m) = self.items.lock().unwrap().get_mut(&id) {
                m.embedding = Some(vec);
            }
            Ok(())
        }

        fn prune_expired(&self, now: i64) -> StoreResult<usize> {
            let mut map = self.items.lock().unwrap();
            let before = map.len();
            map.retain(|_, m| !m.is_expired(now));
            Ok(before - map.len())
        }

        fn count(
            &self,
            scope: Scope,
            scope_key: &str,
            memory_type: Option<MemoryType>,
        ) -> StoreResult<usize> {
            let map = self.items.lock().unwrap();
            Ok(map
                .values()
                .filter(|m| m.scope == scope && m.scope_key == scope_key)
                .filter(|m| memory_type.is_none_or(|t| m.memory_type == t))
                .count())
        }

        fn oldest_working(&self, scope: Scope, scope_key: &str) -> StoreResult<Option<MemoryItem>> {
            let map = self.items.lock().unwrap();
            Ok(map
                .values()
                .filter(|m| {
                    m.scope == scope
                        && m.scope_key == scope_key
                        && m.memory_type == MemoryType::Working
                        && m.superseded_by.is_none()
                })
                .min_by_key(|m| (m.created_at, m.id))
                .cloned())
        }
    }
}
