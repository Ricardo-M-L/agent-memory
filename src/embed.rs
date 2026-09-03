//! 嵌入抽象与内置离线哈希嵌入器。
//!
//! 设计参考：主流 AI 记忆项目（Mem0 / 向量 RAG）都依赖 embedding 做语义检索。
//! 为了做到「开箱即用、零外部服务、零 API key」，本项目内置一个基于
//! **特征哈希（feature hashing）** 的本地嵌入器 [`HashEmbedder`]：
//! 对文本做多语言分词（英文按词、中文按字符二元组），FNV-1a 哈希投影到固定维度并
//! L2 归一化。它不依赖任何模型权重或网络，质量足以支撑近义检索。
//!
//! 高级用户可实现 [`Embedder`] trait 换成真实语义模型（如 OpenAI / BGE / 本地 ONNX 等），
//! 替换后检索的向量混合部分会直接受益。

use crate::text::tokenize;

/// 嵌入器抽象：任何把文本变成固定维度向量的实现。
pub trait Embedder: Send + Sync {
    /// 返回文本的（已归一化）向量。
    fn embed(&self, text: &str) -> Vec<f32>;
    /// 向量维度。
    fn dim(&self) -> usize;
}

/// 内置离线特征哈希嵌入器。
///
/// 默认维度 512。维度越大区分度越高、内存占用越大。
pub struct HashEmbedder {
    dim: usize,
    seed: u64,
}

impl HashEmbedder {
    pub fn new(dim: usize) -> Self {
        Self {
            dim: dim.max(16),
            seed: 0x9E37_79B9_7F4A_7C15,
        }
    }
}

impl Embedder for HashEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; self.dim];
        let tokens = tokenize(text);

        for t in &tokens {
            let idx = fnv1a_index(t, self.seed, self.dim);
            vec[idx] += 1.0;
        }
        // 词对（bigram）也参与，增强短语相关性。
        for pair in tokens.windows(2) {
            let joined = format!("{} {}", pair[0], pair[1]);
            let idx = fnv1a_index(&joined, self.seed ^ 0x00AB_CDEF, self.dim);
            vec[idx] += 0.5;
        }

        l2_normalize(&mut vec);
        vec
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

/// FNV-1a 哈希后取模得到桶索引。
fn fnv1a_index(s: &str, seed: u64, dim: usize) -> usize {
    let mut h = seed;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1_099_511_627_821); // FNV-1a prime = 0x100000001B3
    }
    (h as usize) % dim
}

/// 就地 L2 归一化。
pub fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-8 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// 余弦相似度（要求两个等长向量；归一化向量点积即余弦）。
///
/// 返回值被 clamp 到 [0,1]，用于相关度与去重阈值比较。
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    dot.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similar_sentences_higher_than_dissimilar() {
        let e = HashEmbedder::new(512);
        let a = e.embed("用户喜欢用 Rust 写 Agent");
        let b = e.embed("用户喜欢用 Rust 写 Agent 应用");
        let c = e.embed("今天天气很好，适合跑步");
        assert!(cosine(&a, &b) > cosine(&a, &c), "near should beat far");
    }

    #[test]
    fn english_similarity() {
        let e = HashEmbedder::new(512);
        let a = e.embed("the cat sits on the mat");
        let b = e.embed("a cat is sitting on the mat");
        let c = e.embed("quantum physics equations");
        assert!(cosine(&a, &b) > cosine(&a, &c));
    }

    #[test]
    fn deterministic() {
        let e = HashEmbedder::new(256);
        assert_eq!(e.embed("hello world"), e.embed("hello world"));
        assert_eq!(e.dim(), 256);
    }

    #[test]
    fn normalization() {
        let e = HashEmbedder::new(128);
        let v = e.embed("归一化测试 normalization test");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3);
    }
}
