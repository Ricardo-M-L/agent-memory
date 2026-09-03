//! 实体/关系抽取层：从自然语言文本抽取图谱三元组。
//!
//! 知识图谱的质量上限取决于抽取。生产级系统（Zep/Graphiti、Mem0）通常用 **LLM** 做
//! 结构化抽取；为了保持离线可用，本模块提供一个**规则抽取器** [`RuleExtractor`] 作为兜底，
//! 它能处理「X 喜欢 Y」「X lives in Y」这类显式句式，置信度设为较低值（0.6）。
//!
//! 需要更高质量时，实现 [`Extractor`] trait 接入 LLM（让模型输出 JSON 三元组）即可，
//! 其余存储/检索逻辑无需改动。

use crate::graph::Triple;

/// 抽取器抽象。
pub trait Extractor: Send + Sync {
    /// 从一段文本中抽取若干三元组（可能为空）。
    fn extract(&self, text: &str) -> Vec<Triple>;
}

/// 规则抽取器：基于中英显式关系触发词，离线、确定、可测试。
pub struct RuleExtractor {
    /// 句子缺少主语时使用的默认主体（如第一人称陈述默认归到「用户」）。
    default_subject: Option<String>,
}

impl Default for RuleExtractor {
    fn default() -> Self {
        Self {
            default_subject: Some("用户".to_string()),
        }
    }
}

impl RuleExtractor {
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置缺主语时的默认主体；传 None 表示缺主语则不抽取。
    pub fn with_default_subject(mut self, subject: Option<String>) -> Self {
        self.default_subject = subject;
        self
    }
}

/// 中文触发词 → 关系名。**长词必须排在前面**（如「不喜欢」先于「喜欢」）。
const ZH_PATTERNS: &[(&str, &str)] = &[
    ("不喜欢", "不喜欢"),
    ("讨厌", "不喜欢"),
    ("任职于", "任职于"),
    ("就职于", "任职于"),
    ("住在", "住在"),
    ("擅长", "擅长"),
    ("偏好", "偏好"),
    ("喜欢", "喜欢"),
    ("使用", "使用"),
    ("用的是", "使用"),
];

/// 英文触发词 → 关系名（多词短语优先）。
const EN_PATTERNS: &[(&str, &str)] = &[
    ("doesn't like", "dislikes"),
    ("does not like", "dislikes"),
    ("lives in", "lives_in"),
    ("works at", "works_at"),
    ("is a", "is_a"),
    ("dislikes", "dislikes"),
    ("prefers", "prefers"),
    ("likes", "likes"),
    ("loves", "likes"),
    ("uses", "uses"),
];

/// 中文客体清理时，遇到这些词截断（它们通常引出后续动作，不是客体的一部分）。
const ZH_OBJ_STOP: &[&str] = &[
    "写", "做", "来", "去", "进行", "以及", "和", "，", ",", "。", "；", ";",
];
/// 中文客体前的轻动词/虚词，直接剥掉。
const ZH_OBJ_LEAD: &[&str] = &["用", "在", "了", "的", "是", "喝", "吃", "去"];
const EN_OBJ_STOP: &[&str] = &["and", "to", "for", "with", ",", ".", ";"];
const EN_OBJ_LEAD: &[&str] = &["a", "an", "the", "to", "at", "in"];

fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        cur.push(ch);
        if "。！？!?\n;；".contains(ch) {
            let s = cur.trim();
            if !s.is_empty() {
                out.push(s.to_string());
            }
            cur.clear();
        }
    }
    let tail = cur.trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| {
        let u = c as u32;
        (0x4E00..=0x9FFF).contains(&u)
    })
}

/// 取触发词前的最后一个小句作为主体。
fn clean_subject(prefix: &str, default: &Option<String>) -> Option<String> {
    let clauses: Vec<&str> = prefix.split(['，', ',', '。', '；', ';']).collect();
    let last = clauses.last()?.trim();
    if last.is_empty() {
        return default.clone();
    }
    Some(last.to_string())
}

/// 取触发词后的客体，剥掉引导虚词并在停用词处截断。
fn clean_object_zh(suffix: &str) -> String {
    let mut s = suffix.trim().to_string();
    loop {
        let stripped = ZH_OBJ_LEAD
            .iter()
            .find_map(|w| s.strip_prefix(w))
            .map(|rest| rest.trim().to_string());
        match stripped {
            Some(rest) if rest != s => s = rest,
            _ => break,
        }
    }
    for stop in ZH_OBJ_STOP {
        if let Some(idx) = s.find(stop) {
            s = s[..idx].trim().to_string();
        }
    }
    s.trim_matches(|c: char| !c.is_alphanumeric() && !('一'..='鿿').contains(&c))
        .trim()
        .to_string()
}

