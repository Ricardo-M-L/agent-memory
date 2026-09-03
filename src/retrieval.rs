//! 检索打分：`时效 × 相关 × 重要`（recency × relevance × importance）。
//!
//! 该打分框架源自 *Generative Agents: Interactive Simulacra of Human Behavior*
//! (Park et al., 2023)，并被现代 AI 记忆系统广泛沿用。本项目将三者归一化到 [0,1]
//! 后做加权求和：
//!
//! - **recency**：指数半衰期衰减 `2^(-age/halflife)`；
//! - **relevance**：BM25 关键词分数与向量余弦相似度的混合（权重由 `vector_mix` 控制）；
//! - **importance**：写入时评估的记忆重要性。

use crate::embed::cosine;
use crate::types::MemoryItem;

/// 检索配置。
#[derive(Debug, Clone)]
pub struct RetrievalConfig {
    /// 返回的 Top-K 条数。
    pub top_k: usize,
    /// 时效权重。
    pub recency_weight: f32,
    /// 相关度权重。
    pub relevance_weight: f32,
    /// 重要性权重。
    pub importance_weight: f32,
    /// 时效衰减半衰期（秒）。
    pub recency_halflife_secs: f64,
    /// 关键词与向量相关度的混合比例：0=纯关键词，1=纯向量。
    pub vector_mix: f32,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 5,
            recency_weight: 0.33,
            relevance_weight: 0.44,
            importance_weight: 0.23,
            // 7 天半衰期：约 1 天衰减到 0.9，7 天衰减到 0.5。
            recency_halflife_secs: 7.0 * 24.0 * 3600.0,
            vector_mix: 0.5,
        }
    }
}

/// 一条带评分的检索结果。
#[derive(Debug, Clone)]
pub struct ScoredMemory {
    pub item: MemoryItem,
    /// 加权总分（用于排序）。
    pub score: f32,
    /// 相关度分量 [0,1]。
    pub relevance: f32,
    /// 时效分量 [0,1]。
    pub recency: f32,
    /// 重要性分量 [0,1]。
    pub importance: f32,
}

/// 对单条记忆打分。
///
/// - `query_vec`：查询向量（可选，用于向量混合）。
/// - `kw_score`：BM25 关键词相关度 [0,1]。
/// - `cfg`：检索配置。
/// - `now`：当前 epoch 毫秒。
pub fn score_item(
    item: &MemoryItem,
    query_vec: Option<&[f32]>,
    kw_score: f32,
    cfg: &RetrievalConfig,
    now: i64,
) -> ScoredMemory {
    let age_secs = ((now - item.created_at) as f64 / 1000.0).max(0.0);
    let recency = 2f32.powf(-(age_secs / cfg.recency_halflife_secs) as f32);

    let vector_sim = match (query_vec, item.embedding.as_deref()) {
        (Some(q), Some(e)) => cosine(q, e),
        _ => 0.0,
    };
    let relevance = (1.0 - cfg.vector_mix) * kw_score + cfg.vector_mix * vector_sim;

    let importance = item.importance;

    let score = cfg.recency_weight * recency
        + cfg.relevance_weight * relevance
        + cfg.importance_weight * importance;

    ScoredMemory {
        item: item.clone(),
        score,
        relevance,
        recency,
        importance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MemoryType, Scope};

    fn item(id: i64, created: i64, importance: f32, embedding: Option<Vec<f32>>) -> MemoryItem {
        MemoryItem {
            id,
            scope: Scope::User,
            scope_key: "u".into(),
            memory_type: MemoryType::Semantic,
            content: format!("memory {id}"),
            importance,
            created_at: created,
            last_access_at: created,
            access_count: 0,
            superseded_by: None,
            ttl_secs: None,
            embedding,
            meta: None,
        }
    }

    #[test]
    fn recency_decays() {
        let cfg = RetrievalConfig::default();
        let now = 1_000_000_000_000i64;
        let fresh = item(1, now - 1000, 0.5, None);
        let old = item(2, now - cfg.recency_halflife_secs as i64 * 1000, 0.5, None);
        let a = score_item(&fresh, None, 0.0, &cfg, now);
        let b = score_item(&old, None, 0.0, &cfg, now);
        assert!(a.recency > b.recency, "fresh should have higher recency");
    }

    #[test]
    fn relevance_and_importance_raise_score() {
        let cfg = RetrievalConfig::default();
        let now = 1_000_000_000_000i64;
        let relevant = item(1, now - 1000, 0.5, None);
        let irrelevant = item(2, now - 1000, 0.5, None);
        let a = score_item(&relevant, None, 0.9, &cfg, now);
        let b = score_item(&irrelevant, None, 0.1, &cfg, now);
        assert!(a.score > b.score);
    }

    #[test]
    fn vector_similarity_boost() {
        let cfg = RetrievalConfig {
            vector_mix: 1.0,
            ..Default::default()
        };
        let now = 1_000_000_000_000i64;
        let q = vec![1.0, 0.0, 0.0];
        let near = item(1, now - 1000, 0.0, Some(vec![1.0, 0.0, 0.0]));
        let far = item(2, now - 1000, 0.0, Some(vec![0.0, 1.0, 0.0]));
        let a = score_item(&near, Some(&q), 0.0, &cfg, now);
        let b = score_item(&far, Some(&q), 0.0, &cfg, now);
        assert!(a.relevance > b.relevance);
    }
}
