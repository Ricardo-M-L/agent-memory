//! 记忆库门面：编排存储、嵌入、检索、遗忘与整合。
//!
//! 设计对齐业界主流 AI 记忆系统的成熟思路：
//! - **记忆分类**：工作 / 情景 / 语义三层（CoALA / Mem0）；
//! - **作用域**：user / session / agent 三级隔离（Mem0）；
//! - **检索打分**：`时效 × 相关 × 重要`（Generative Agents）；
//! - **相关度**：BM25 关键词 + 可选向量余弦混合；
//! - **冲突解决**：新事实写入时旧条目标记 superseded，不静默覆盖（Zep/Graphiti 时序图谱）；
//! - **遗忘**：TTL 过期 + 主动清理（Letta 归档层级）；
//! - **整合**：语义去重合并（LangMem 蒸馏层级）。

use std::sync::Arc;

use crate::db::Db;
use crate::embed::{self, Embedder, HashEmbedder};
use crate::extract::Extractor;
use crate::graph::{Edge, GraphStore, SqliteGraphStore, Triple};
use crate::retrieval::{self, RetrievalConfig, ScoredMemory};
use crate::sqlite_store::SqliteStore;
use crate::store::{MemoryStore, StoreError, StoreResult};
use crate::text;
use crate::types::{MemoryItem, MemoryType, NewMemory, Scope};

/// 单个作用域下的记忆统计。
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    pub working: usize,
    pub episodic: usize,
    pub semantic: usize,
    /// 已被取代（superseded）的历史记忆条数。
    pub superseded: usize,
    /// 全部记录条数（含已取代）。
    pub total: usize,
}

/// 记忆创建时间范围（epoch 毫秒，闭区间）。`None` 表示该侧不设界。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeRange {
    /// 起始时间（含）。
    pub start_ms: Option<i64>,
    /// 结束时间（含）。
    pub end_ms: Option<i64>,
}

impl TimeRange {
    /// 不限制时间（返回全部）。
    pub fn all() -> Self {
        Self {
            start_ms: None,
            end_ms: None,
        }
    }
    /// 只取 `start_ms` 之后（含）的记忆。
    pub fn since(start_ms: i64) -> Self {
        Self {
            start_ms: Some(start_ms),
            end_ms: None,
        }
    }
    /// 只取 `end_ms` 之前（含）的记忆。
    pub fn until(end_ms: i64) -> Self {
        Self {
            start_ms: None,
            end_ms: Some(end_ms),
        }
    }
    /// 闭区间 `[start_ms, end_ms]`。
    pub fn between(start_ms: i64, end_ms: i64) -> Self {
        Self {
            start_ms: Some(start_ms),
            end_ms: Some(end_ms),
        }
    }
    /// 时间戳是否落在范围内。
    pub fn contains(&self, ts: i64) -> bool {
        self.start_ms.is_none_or(|s| ts >= s) && self.end_ms.is_none_or(|e| ts <= e)
    }
}

/// 记忆库门面。
///
/// 线程安全：内部存储通过锁同步，可用 `Arc<AgentMemory>` 在多线程 agent 中共享。
pub struct AgentMemory {
    store: Arc<dyn MemoryStore>,
    graph: Option<Arc<dyn GraphStore>>,
    embedder: Arc<dyn Embedder>,
    extractor: Option<Arc<dyn Extractor>>,
    retrieval: RetrievalConfig,
    working_capacity: usize,
}

impl AgentMemory {
    /// 打开（或创建）一个基于 SQLite 的记忆库（同时初始化知识图谱表）。
    ///
    /// `path` 传 `:memory:` 使用纯内存库（便于测试/演示）。
    pub fn open(path: &str) -> StoreResult<Self> {
        let db = Db::open(path)?;
        let store = SqliteStore::from_db(db.clone());
        let graph = SqliteGraphStore::from_db(db);
        Ok(Self {
            store: Arc::new(store),
            graph: Some(Arc::new(graph)),
            embedder: Arc::new(HashEmbedder::new(512)),
            extractor: None,
            retrieval: RetrievalConfig::default(),
            working_capacity: 50,
        })
    }

