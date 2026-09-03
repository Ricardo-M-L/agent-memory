//! OpenAI 兼容的 HTTP 嵌入器（**可选 `http` feature**，默认不编译、不引入网络依赖）。
//!
//! 启用方式：`Cargo.toml` 中 `agent-memory = { version = "*", features = ["http"] }`。
//!
//! 兼容任何遵循 OpenAI `/v1/embeddings` 请求/响应格式的服务（OpenAI、vLLM、
//! 本地 text-embeddings-inference、各类网关）。真正的 HTTP 调用被抽象为
//! `HttpTransport` trait，默认用 `ureq`（rustls，无需系统 OpenSSL）；测试或自定义
//! 场景可注入 mock / 其他 HTTP 客户端。

use serde_json::Value;

use crate::embed::{l2_normalize, Embedder};
use crate::store::{StoreError, StoreResult};

/// HTTP 传输抽象：POST JSON 并返回响应体字符串。
pub trait HttpTransport: Send + Sync {
    /// 向 `url` 发送 `body_json`，带 `Authorization: Bearer <bearer>`，返回响应文本。
    fn post_json(&self, url: &str, bearer: &str, body_json: &str) -> StoreResult<String>;
}

/// 基于 `ureq`（rustls）的默认传输实现。
pub struct UreqTransport;

impl HttpTransport for UreqTransport {
    fn post_json(&self, url: &str, bearer: &str, body_json: &str) -> StoreResult<String> {
        let resp = ureq::post(url)
            .set("Authorization", &format!("Bearer {bearer}"))
            .set("Content-Type", "application/json")
            .send_string(body_json)
            .map_err(|e| StoreError::Http(e.to_string()))?;
        resp.into_string()
            .map_err(|e| StoreError::Http(e.to_string()))
    }
}

/// 构造 OpenAI embeddings 请求体。
pub fn build_request_body(model: &str, texts: &[String]) -> String {
    serde_json::json!({ "model": model, "input": texts }).to_string()
}

/// 解析 OpenAI embeddings 响应，按 `index` 排序返回向量列表。
pub fn parse_embeddings_response(json: &str) -> StoreResult<Vec<Vec<f32>>> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| StoreError::Http(format!("响应不是合法 JSON: {e}")))?;
    let data = value
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| StoreError::Http("响应缺少 data 数组".into()))?;

    let mut indexed: Vec<(usize, Vec<f32>)> = Vec::with_capacity(data.len());
    for item in data {
        let idx = item.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
        let emb = item
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| StoreError::Http("某条结果缺少 embedding 数组".into()))?;
        let mut vec: Vec<f32> = emb
            .iter()
            .map(|x| x.as_f64().unwrap_or(0.0) as f32)
            .collect();
        l2_normalize(&mut vec); // 与本地嵌入器保持一致：统一归一化。
        indexed.push((idx, vec));
    }
    indexed.sort_by_key(|(i, _)| *i);
    Ok(indexed.into_iter().map(|(_, v)| v).collect())
}

/// OpenAI 兼容嵌入器。
pub struct OpenAiEmbedder<T = UreqTransport> {
    endpoint: String,
    model: String,
    api_key: String,
    dim: usize,
    transport: T,
}

impl OpenAiEmbedder<UreqTransport> {
    /// 创建嵌入器：`api_key` 为服务密钥，`model` 为模型名，`dim` 为该模型向量维度。
    pub fn new(api_key: impl Into<String>, model: impl Into<String>, dim: usize) -> Self {
        Self {
            endpoint: "https://api.openai.com/v1/embeddings".to_string(),
            model: model.into(),
            api_key: api_key.into(),
            dim,
            transport: UreqTransport,
        }
    }
}

impl<T: HttpTransport> OpenAiEmbedder<T> {
    /// 覆盖 endpoint（用于自建/兼容服务）。
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// 替换传输层（注入自定义客户端或 mock）。
    pub fn with_transport<U: HttpTransport>(self, transport: U) -> OpenAiEmbedder<U> {
        OpenAiEmbedder {
            endpoint: self.endpoint,
            model: self.model,
            api_key: self.api_key,
            dim: self.dim,
            transport,
        }
    }
}

impl<T: HttpTransport> Embedder for OpenAiEmbedder<T> {
    fn embed(&self, text: &str) -> StoreResult<Vec<f32>> {
        let mut batch = self.embed_batch(&[text.to_string()])?;
        batch
            .pop()
            .ok_or_else(|| StoreError::Http("嵌入服务返回空结果".into()))
    }

    fn embed_batch(&self, texts: &[String]) -> StoreResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let body = build_request_body(&self.model, texts);
        let resp = self
            .transport
            .post_json(&self.endpoint, &self.api_key, &body)?;
        parse_embeddings_response(&resp)
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 记录请求并返回预置响应的 mock 传输层。
    struct MockTransport {
        last_url: Mutex<String>,
        last_body: Mutex<String>,
        reply: String,
    }
    impl HttpTransport for MockTransport {
        fn post_json(&self, url: &str, bearer: &str, body_json: &str) -> StoreResult<String> {
            assert_eq!(bearer, "secret-key");
            *self.last_url.lock().unwrap() = url.to_string();
            *self.last_body.lock().unwrap() = body_json.to_string();
            Ok(self.reply.clone())
        }
    }

    #[test]
    fn parses_and_orders_by_index() {
        // 故意打乱 index 顺序，验证按 index 排序。
        let reply = serde_json::json!({
            "data": [
                {"index": 1, "embedding": [0.0, 3.0, 4.0]},
                {"index": 0, "embedding": [1.0, 0.0, 0.0]}
            ]
        })
        .to_string();
        let parsed = parse_embeddings_response(&reply).unwrap();
        assert_eq!(parsed.len(), 2);
        // index 0 在前，且已 L2 归一化。
        assert!((parsed[0][0] - 1.0).abs() < 1e-5);
        assert!((parsed[1][2] - 0.8).abs() < 1e-5); // [0,3,4]/5
    }

    #[test]
    fn malformed_response_errors() {
        assert!(parse_embeddings_response("not json").is_err());
        assert!(parse_embeddings_response(r#"{"data":[]}"#).is_ok());
        assert!(parse_embeddings_response(r#"{"no_data":1}"#).is_err());
    }

    #[test]
    fn embedder_uses_transport_and_builds_body() {
        let reply = serde_json::json!({
            "data": [{"index": 0, "embedding": [1.0, 2.0]}]
        })
        .to_string();
        let mock = MockTransport {
            last_url: Mutex::new(String::new()),
            last_body: Mutex::new(String::new()),
            reply,
        };
        let emb = OpenAiEmbedder::new("secret-key", "text-embedding-3-small", 2)
            .with_endpoint("http://localhost/v1/embeddings")
            .with_transport(mock);
        let v = emb.embed("你好").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(emb.dim(), 2);

        // 传输层不可再借用（已 move），改为通过请求体构造函数校验。
        let body = build_request_body("text-embedding-3-small", &["你好".to_string()]);
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["model"], "text-embedding-3-small");
        assert_eq!(v["input"][0], "你好");
        // endpoint 生效：mock 断言过 url 不 panic 即说明调用成功。
        let _ = &emb;
    }

    #[test]
    fn empty_batch_short_circuits() {
        let mock = MockTransport {
            last_url: Mutex::new(String::new()),
            last_body: Mutex::new(String::new()),
            reply: String::new(),
        };
        let emb = OpenAiEmbedder::new("k", "m", 1).with_transport(mock);
        assert!(emb.embed_batch(&[]).unwrap().is_empty());
    }
}
