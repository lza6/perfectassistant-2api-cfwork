//! HTTP API：OpenAI/Anthropic 兼容端点 + PerfectAssistant 上游桥接 + 控制面板

use crate::config::Config;
use crate::errors::ApiError;
use crate::models::ModelRegistry;
use crate::protocol::anthropic_sse::{self, AnthropicSseResponse};
use crate::protocol::openai_sse::{self, SseResponse};
use crate::upstream::{AiRequest, UpstreamClient, DEFAULT_HOST, DEFAULT_SOURCE};
use crate::web;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub upstream: Arc<UpstreamClient>,
    pub registry: Arc<ModelRegistry>,
    pub api_keys: Arc<std::sync::RwLock<Vec<String>>>,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(handle_dashboard))
        .route("/ui", get(handle_dashboard))
        .route("/healthz", get(handle_healthz))
        .route("/v1/models", get(handle_v1_models))
        .route("/v1/chat/completions", post(handle_chat_completions))
        .route("/v1/messages", post(handle_messages))
        .route("/v1/messages/count_tokens", post(handle_count_tokens))
        .route("/api/guide", get(handle_guide))
        .route("/api/config/api-key", post(handle_config_api_key))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            cors_mw,
        ))
        .with_state(state)
}

// ---------- CORS ----------

async fn cors_mw(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let origins = state.cfg.cors_allow_origins.clone();
    let origin = request
        .headers()
        .get(axum::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|o| o.to_string());
    let allowed = origin
        .as_ref()
        .map(|o| origins.iter().any(|a| a == "*" || a == o))
        .unwrap_or(false);
    let mut response = if request.method() == axum::http::Method::OPTIONS {
        Response::builder()
            .status(204)
            .body(axum::body::Body::empty())
            .unwrap()
    } else {
        next.run(request).await
    };
    if allowed {
        if let Some(o) = origin {
            if let Ok(val) = axum::http::HeaderValue::from_str(&o) {
                response
                    .headers_mut()
                    .insert(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
            }
        }
        response.headers_mut().insert(
            axum::http::header::ACCESS_CONTROL_ALLOW_METHODS,
            axum::http::HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        response.headers_mut().insert(
            axum::http::header::ACCESS_CONTROL_ALLOW_HEADERS,
            axum::http::HeaderValue::from_static("Content-Type, Authorization, X-API-Key, anthropic-version"),
        );
    }
    response
}

// ---------- 认证 ----------

fn check_api_key(
    cfg: &Config,
    api_keys: &std::sync::RwLock<Vec<String>>,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let keys = api_keys.read().map(|g| g.clone()).unwrap_or_default();
    if keys.is_empty() && cfg.api_keys.is_empty() {
        return Ok(()); // 未配置则不校验（本地使用）
    }
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let bearer = auth.strip_prefix("Bearer ").unwrap_or("").trim();
    let x_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .trim();
    if keys.iter().any(|k| k == bearer || k == x_key)
        || cfg.api_keys.iter().any(|k| k == bearer || k == x_key)
    {
        return Ok(());
    }
    Err(ApiError::unauthorized(
        "无效的 API Key。请在面板生成 Key 或配置 config.json 的 api_keys",
    ))
}

// ---------- 路由处理 ----------

async fn handle_dashboard(State(state): State<AppState>) -> impl IntoResponse {
    let origin = state.upstream.origin().to_string();
    Html(web::dashboard_html(&state.cfg, &state.registry, &origin))
}

async fn handle_healthz(State(state): State<AppState>) -> Response {
    let body = Json(serde_json::json!({
        "status": "ok",
        "models": state.registry.len(),
        "upstream": state.upstream.origin(),
    }));
    body.into_response()
}

async fn handle_v1_models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(e) = check_api_key(&state.cfg, &state.api_keys, &headers) {
        return e.into_response();
    }
    let data: Vec<serde_json::Value> = state
        .registry
        .all()
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "object": "model",
                "created": 1735689600,
                "owned_by": "perfectassistant",
                "perfectassistant": { "category": m.category, "name": m.name }
            })
        })
        .collect();
    Json(serde_json::json!({ "object": "list", "data": data })).into_response()
}

async fn handle_guide(State(state): State<AppState>) -> Response {
    let keys = state.api_keys.read().map(|g| g.clone()).unwrap_or_default();
    Json(serde_json::json!({
        "upstream": state.upstream.origin(),
        "models": state.registry.len(),
        "default_model": state.cfg.default_model,
        "has_api_key": !keys.is_empty() || !state.cfg.api_keys.is_empty(),
        "openai_base_url": "/v1",
        "anthropic_base_url": "",
    }))
    .into_response()
}