    /// 以自定义存储 / 嵌入器 / 检索配置构建（不带知识图谱，适合纯自定义后端）。
    pub fn with_store(
        store: Box<dyn MemoryStore>,
        embedder: Arc<dyn Embedder>,
        retrieval: RetrievalConfig,
    ) -> Self {
        Self {
            store: Arc::from(store),
            graph: None,
            embedder,
            extractor: None,
            retrieval,
            working_capacity: 50,
        }
    }

    /// 挂载知识图谱后端（与自定义存储组合使用）。
    pub fn with_graph(mut self, graph: Arc<dyn GraphStore>) -> Self {
        self.graph = Some(graph);
        self
    }

    /// 开启写入时的自动三元组抽取（规则抽取或 LLM 抽取均可）。
    pub fn with_extractor(mut self, extractor: Arc<dyn Extractor>) -> Self {
        self.extractor = Some(extractor);
        self
    }

    /// 设置工作记忆滚动上限（超出后淘汰最旧条目）。
    pub fn with_working_capacity(mut self, n: usize) -> Self {
        self.working_capacity = n.max(1);
        self
    }

    /// 写入一条记忆，自动评估重要性并计算嵌入。
    pub fn add(
        &self,
        scope: Scope,
        key: &str,
        ty: MemoryType,
        content: &str,
    ) -> StoreResult<MemoryItem> {
        let importance = estimate_importance(content);
        self.add_with_importance(scope, key, ty, content, importance, None, None)
    }

    /// 语义事实便捷入口。
    pub fn remember_fact(&self, scope: Scope, key: &str, content: &str) -> StoreResult<MemoryItem> {
        self.add(scope, key, MemoryType::Semantic, content)
    }

    /// 情景事件便捷入口。
    pub fn remember_event(
        &self,
        scope: Scope,
        key: &str,
        content: &str,
    ) -> StoreResult<MemoryItem> {
        self.add(scope, key, MemoryType::Episodic, content)
    }

    /// 写入一条记忆（显式重要性 / TTL / 元数据）。
    #[allow(clippy::too_many_arguments)] // 保留扁平 API 便于快速调用
    pub fn add_with_importance(
        &self,
        scope: Scope,
        key: &str,
        ty: MemoryType,
        content: &str,
        importance: f32,
        ttl_secs: Option<i64>,
        meta: Option<String>,
    ) -> StoreResult<MemoryItem> {
        let now = now_millis();
        let vec = self.embedder.embed(content)?;
        let item = NewMemory {
            scope,
            scope_key: key.to_string(),
            memory_type: ty,
            content: content.to_string(),
            importance: importance.clamp(0.0, 1.0),
            ttl_secs,
            meta,
        };
        let id = self.store.insert(&item, now)?;
        self.store.set_embedding(id, vec)?;
        if ty == MemoryType::Working {
            self.enforce_working_capacity(scope, key)?;
        }
        // 自动抽取知识图谱三元组（挂载了抽取器时）。
        if let (Some(graph), Some(extractor)) = (&self.graph, &self.extractor) {
            for triple in extractor.extract(content) {
                graph.add_triple(&triple, Some(id), now)?;
            }
        }
        self.store
            .get(id)?
            .ok_or_else(|| StoreError::Other("inserted memory disappeared".into()))
    }

    fn enforce_working_capacity(&self, scope: Scope, key: &str) -> StoreResult<()> {
        let alive = self.store.count(scope, key, Some(MemoryType::Working))?;
        let mut to_delete = alive.saturating_sub(self.working_capacity);
        while to_delete > 0 {
            match self.store.oldest_working(scope, key)? {
                Some(old) => {
                    self.store.delete(old.id)?;
                    to_delete -= 1;
                }
                None => break,
            }
        }
        Ok(())
    }

    /// 检索：按 `时效 × 相关 × 重要` 打分，返回 Top-K。
    pub fn recall(&self, scope: Scope, key: &str, query: &str) -> StoreResult<Vec<ScoredMemory>> {
        self.recall_between(scope, key, query, TimeRange::all())
    }

