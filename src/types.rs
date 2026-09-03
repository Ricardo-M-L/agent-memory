//! 记忆数据模型：记忆类型、作用域、单条记忆。
//!
//! 类型与作用域的划分对齐业界主流（CoALA 认知架构 / Mem0 的 user-session-agent 分层）。

use serde::{Deserialize, Serialize};

/// 记忆类型（对齐认知科学分类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryType {
    /// 工作/短期记忆：最近一段交互的原始消息，滚动保留
    Working,
    /// 情景记忆：发生过的事件、任务轨迹
    Episodic,
    /// 语义记忆：事实、偏好、实体关系
    Semantic,
}

impl MemoryType {
    /// 稳定字符串表示（用于持久化）。
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryType::Working => "working",
            MemoryType::Episodic => "episodic",
            MemoryType::Semantic => "semantic",
        }
    }
}

impl std::str::FromStr for MemoryType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "working" | "short" | "work" => Ok(MemoryType::Working),
            "episodic" | "event" => Ok(MemoryType::Episodic),
            "semantic" | "fact" => Ok(MemoryType::Semantic),
            other => Err(format!("unknown memory type: {other}")),
        }
    }
}

/// 记忆作用域：隔离不同主体的记忆空间。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    /// 某个用户（跨会话持久）
    User,
    /// 某次会话（随会话生命周期）
    Session,
    /// 某个 agent（跨用户共享）
    Agent,
}

impl Scope {
    /// 稳定字符串表示（用于持久化）。
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::User => "user",
            Scope::Session => "session",
            Scope::Agent => "agent",
        }
    }
}

impl std::str::FromStr for Scope {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Scope::User),
            "session" => Ok(Scope::Session),
            "agent" => Ok(Scope::Agent),
            other => Err(format!("unknown scope: {other}")),
        }
    }
}

/// 待写入的新记忆（不含 id 与统计字段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewMemory {
    pub scope: Scope,
    pub scope_key: String,
    pub memory_type: MemoryType,
    pub content: String,
    pub importance: f32,
    pub ttl_secs: Option<i64>,
    pub meta: Option<String>,
}

/// 一条已持久化的记忆。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: i64,
    pub scope: Scope,
    pub scope_key: String,
    pub memory_type: MemoryType,
    pub content: String,
    /// 重要性 0.0~1.0，参与检索打分
    pub importance: f32,
    /// 创建时间（epoch 毫秒）
    pub created_at: i64,
    /// 最近访问时间（epoch 毫秒）
    pub last_access_at: i64,
    /// 访问次数
    pub access_count: i64,
    /// 冲突解决：被哪条新记忆取代（None 表示仍是有效记忆）
    pub superseded_by: Option<i64>,
    /// 可选过期时间（秒）
    pub ttl_secs: Option<i64>,
    /// 语义向量（可选）
    pub embedding: Option<Vec<f32>>,
    /// 额外元数据（JSON 字符串，可选）
    pub meta: Option<String>,
}

impl MemoryItem {
    /// 是否已被更新的记忆取代（冲突解决后的旧事实）。
    pub fn is_superseded(&self) -> bool {
        self.superseded_by.is_some()
    }

    /// 是否已过期。
    pub fn is_expired(&self, now_millis: i64) -> bool {
        match self.ttl_secs {
            Some(ttl) => now_millis - self.created_at > ttl * 1000,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn memory_type_roundtrip() {
        for t in [
            MemoryType::Working,
            MemoryType::Episodic,
            MemoryType::Semantic,
        ] {
            assert_eq!(MemoryType::from_str(t.as_str()), Ok(t));
        }
        assert!(MemoryType::from_str("bogus").is_err());
    }

    #[test]
    fn scope_roundtrip() {
        for s in [Scope::User, Scope::Session, Scope::Agent] {
            assert_eq!(Scope::from_str(s.as_str()), Ok(s));
        }
    }

    #[test]
    fn expiry_logic() {
        let item = MemoryItem {
            id: 1,
            scope: Scope::User,
            scope_key: "u".into(),
            memory_type: MemoryType::Semantic,
            content: "x".into(),
            importance: 0.5,
            created_at: 1_000,
            last_access_at: 1_000,
            access_count: 0,
            superseded_by: None,
            ttl_secs: Some(10),
            embedding: None,
            meta: None,
        };
        assert!(!item.is_expired(1_000 + 9_000)); // 10s TTL，9s 未过期
        assert!(item.is_expired(1_000 + 11_000)); // 11s 已过期
        assert!(!item.is_superseded());
        let mut s = item.clone();
        s.superseded_by = Some(2);
        assert!(s.is_superseded());
    }
}