#[derive(Deserialize)]
struct ApiKeyConfig {
    #[serde(default)]
    action: String,
    #[serde(default)]
    key: Option<String>,
}

async fn handle_config_api_key(
    State(state): State<AppState>,
    Json(body): Json<ApiKeyConfig>,
) -> Response {
    let mut keys = state.api_keys.write().unwrap();
    match body.action.as_str() {
        "set" => {
            if let Some(k) = body.key.filter(|k| !k.trim().is_empty()) {
                *keys = vec![k.trim().to_string()];
            }
        }
        "generate" => {
            let k = format!("sk-pa-{}", uuid::Uuid::new_v4().simple());
            *keys = vec![k];
        }
        "clear" => {
            *keys = vec![];
        }
        _ => {
            return ApiError::bad_request("action 必须是 set / generate / clear").into_response();
        }
    }
    Json(serde_json::json!({ "ok": true, "key": keys.first() })).into_response()
}

// ---------- OpenAI 兼容 ----------

#[derive(Debug, Deserialize)]
struct ChatMessage {
    // 上游仅消费合并后的纯文本，role 保留在契约中以文档化请求结构
    #[serde(default)]
    #[allow(dead_code)]
    role: String,
    #[serde(default)]
    content: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ChatRequest {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    messages: Vec<ChatMessage>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    tone: Option<String>,
    #[serde(default)]
    language: Option<String>,
}

async fn handle_chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChatRequest>,
) -> Response {
    if let Err(e) = check_api_key(&state.cfg, &state.api_keys, &headers) {
        return e.into_response();
    }
    if req.messages.is_empty() {
        return ApiError::bad_request("messages 不能为空").into_response();
    }
    let prompt = build_prompt(&req.messages, None);
    let model = resolve_model(&state, req.model.as_deref());
    let tone = req.tone.unwrap_or_else(|| state.cfg.default_tone.clone());
    let language = req.language.unwrap_or_else(|| state.cfg.default_language.clone());

    let created = chrono::Utc::now().timestamp();
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());

    match call_upstream(&state, &model, &prompt, &tone, &language).await {
        Ok(text) => {
            if req.stream {
                let body = openai_sse::openai_stream_body(
                    text,
                    model,
                    id,
                    created,
                    state.cfg.pseudo_chunk_chars,
                    state.cfg.pseudo_chunk_delay_ms,
                );
                SseResponse { body }.into_response()
            } else {
                let out = openai_sse::openai_nonstream(
                    &text,
                    &model,
                    &id,
                    created,
                    crate::protocol::estimate_tokens(&prompt),
                    crate::protocol::estimate_tokens(&text),
                );
                json_response(&out)
            }
        }
        Err(e) => {
            if req.stream {
                let body = openai_sse::sse_error_from(&e, &model, &id, created);
                sse_response(body)
            } else {
                e.into_response()
            }
        }
    }
}

// ---------- Anthropic 兼容 ----------

#[derive(Debug, Deserialize)]
struct MessagesRequest {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    messages: Vec<ChatMessage>,
    #[serde(default)]
    system: Option<serde_json::Value>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    tone: Option<String>,
    #[serde(default)]
    language: Option<String>,
}

async fn handle_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<MessagesRequest>,
) -> Response {
    if let Err(e) = check_api_key(&state.cfg, &state.api_keys, &headers) {
        return anthropic_error(e);
    }
    if req.messages.is_empty() {
        return anthropic_error(ApiError::bad_request("messages 不能为空"));
    }
    let prompt = build_prompt(&req.messages, req.system.as_ref());
    let model = resolve_model(&state, req.model.as_deref());
    let tone = req.tone.unwrap_or_else(|| state.cfg.default_tone.clone());
    let language = req.language.unwrap_or_else(|| state.cfg.default_language.clone());
    let id = format!("msg_{}", uuid::Uuid::new_v4().simple());
    let input_tokens = crate::protocol::estimate_tokens(&prompt);

    match call_upstream(&state, &model, &prompt, &tone, &language).await {
        Ok(text) => {
            if req.stream {
                let body = anthropic_sse::anthropic_stream_body(
                    text,
                    model,
                    id,
                    input_tokens,
                    state.cfg.pseudo_chunk_chars,
                    state.cfg.pseudo_chunk_delay_ms,
                );
                AnthropicSseResponse { body }.into_response()
            } else {
                let out = anthropic_sse::anthropic_nonstream(
                    &text,
                    &model,
                    &id,
                    input_tokens,
                    crate::protocol::estimate_tokens(&text),
                );
                json_response(&out)
            }
        }
        Err(e) => {
            if req.stream {
                let body = anthropic_sse::anthropic_stream_error(
                    &format!("[上游错误: {}]", e.message()),
                    &model,
                    &id,
                );
                sse_response(body)
            } else {
                anthropic_error(e)
            }
        }
    }
}