    /// 检索（限定创建时间范围）：只在 [`TimeRange`] 内的有效记忆中打分排序。
    pub fn recall_between(
        &self,
        scope: Scope,
        key: &str,
        query: &str,
        range: TimeRange,
    ) -> StoreResult<Vec<ScoredMemory>> {
        let now = now_millis();
        let mut candidates = self.store.list(scope, key, None)?;
        candidates
            .retain(|m| !m.is_superseded() && !m.is_expired(now) && range.contains(m.created_at));
        if candidates.is_empty() {
            return Ok(vec![]);
        }

        let qvec = self.embedder.embed(query)?;
        let docs: Vec<&str> = candidates.iter().map(|m| m.content.as_str()).collect();
        let kw = text::bm25_scores(query, &docs);

        let mut scored: Vec<ScoredMemory> = candidates
            .iter()
            .zip(kw)
            .map(|(item, s)| retrieval::score_item(item, Some(&qvec), s, &self.retrieval, now))
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(self.retrieval.top_k);

        for s in &scored {
            let _ = self.store.touch(s.item.id, now);
        }
        Ok(scored)
    }

    /// 删除一条记忆。
    pub fn forget(&self, id: i64) -> StoreResult<()> {
        self.store.delete(id)
    }

    /// 冲突解决：用新内容取代旧记忆，旧条目标记 superseded 而不是静默覆盖。
    pub fn supersede(
        &self,
        scope: Scope,
        key: &str,
        old_id: i64,
        new_content: &str,
    ) -> StoreResult<MemoryItem> {
        let old = self
            .store
            .get(old_id)?
            .ok_or_else(|| StoreError::Other(format!("memory {old_id} not found")))?;
        let ty = old.memory_type;
        let new = self.add(scope, key, ty, new_content)?;
        self.store.mark_superseded(old_id, new.id)?;
        Ok(new)
    }

    /// 遗忘管理：清理已过期条目，返回清理数量。
    pub fn prune(&self) -> StoreResult<usize> {
        self.store.prune_expired(now_millis())
    }

    /// 整合：语义记忆去重合并。
    ///
    /// 对语义记忆中余弦相似度 ≥ `threshold` 的条目，保留较新者、将较旧者标记为被取代。
    /// 返回被合并（取代）的条数。默认阈值建议 0.82 以上，避免误合并。
    pub fn consolidate(&self, scope: Scope, key: &str, threshold: f32) -> StoreResult<usize> {
        let mut items = self.store.list(scope, key, Some(MemoryType::Semantic))?;
        items.sort_by_key(|m| std::cmp::Reverse(m.created_at)); // 新的在前
        let mut merged = 0usize;
        for i in 0..items.len() {
            let a = &items[i];
            if a.is_superseded() {
                continue;
            }
            for b in items.iter().skip(i + 1) {
                if b.is_superseded() {
                    continue;
                }
                if let (Some(av), Some(bv)) = (a.embedding.as_deref(), b.embedding.as_deref()) {
                    if embed::cosine(av, bv) >= threshold {
                        self.store.mark_superseded(b.id, a.id)?;
                        merged += 1;
                    }
                }
            }
        }
        Ok(merged)
    }

    /// 作用域统计。
    pub fn stats(&self, scope: Scope, key: &str) -> StoreResult<MemoryStats> {
        let all = self.store.list(scope, key, None)?;
        let mut st = MemoryStats::default();
        for m in &all {
            if m.is_superseded() {
                st.superseded += 1;
                continue;
            }
            match m.memory_type {
                MemoryType::Working => st.working += 1,
                MemoryType::Episodic => st.episodic += 1,
                MemoryType::Semantic => st.semantic += 1,
            }
        }
        st.total = all.len();
        Ok(st)
    }

    /// 直接读取一条记忆。
    pub fn get(&self, id: i64) -> StoreResult<Option<MemoryItem>> {
        self.store.get(id)
    }

    /// 列出某作用域的有效记忆（隐藏已取代、已过期）。
    pub fn list(
        &self,
        scope: Scope,
        key: &str,
        ty: Option<MemoryType>,
    ) -> StoreResult<Vec<MemoryItem>> {
        self.list_between(scope, key, ty, TimeRange::all())
    }

    /// 列出某作用域、创建时间落在 `range` 内的有效记忆（按创建时间升序）。
    pub fn list_between(
        &self,
        scope: Scope,
        key: &str,
        ty: Option<MemoryType>,
        range: TimeRange,
    ) -> StoreResult<Vec<MemoryItem>> {
        let now = now_millis();
        let mut items: Vec<MemoryItem> = self
            .store
            .list(scope, key, ty)?
            .into_iter()
            .filter(|m| !m.is_superseded() && !m.is_expired(now) && range.contains(m.created_at))
            .collect();
        items.sort_by_key(|m| m.created_at);
        Ok(items)
    }

