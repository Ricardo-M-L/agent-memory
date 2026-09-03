//! 记忆摘要：抽取式摘要（离线、确定）。
//!
//! 用于情景记忆的压缩展示 / 长期对话的紧凑化。实现为基于词频加权的抽取式摘要：
//! 按句子与全文的词频重叠度打分，挑选代表句并保持原文顺序。
//! 需要更高质量摘要时可实现 [`Summarizer`] trait 接入 LLM。

use std::collections::HashMap;

use crate::text::tokenize;

/// 摘要器抽象。
pub trait Summarizer: Send + Sync {
    /// 把长文本压缩为不超过 `max_sentences` 句的摘要。
    fn summarize(&self, text: &str, max_sentences: usize) -> String;
}

/// 抽取式摘要器：词频加权选句，保持原文顺序。
pub struct ExtractiveSummarizer;

impl Summarizer for ExtractiveSummarizer {
    fn summarize(&self, text: &str, max_sentences: usize) -> String {
        let sentences = split_sentences(text);
        if sentences.is_empty() || max_sentences == 0 {
            return String::new();
        }
        if sentences.len() <= max_sentences {
            return sentences.join("");
        }

        // 全文字词频。
        let mut freq: HashMap<String, usize> = HashMap::new();
        for s in &sentences {
            for t in tokenize(s) {
                *freq.entry(t).or_insert(0) += 1;
            }
        }

        // 句子得分 = 平均词频（长度归一化）。
        let mut ranked: Vec<(usize, f32)> = sentences
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let toks = tokenize(s);
                let n = toks.len().max(1) as f32;
                let score: f32 = toks
                    .iter()
                    .map(|t| *freq.get(t).unwrap_or(&0) as f32)
                    .sum::<f32>()
                    / n;
                (i, score)
            })
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(max_sentences);
        ranked.sort_by_key(|(i, _)| *i); // 恢复原文顺序

        ranked
            .into_iter()
            .map(|(i, _)| sentences[i].as_str())
            .collect()
    }
}

/// 按句子结束标点 / 换行切句（保留标点）。
fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        cur.push(ch);
        if "。！？!?\n".contains(ch) {
            let s = cur.trim().to_string();
            if !s.is_empty() {
                out.push(s);
            }
            cur.clear();
        }
    }
    let tail = cur.trim().to_string();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_untouched() {
        let s = ExtractiveSummarizer;
        assert_eq!(s.summarize("只有一句。", 3), "只有一句。");
    }

    #[test]
    fn long_text_compresses_and_keeps_order() {
        let text = "第一句讲用户的背景。第二句提到用户喜欢 Rust。第三句说用户正在找工作。第四句是无关紧要的补充说明内容。";
        let s = ExtractiveSummarizer;
        let out = s.summarize(text, 2);
        let sentences = split_sentences(&out);
        assert!(sentences.len() <= 2);
        // 结果应保留原文中相对重要的句子且按顺序出现。
        assert!(out.contains("喜欢 Rust"));
    }

    #[test]
    fn empty_safe() {
        let s = ExtractiveSummarizer;
        assert_eq!(s.summarize("", 3), "");
    }
}