// ---------- Claude Code 兼容 ----------

/// Claude Code 会调用 /v1/messages/count_tokens 预统计。返回估算值。
async fn handle_count_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<MessagesRequest>,
) -> Response {
    if let Err(e) = check_api_key(&state.cfg, &state.api_keys, &headers) {
        return anthropic_error(e);
    }
    let prompt = build_prompt(&req.messages, req.system.as_ref());
    Json(serde_json::json!({ "input_tokens": crate::protocol::estimate_tokens(&prompt) })).into_response()
}

// ---------- 共享逻辑 ----------

/// 调用上游；命中注册/额度哨兵时返回明确的可读错误
async fn call_upstream(
    state: &AppState,
    model: &str,
    prompt: &str,
    tone: &str,
    language: &str,
) -> Result<String, ApiError> {
    let req = AiRequest {
        text: prompt.to_string(),
        id: model.to_string(),
        chat_id: Some(uuid::Uuid::new_v4().to_string()),
        host: Some(DEFAULT_HOST.to_string()),
        source: Some(DEFAULT_SOURCE.to_string()),
        tone: Some(tone.to_string()),
        language: Some(language.to_string()),
    };
    let resp = state
        .upstream
        .call_free(&req, model)
        .await
        .map_err(|e| ApiError::upstream(e.to_string()))?;
    if resp.is_quota_gated() {
        return Err(ApiError::rate_limited(
            "上游免费限流：约 60 次/小时/IP。请稍后再试（约 1 小时）或更换网络出口",
        ));
    }
    let text = resp.best_text();
    if text.is_empty() {
        return Err(ApiError::upstream("上游未返回有效内容"));
    }
    Ok(text)
}

/// 解析模型（工具）id：目录内直接用；未知时按配置透传或回退默认
fn resolve_model(state: &AppState, requested: Option<&str>) -> String {
    match requested {
        Some(m) if state.registry.contains(m) => m.to_string(),
        Some(m) if state.cfg.pass_through_unknown_models => m.to_string(),
        _ => state.cfg.default_model.clone(),
    }
}

/// 从 messages 抽取文本（支持 string 或 parts 数组），可选前置 system
fn build_prompt(messages: &[ChatMessage], system: Option<&serde_json::Value>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(s) = system {
        let t = extract_text(s);
        if !t.trim().is_empty() {
            parts.push(t);
        }
    }
    for m in messages {
        let t = extract_text(&m.content);
        if !t.trim().is_empty() {
            parts.push(t);
        }
    }
    parts.join("\n\n")
}

fn extract_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|item| match item {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Object(o) => o
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn json_response(body: &str) -> Response {
    Response::builder()
        .header("content-type", "application/json; charset=utf-8")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap()
}

fn sse_response(body: String) -> Response {
    Response::builder()
        .header("content-type", "text/event-stream; charset=utf-8")
        .header("cache-control", "no-cache")
        .body(axum::body::Body::from(body))
        .unwrap()
}

fn anthropic_error(e: ApiError) -> Response {
    let status = e.status();
    (status, e.anthropic_json()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: serde_json::Value) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content,
        }
    }

    #[test]
    fn prompt_from_string_messages() {
        let msgs = vec![
            msg("system", serde_json::json!("be brief")),
            msg("user", serde_json::json!("hello")),
        ];
        assert_eq!(build_prompt(&msgs, None), "be brief\n\nhello");
    }

    #[test]
    fn prompt_from_parts_array() {
        let msgs = vec![msg(
            "user",
            serde_json::json!([
                { "type": "text", "text": "part1" },
                { "type": "text", "text": "part2" }
            ]),
        )];
        assert_eq!(build_prompt(&msgs, None), "part1\npart2");
    }

    #[test]
    fn prompt_prepends_system() {
        let msgs = vec![msg("user", serde_json::json!("hi"))];
        let sys = serde_json::json!("SYS");
        assert_eq!(build_prompt(&msgs, Some(&sys)), "SYS\n\nhi");
    }

    #[test]
    fn extract_text_ignores_non_text() {
        assert_eq!(extract_text(&serde_json::json!(42)), "");
    }
}