    /// 访问底层存储（高级用法）。
    pub fn store(&self) -> &Arc<dyn MemoryStore> {
        &self.store
    }
    /// 当前嵌入器。
    pub fn embedder(&self) -> &Arc<dyn Embedder> {
        &self.embedder
    }
    /// 当前检索配置（可变）。
    pub fn retrieval_config_mut(&mut self) -> &mut RetrievalConfig {
        &mut self.retrieval
    }
    /// 当前检索配置（只读）。
    pub fn retrieval_config(&self) -> &RetrievalConfig {
        &self.retrieval
    }

    // ===== 知识图谱能力 =====

    /// 是否启用了知识图谱。
    pub fn has_graph(&self) -> bool {
        self.graph.is_some()
    }

    fn graph_ref(&self) -> StoreResult<&Arc<dyn GraphStore>> {
        self.graph
            .as_ref()
            .ok_or_else(|| StoreError::Other("knowledge graph is not enabled".into()))
    }

    /// 手动写入一条关系三元组，返回边 id（需要图谱已启用）。
    ///
    /// 多值关系（喜欢、使用……）可同时指向多个客体。
    pub fn remember_relation(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
    ) -> StoreResult<i64> {
        let graph = self.graph_ref()?;
        graph.add_triple(&Triple::new(subject, predicate, object), None, now_millis())
    }

    /// 用新事实替换某主体的单值关系（住在、任职于……），旧边标记失效可追溯。
    pub fn replace_relation(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
    ) -> StoreResult<i64> {
        let graph = self.graph_ref()?;
        graph.replace_triple(&Triple::new(subject, predicate, object), None, now_millis())
    }

    /// 查询某实体 `depth` 跳内的无向邻居边。
    pub fn graph_neighbors(&self, entity: &str, depth: usize) -> StoreResult<Vec<Edge>> {
        self.graph_ref()?.neighbors(entity, depth)
    }

    /// 查询从 `from` 到 `to` 不超过 `max_depth` 跳的全部有向路径。
    pub fn graph_paths(
        &self,
        from: &str,
        to: &str,
        max_depth: usize,
    ) -> StoreResult<Vec<Vec<Edge>>> {
        self.graph_ref()?.find_paths(from, to, max_depth)
    }

    /// 列出图谱中的全部实体。
    pub fn graph_entities(&self) -> StoreResult<Vec<crate::graph::Entity>> {
        self.graph_ref()?.entities()
    }

    /// 列出图谱边；`include_invalid` 为 true 时含已失效边。
    pub fn graph_edges(&self, include_invalid: bool) -> StoreResult<Vec<Edge>> {
        self.graph_ref()?.edges(include_invalid)
    }

    /// 主动失效某主体的某关系。
    pub fn invalidate_relation(&self, subject: &str, predicate: &str) -> StoreResult<usize> {
        self.graph_ref()?
            .invalidate(subject, predicate, now_millis())
    }

    /// 社区发现：返回弱连通分量（每组是一个实体社区）。
    pub fn graph_communities(&self) -> StoreResult<Vec<Vec<String>>> {
        self.graph_ref()?.communities()
    }

    /// 实体消歧：把别名实体 `alias` 合并进规范实体 `keep`，返回受影响边数。
    pub fn merge_graph_entities(&self, keep: &str, alias: &str) -> StoreResult<usize> {
        self.graph_ref()?.merge_entities(keep, alias)
    }

    /// 图谱统计摘要（实体/边/社区）。
    pub fn graph_summary(&self) -> StoreResult<crate::graph::GraphStats> {
        self.graph_ref()?.graph_stats()
    }

    /// 检索记忆的同时，附带查询中提到实体的 1 跳图谱关系。
    ///
    /// 返回 `(记忆命中, 相关边)`：向量/关键词负责"语义相似"，图谱负责"实体关系"，
    /// 两者互补，可一并拼进 agent 的上下文。
    pub fn recall_with_graph(
        &self,
        scope: Scope,
        key: &str,
        query: &str,
    ) -> StoreResult<(Vec<ScoredMemory>, Vec<Edge>)> {
        let hits = self.recall(scope, key, query)?;
        let mut edges = Vec::new();
        if let Some(graph) = &self.graph {
            let mut seen = std::collections::HashSet::new();
            for entity in graph.entities()? {
                if entity.name.len() >= 2 && query.contains(&entity.name) {
                    for e in graph.neighbors(&entity.name, 1)? {
                        if seen.insert(e.id) {
                            edges.push(e);
                        }
                    }
                }
            }
        }
        Ok((hits, edges))
    }
}

