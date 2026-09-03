//! 轻量全文评分：多语言分词 + BM25 近似相关度。
//!
//! 记忆检索的相关度采用 BM25 近似：对查询分词，对每条候选记忆计算
//! `Σ idf(t)·tf(t,d)/(tf(t,d)+k1·(1-b+b·dl/avgdl))`，idf 从候选集内部估计，
//! 无需外部索引即可工作，结果确定、可测试。
//!
//! 分词规则与 [`crate::embed::HashEmbedder`] 保持一致：
//! 英文按词（小写、去标点），中文按「字符二元组 + 单字」切分。

use std::collections::{HashMap, HashSet};

/// BM25 饱和因子。
pub const K1: f32 = 1.2;
/// BM25 长度归一化系数。
pub const B: f32 = 0.75;

/// 判断是否 CJK 字符（中日韩统一表意文字、扩展区、兼容区、标点与假名）。
pub fn is_cjk(c: char) -> bool {
    let u = c as u32;
    (0x4E00..=0x9FFF).contains(&u)          // 中日韩统一表意文字
        || (0x3400..=0x4DBF).contains(&u)   // 扩展 A
        || (0xF900..=0xFAFF).contains(&u)   // 兼容表意文字
        || (0x3000..=0x303F).contains(&u)   // CJK 标点
        || (0x3040..=0x30FF).contains(&u) // 日文假名
}

/// 多语言分词：英文按词、中文按字符二元组 + 单字。
pub fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut tokens = Vec::new();
    let mut word = String::new();
    let flush = |w: &str, toks: &mut Vec<String>| {
        if w.is_empty() {
            return;
        }
        if w.chars().any(is_cjk) {
            let chars: Vec<char> = w.chars().collect();
            for win in chars.windows(2) {
                toks.push(win.iter().collect::<String>());
            }
            for c in &chars {
                if c.is_alphanumeric() {
                    toks.push(c.to_string());
                }
            }
        } else {
            toks.push(w.to_string());
        }
    };
    for ch in lower.chars() {
        if ch.is_alphanumeric() {
            word.push(ch);
        } else {
            flush(&word, &mut tokens);
            word.clear();
        }
    }
    flush(&word, &mut tokens);
    tokens
}

/// 计算 idf：ln(1 + N/(1+df))，N 为文档总数，df 为包含该词的文档数。
pub fn idf(doc_freq: usize, total: usize) -> f32 {
    if total == 0 {
        return 0.0;
    }
    (1.0 + (total as f32) / (1.0 + doc_freq as f32)).ln()
}

/// 对候选文档集合返回每个文档的 BM25 关键词相关度，归一化到 \[0,1\]。
///
/// `docs[i]` 对应返回的 `scores[i]`。
pub fn bm25_scores(query: &str, docs: &[&str]) -> Vec<f32> {
    let n = docs.len();
    if n == 0 || query.trim().is_empty() {
        return vec![0.0; n];
    }

    // 统计文档词频、词项文档频率与文档长度。
    let mut df: HashMap<String, usize> = HashMap::new();
    let mut tfs: Vec<HashMap<String, f32>> = Vec::with_capacity(n);
    let mut lens = Vec::with_capacity(n);
    let mut total_len = 0usize;

    for d in docs {
        let toks = tokenize(d);
        let mut tf: HashMap<String, f32> = HashMap::new();
        for t in &toks {
            *tf.entry(t.clone()).or_insert(0.0) += 1.0;
        }
        for t in tf.keys() {
            *df.entry(t.clone()).or_insert(0) += 1;
        }
        tfs.push(tf);
        let l = toks.len();
        lens.push(l);
        total_len += l;
    }
    let avgdl = if n > 0 {
        total_len as f32 / n as f32
    } else {
        1.0
    };

    // 查询去重词项。
    let mut seen = HashSet::new();
    let q_terms: Vec<String> = tokenize(query)
        .into_iter()
        .filter(|t| seen.insert(t.clone()))
        .collect();

    let mut scores = vec![0.0f32; n];
    for (i, tfmap) in tfs.iter().enumerate() {
        let dl = lens[i] as f32;
        let mut s = 0.0;
        for t in &q_terms {
            let tf = *tfmap.get(t).unwrap_or(&0.0);
            if tf <= 0.0 {
                continue;
            }
            let df_v = *df.get(t).unwrap_or(&1);
            let idf_v = idf(df_v, n);
            let denom = tf + K1 * (1.0 - B + B * dl / avgdl.max(1.0));
            s += idf_v * (tf * (K1 + 1.0)) / denom;
        }
        scores[i] = s;
    }

    let max = scores.iter().cloned().fold(f32::MIN, f32::max);
    if max > 0.0 {
        for s in scores.iter_mut() {
            *s /= max;
        }
    }
    scores
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_mixed_language() {
        let toks = tokenize("我喜欢 Rust & Go");
        // 中文二元组 + 单字 + 英文词
        assert!(toks.contains(&"我喜".to_string()));
        assert!(toks.contains(&"rust".to_string()));
        assert!(toks.contains(&"go".to_string()));
        // 标点被过滤
        assert!(!toks.iter().any(|t| t.contains('&')));
    }

    #[test]
    fn bm25_ranks_relevant_first() {
        let docs = [
            "用户喜欢用 Rust 编写 Agent 记忆系统",
            "今天北京下雨，适合在家看代码",
            "Rust 的所有权和借用检查",
        ];
        let scores = bm25_scores("Rust 记忆", &docs);
        assert!(scores[0] > scores[1]);
        assert!(scores[0] > scores[2]);
    }

    #[test]
    fn bm25_empty_safe() {
        assert_eq!(bm25_scores("", &["a"]), vec![0.0]);
        assert_eq!(bm25_scores("q", &[]), Vec::<f32>::new());
    }
}
