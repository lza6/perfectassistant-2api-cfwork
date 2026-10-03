//! 上游 PerfectAssistant HTTP 客户端
//!
//! 逆向自 `iframe/<toolId>` 的 `index.client-*.js`：
//! `fetch(account ? "/ai" : "/ai/free", { method:"POST", body: JSON.stringify({...form, text, id, chatId, host, source}) }).json()`
//! 响应体：`{ response: string, responses: string[] }`（免费端点匿名可用，非流式）。
//!
//! 证据：`/ai/free` 实测 200 + `{"response":"...","responses":["...","...","..."]}`。

use crate::config::Config;
use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, ORIGIN, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 上游表单默认值（tone / language 由配置提供，可被请求覆盖）
pub const DEFAULT_HOST: &str = "perfectassistant.ai";
pub const DEFAULT_SOURCE: &str = "iframe";

/// 触发注册/限额弹窗的哨兵（前端据此弹 Sign up）
pub const SIGNUP_SENTINEL: &str = "Sign up to continue using Perfect Assistant!";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub text: String,
    pub id: String,
    #[serde(rename = "chatId", default, skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// 上游响应（`response` 为最终文本，`responses` 为变体数组）
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AiResponse {
    #[serde(default)]
    pub response: Option<String>,
    #[serde(default)]
    pub responses: Option<Vec<String>>,
}

impl AiResponse {
    /// 取最终文本：优先 `response`，退回 `responses[0]`
    pub fn best_text(&self) -> String {
        if let Some(r) = &self.response {
            if !r.trim().is_empty() {
                return r.clone();
            }
        }
        if let Some(list) = &self.responses {
            for r in list {
                if !r.trim().is_empty() {
                    return r.clone();
                }
            }
        }
        String::new()
    }

    /// 是否命中注册/额度哨兵
    pub fn is_quota_gated(&self) -> bool {
        if let Some(r) = &self.response {
            if r.contains(SIGNUP_SENTINEL) {
                return true;
            }
        }
        if let Some(list) = &self.responses {
            if list.iter().any(|r| r.contains(SIGNUP_SENTINEL)) {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone)]
pub struct UpstreamClient {
    http: reqwest::Client,
    free_url: String,
    origin: String,
}

impl UpstreamClient {
    pub fn new(cfg: &Config) -> Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(cfg.request_timeout_sec))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent(DEFAULT_UA)
            .default_headers(default_headers(&cfg.upstream_origin()))
            .build()?;
        Ok(Self {
            http,
            free_url: cfg.upstream_free_url(),
            origin: cfg.upstream_origin(),
        })
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// 调用上游 `/ai/free`，返回解析后的 JSON
    pub async fn call_free(&self, req: &AiRequest, referer_id: &str) -> Result<AiResponse> {
        let referer = format!("{}/iframe/{}?lang=en", self.origin, referer_id);
        let mut headers = HeaderMap::new();
        if let Ok(v) = HeaderValue::from_str(&referer) {
            headers.insert(REFERER, v);
        }
        let resp = self
            .http
            .post(&self.free_url)
            .headers(headers)
            .json(req)
            .send()
            .await
            .context("上游 /ai/free 请求失败")?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!(
                "上游 HTTP {status}: {}",
                truncate(&text, 300)
            ));
        }
        let parsed: AiResponse = serde_json::from_str(&text)
            .with_context(|| format!("解析上游响应失败: {}", truncate(&text, 300)))?;
        Ok(parsed)
    }

    /// 健康检查：调用一个轻量工具（summarize），验证契约仍有效
    pub async fn check_health(&self) -> Result<()> {
        let req = AiRequest {
            text: "ping".into(),
            id: "summarize".into(),
            chat_id: Some(uuid::Uuid::new_v4().to_string()),
            host: Some(DEFAULT_HOST.into()),
            source: Some(DEFAULT_SOURCE.into()),
            tone: Some("professional".into()),
            language: Some("en".into()),
        };
        let r = self.call_free(&req, "summarize").await?;
        if r.best_text().is_empty() {
            return Err(anyhow!("上游健康检查未返回内容"));
        }
        Ok(())
    }
}

pub const DEFAULT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36";

fn default_headers(origin: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_UA));
    h.insert("accept", HeaderValue::from_static("*/*"));
    h.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain;charset=UTF-8"),
    );
    h.insert(
        ORIGIN,
        HeaderValue::from_str(origin).unwrap_or(HeaderValue::from_static("https://perfectassistant.ai")),
    );
    h
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{t}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_text_prefers_response() {
        let r = AiResponse {
            response: Some("hello".into()),
            responses: Some(vec!["a".into(), "b".into()]),
        };
        assert_eq!(r.best_text(), "hello");
    }

    #[test]
    fn best_text_falls_back_to_responses() {
        let r = AiResponse {
            response: None,
            responses: Some(vec!["".into(), "first non-empty".into()]),
        };
        assert_eq!(r.best_text(), "first non-empty");
    }

    #[test]
    fn quota_gate_detected() {
        let r = AiResponse {
            response: Some(format!("prefix {SIGNUP_SENTINEL} suffix")),
            responses: None,
        };
        assert!(r.is_quota_gated());
        let ok = AiResponse {
            response: Some("normal".into()),
            responses: None,
        };
        assert!(!ok.is_quota_gated());
    }

    #[test]
    fn ai_request_serializes_contract_fields() {
        let req = AiRequest {
            text: "hi".into(),
            id: "summarize".into(),
            chat_id: Some("c1".into()),
            host: Some(DEFAULT_HOST.into()),
            source: Some(DEFAULT_SOURCE.into()),
            tone: Some("professional".into()),
            language: Some("en".into()),
        };
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        assert_eq!(v["text"], "hi");
        assert_eq!(v["id"], "summarize");
        assert_eq!(v["chatId"], "c1");
        assert_eq!(v["host"], "perfectassistant.ai");
        assert_eq!(v["source"], "iframe");
    }
}