/// 当前 epoch 毫秒。
pub(crate) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 无 LLM 的重要性启发式评估：信号词 + 内容长度。
///
/// 仅供离线默认使用；生产环境可替换为 LLM 抽取（重要性会影响检索排序）。
fn estimate_importance(content: &str) -> f32 {
    let lower = content.to_lowercase();
    let strong = [
        "重要",
        "务必",
        "记住",
        "关键",
        "always",
        "never",
        "important",
        "prefer",
        "喜欢",
        "不喜欢",
        "讨厌",
        "用户是",
        "我是",
        "user is",
        "i am",
        "must",
        "priority",
        "核心",
    ];
    let mut boost = 0.0;
    for w in strong {
        if lower.contains(w) {
            boost += 0.12;
        }
    }
    let len_factor = (content.chars().count() as f32 / 200.0).min(0.2);
    (0.5 + boost + len_factor).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> AgentMemory {
        AgentMemory::open(":memory:").unwrap()
    }

    #[test]
    fn add_and_recall_fact() {
        let m = mem();
        m.remember_fact(Scope::User, "u1", "用户喜欢用 Rust 编写 Agent")
            .unwrap();
        m.remember_fact(Scope::User, "u1", "用户正在准备面试")
            .unwrap();
        let hits = m.recall(Scope::User, "u1", "用户喜欢什么语言").unwrap();
        assert!(!hits.is_empty());
        assert!(
            hits[0].item.content.contains("Rust"),
            "related fact should rank first: {:?}",
            hits
        );
    }

    #[test]
    fn scope_isolation() {
        let m = mem();
        m.remember_fact(Scope::User, "alice", "Alice 喜欢咖啡")
            .unwrap();
        m.remember_fact(Scope::User, "bob", "Bob 喜欢茶").unwrap();
        let hits = m.recall(Scope::User, "alice", "喝什么").unwrap();
        assert!(hits.iter().all(|h| h.item.content.contains("咖啡")));
    }

    #[test]
    fn supersede_marks_old() {
        let m = mem();
        let old = m.remember_fact(Scope::User, "u", "用户住在北京").unwrap();
        let new = m
            .supersede(Scope::User, "u", old.id, "用户搬到上海")
            .unwrap();
        let old_after = m.get(old.id).unwrap().unwrap();
        assert!(old_after.is_superseded());
        assert_eq!(old_after.superseded_by, Some(new.id));
        // 检索时旧事实不再出现
        let hits = m.recall(Scope::User, "u", "用户住在哪").unwrap();
        assert!(hits.iter().all(|h| !h.item.is_superseded()));
    }

    #[test]
    fn ttl_prune_removes_expired() {
        let m = mem();
        m.add_with_importance(
            Scope::Session,
            "s1",
            MemoryType::Working,
            "临时提醒",
            0.5,
            Some(1),
            None,
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(m.prune().unwrap(), 1);
        assert!(m.list(Scope::Session, "s1", None).unwrap().is_empty());
    }

    #[test]
    fn working_capacity_evicts_oldest() {
        let m = mem().with_working_capacity(3);
        for i in 0..5 {
            m.add(
                Scope::Session,
                "s",
                MemoryType::Working,
                &format!("msg {i}"),
            )
            .unwrap();
        }
        let alive = m
            .list(Scope::Session, "s", Some(MemoryType::Working))
            .unwrap();
        assert_eq!(alive.len(), 3);
        // 最旧的 0、1 被淘汰
        assert!(!alive.iter().any(|x| x.content == "msg 0"));
        assert!(!alive.iter().any(|x| x.content == "msg 1"));
    }

    #[test]
    fn consolidate_dedups_semantic() {
        let m = mem();
        m.remember_fact(Scope::User, "u", "用户喜欢喝美式咖啡")
            .unwrap();
        m.remember_fact(Scope::User, "u", "用户喜欢美式咖啡")
            .unwrap();
        let merged = m.consolidate(Scope::User, "u", 0.8).unwrap();
        assert!(merged >= 1);
        let alive = m
            .list(Scope::User, "u", Some(MemoryType::Semantic))
            .unwrap();
        assert_eq!(alive.len(), 1);
    }

    #[test]
    fn stats_counts_types() {
        let m = mem();
        m.remember_fact(Scope::User, "u", "事实").unwrap();
        m.remember_event(Scope::User, "u", "事件").unwrap();
        m.add(Scope::User, "u", MemoryType::Working, "消息")
            .unwrap();
        let s = m.stats(Scope::User, "u").unwrap();
        assert_eq!(s.semantic, 1);
        assert_eq!(s.episodic, 1);
        assert_eq!(s.working, 1);
        assert_eq!(s.total, 3);
    }

    #[test]
    fn custom_store_works() {
        use crate::store::testutil::MemStore;
        let m = AgentMemory::with_store(
            Box::new(MemStore::new()),
            Arc::new(HashEmbedder::new(256)),
            RetrievalConfig::default(),
        );
        m.remember_fact(Scope::User, "u", "测试自定义存储").unwrap();
        assert_eq!(m.recall(Scope::User, "u", "存储").unwrap().len(), 1);
    }

    #[test]
    fn graph_enabled_by_default_and_manual_relation() {
        let m = mem();
        assert!(m.has_graph());
        m.remember_relation("用户", "喜欢", "Rust").unwrap();
        m.remember_relation("Rust", "适合", "系统编程").unwrap();
        let n = m.graph_neighbors("用户", 2).unwrap();
        assert_eq!(n.len(), 2);
        let paths = m.graph_paths("用户", "系统编程", 2).unwrap();
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn auto_extract_populates_graph() {
        use crate::extract::RuleExtractor;
        let m = mem().with_extractor(Arc::new(RuleExtractor::new()));
        m.remember_fact(Scope::User, "u", "用户喜欢 Rust").unwrap();
        let edges = m.graph_edges(false).unwrap();
        assert!(edges
            .iter()
            .any(|e| e.subject == "用户" && e.predicate == "喜欢" && e.object == "Rust"));
    }

    #[test]
    fn recall_with_graph_returns_related_edges() {
        let m = mem();
        m.remember_relation("用户", "住在", "北京").unwrap();
        m.remember_fact(Scope::User, "u", "用户在北京工作多年")
            .unwrap();
        let (hits, edges) = m
            .recall_with_graph(Scope::User, "u", "用户住在哪里")
            .unwrap();
        assert!(!hits.is_empty());
        assert!(edges.iter().any(|e| e.object == "北京"));
    }

    #[test]
    fn graph_temporal_change() {
        let m = mem();
        m.replace_relation("用户", "住在", "北京").unwrap();
        m.replace_relation("用户", "住在", "上海").unwrap();
        let valid = m.graph_edges(false).unwrap();
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].object, "上海");
        assert_eq!(m.graph_edges(true).unwrap().len(), 2);
    }

    #[test]
    fn time_range_filters_recall_and_list() {
        let m = mem();
        let a = m
            .remember_fact(Scope::User, "u", "第一条事实 Alpha")
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = m
            .remember_fact(Scope::User, "u", "第二条事实 Beta")
            .unwrap();
        assert!(b.created_at > a.created_at);

        // since(b) 只包含 b
        let later = m
            .list_between(Scope::User, "u", None, TimeRange::since(b.created_at))
            .unwrap();
        assert_eq!(later.len(), 1);
        assert_eq!(later[0].id, b.id);

        // until(a) 只包含 a
        let earlier = m
            .list_between(Scope::User, "u", None, TimeRange::until(a.created_at))
            .unwrap();
        assert_eq!(earlier.len(), 1);
        assert_eq!(earlier[0].id, a.id);

        // recall_between 同样受时间范围约束
        let hits = m
            .recall_between(Scope::User, "u", "事实", TimeRange::since(b.created_at))
            .unwrap();
        assert!(hits.iter().all(|h| h.item.id == b.id));

        // TimeRange::contains 边界为闭区间
        assert!(TimeRange::between(10, 20).contains(10));
        assert!(TimeRange::between(10, 20).contains(20));
        assert!(!TimeRange::between(10, 20).contains(9));
    }
}