fn clean_object_en(suffix: &str) -> String {
    let lower = suffix.trim().to_lowercase();
    let mut words: Vec<&str> = lower.split_whitespace().collect();
    while let Some(first) = words.first() {
        if EN_OBJ_LEAD.contains(first) {
            words.remove(0);
        } else {
            break;
        }
    }
    let mut kept = Vec::new();
    for w in words {
        let bare = w.trim_matches(|c: char| !c.is_alphanumeric());
        if EN_OBJ_STOP.contains(&bare) {
            break;
        }
        kept.push(bare);
    }
    kept.join(" ")
}

impl Extractor for RuleExtractor {
    fn extract(&self, text: &str) -> Vec<Triple> {
        let mut triples = Vec::new();
        for sent in split_sentences(text) {
            let cjk = has_cjk(&sent);
            let patterns: &[(&str, &str)] = if cjk { ZH_PATTERNS } else { EN_PATTERNS };
            let lower_sent = if cjk {
                sent.clone()
            } else {
                sent.to_lowercase()
            };

            // 找到最早出现的触发词（patterns 已按长度/特异性排序）。
            let hit = patterns
                .iter()
                .filter_map(|(trig, pred)| {
                    lower_sent.find(trig).map(|idx| (idx, trig.len(), *pred))
                })
                .min_by_key(|(idx, _, _)| *idx);

            if let Some((idx, len, predicate)) = hit {
                let prefix = &lower_sent[..idx];
                let suffix = &lower_sent[idx + len..];
                let subject = match clean_subject(prefix, &self.default_subject) {
                    Some(s) if !s.is_empty() => s,
                    _ => continue,
                };
                let object = if cjk {
                    clean_object_zh(suffix)
                } else {
                    clean_object_en(suffix)
                };
                if object.is_empty() || subject == object {
                    continue;
                }
                triples.push(Triple::new(subject, predicate, object).with_confidence(0.6));
            }
        }
        triples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_explicit_patterns() {
        let ex = RuleExtractor::new();
        let t = ex.extract("用户喜欢 Rust");
        assert_eq!(
            t,
            vec![Triple::new("用户", "喜欢", "Rust").with_confidence(0.6)]
        );

        let t = ex.extract("我住在北京");
        assert_eq!(t[0].subject, "我");
        assert_eq!(t[0].predicate, "住在");
        assert_eq!(t[0].object, "北京");
    }

    #[test]
    fn chinese_object_truncates_trailing_action() {
        let ex = RuleExtractor::new();
        let t = ex.extract("用户喜欢用 Rust 写 Agent 应用");
        assert_eq!(t[0].object, "Rust");
    }

    #[test]
    fn negation_has_priority() {
        let ex = RuleExtractor::new();
        let t = ex.extract("用户不喜欢加班");
        assert_eq!(t[0].predicate, "不喜欢");
        assert_eq!(t[0].object, "加班");
    }

    #[test]
    fn english_patterns() {
        let ex = RuleExtractor::new().with_default_subject(Some("user".into()));
        let t = ex.extract("Alice lives in Beijing");
        assert_eq!(
            t[0],
            Triple::new("alice", "lives_in", "beijing").with_confidence(0.6)
        );

        let t = ex.extract("The user works at Google");
        assert_eq!(t[0].subject, "the user");
        assert_eq!(t[0].predicate, "works_at");
        assert_eq!(t[0].object, "google");
    }

    #[test]
    fn default_subject_when_missing() {
        let ex = RuleExtractor::new();
        let t = ex.extract("喜欢喝冷萃咖啡");
        assert_eq!(t[0].subject, "用户");
        assert_eq!(t[0].object, "冷萃咖啡");

        let ex2 = RuleExtractor::new().with_default_subject(None);
        assert!(ex2.extract("喜欢喝冷萃咖啡").is_empty());
    }

    #[test]
    fn no_match_returns_empty() {
        let ex = RuleExtractor::new();
        assert!(ex.extract("今天天气不错").is_empty());
    }

    #[test]
    fn multiple_sentences() {
        let ex = RuleExtractor::new();
        let t = ex.extract("用户喜欢 Rust。用户住在上海。");
        assert_eq!(t.len(), 2);
    }
}
