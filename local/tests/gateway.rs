//! 集成测试：本地 mock 上游 + 真实路由，端到端验证 OpenAI/Anthropic 端点与伪流式。
//!
//! 不触网：mock 上游返回固定 `{response, responses}`，验证契约解析、伪流式帧序、
//! 限额哨兵转 429、鉴权放行等关键路径。

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use http_body_util::BodyExt;
use perfectassistant2api::api::{build_router, AppState};
use perfectassistant2api::config::Config;
use perfectassistant2api::models::ModelRegistry;
use perfectassistant2api::upstream::UpstreamClient;
use std::sync::Arc;
use tower::ServiceExt;

/// 启动 mock 上游，`mode` 决定返回内容
async fn spawn_mock(mode: &'static str) -> String {
    let app = Router::new().route(
        "/ai/free",
        post(move || async move {
            let body = match mode {
                "quota" => serde_json::json!({
                    "response": "Sign up to continue using Perfect Assistant!",
                    "responses": ["Sign up to continue using Perfect Assistant!"]
                }),
                "empty" => serde_json::json!({ "response": "", "responses": [] }),
                _ => serde_json::json!({
                    "response": "hello world from mock",
                    "responses": ["hello world from mock", "hi from mock", "hey"]
                }),
            };
            axum::Json(body).into_response()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    format!("http://127.0.0.1:{}", addr.port())
}

async fn make_app(mode: &'static str) -> (Router, Arc<std::sync::RwLock<Vec<String>>>) {
    let upstream_url = spawn_mock(mode).await;
    let mut cfg = Config::default();
    cfg.upstream_base_url = upstream_url;
    cfg.skip_upstream_check = true;
    cfg.pseudo_chunk_delay_ms = 0; // 测试不加延迟
    cfg.default_model = "brainstorm-tool".into();

    let upstream = Arc::new(UpstreamClient::new(&cfg).unwrap());
    let registry = Arc::new(ModelRegistry::new(&cfg.default_model));
    let api_keys = Arc::new(std::sync::RwLock::new(Vec::<String>::new()));
    let state = AppState {
        cfg: Arc::new(cfg),
        upstream,
        registry,
        api_keys: api_keys.clone(),
    };
    (build_router(state), api_keys)
}

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).to_string()
}

fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn models_lists_62_tools() {
    let (app, _) = make_app("ok").await;
    let resp = app
        .oneshot(Request::builder().uri("/v1/models").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert_eq!(v["object"], "list");
    assert_eq!(v["data"].as_array().unwrap().len(), 62);
}

#[tokio::test]
async fn openai_nonstream_ok() {
    let (app, _) = make_app("ok").await;
    let req = post_json(
        "/v1/chat/completions",
        serde_json::json!({
            "model": "summarize",
            "messages": [{ "role": "user", "content": "hi" }]
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert_eq!(v["object"], "chat.completion");
    assert_eq!(v["choices"][0]["message"]["content"], "hello world from mock");
    assert_eq!(v["choices"][0]["finish_reason"], "stop");
    assert!(v["usage"]["total_tokens"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn openai_stream_frame_order() {
    let (app, _) = make_app("ok").await;
    let req = post_json(
        "/v1/chat/completions",
        serde_json::json!({
            "model": "summarize",
            "messages": [{ "role": "user", "content": "hi" }],
            "stream": true
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/event-stream; charset=utf-8"
    );
    let s = body_string(resp).await;
    assert!(s.starts_with("data: "), "首帧应为 SSE data");
    assert!(s.contains("\"role\":\"assistant\""));
    // 流式内容应拼回完整文本
    let rebuilt: String = s
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter(|d| *d != "[DONE]")
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(d).ok())
        .filter_map(|j| {
            j["choices"][0]["delta"]["content"]
                .as_str()
                .map(|s| s.to_string())
        })
        .collect();
    assert_eq!(rebuilt, "hello world from mock");
    assert!(s.trim_end().ends_with("[DONE]"));
}

#[tokio::test]
async fn anthropic_nonstream_ok() {
    let (app, _) = make_app("ok").await;
    let req = post_json(
        "/v1/messages",
        serde_json::json!({
            "model": "summarize",
            "max_tokens": 100,
            "messages": [{ "role": "user", "content": "hi" }]
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert_eq!(v["type"], "message");
    assert_eq!(v["content"][0]["text"], "hello world from mock");
    assert_eq!(v["stop_reason"], "end_turn");
}

#[tokio::test]
async fn anthropic_stream_event_sequence() {
    let (app, _) = make_app("ok").await;
    let req = post_json(
        "/v1/messages",
        serde_json::json!({
            "model": "summarize",
            "max_tokens": 100,
            "messages": [{ "role": "user", "content": "hi" }],
            "stream": true
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let s = body_string(resp).await;
    assert!(s.contains("event: message_start"));
    assert!(s.contains("event: content_block_delta"));
    assert!(s.contains("event: message_stop"));
}

#[tokio::test]
async fn quota_gate_becomes_429() {
    let (app, _) = make_app("quota").await;
    let req = post_json(
        "/v1/chat/completions",
        serde_json::json!({
            "model": "summarize",
            "messages": [{ "role": "user", "content": "hi" }]
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    let v: serde_json::Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert_eq!(v["error"]["type"], "rate_limit_error");
}

#[tokio::test]
async fn empty_upstream_becomes_502() {
    let (app, _) = make_app("empty").await;
    let req = post_json(
        "/v1/chat/completions",
        serde_json::json!({
            "model": "summarize",
            "messages": [{ "role": "user", "content": "hi" }]
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn empty_messages_rejected() {
    let (app, _) = make_app("ok").await;
    let req = post_json("/v1/chat/completions", serde_json::json!({ "messages": [] }));
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_model_passes_through_by_default() {
    let (app, _) = make_app("ok").await;
    let req = post_json(
        "/v1/chat/completions",
        serde_json::json!({
            "model": "totally-new-upstream-tool",
            "messages": [{ "role": "user", "content": "hi" }]
        }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert_eq!(v["model"], "totally-new-upstream-tool");
}

#[tokio::test]
async fn api_key_enforced_when_configured() {
    let (app, keys) = make_app("ok").await;
    *keys.write().unwrap() = vec!["sk-test".into()];

    // 无 key → 401
    let req = post_json(
        "/v1/chat/completions",
        serde_json::json!({ "model": "summarize", "messages": [{ "role": "user", "content": "hi" }] }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 带 key → 200
    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-test")
        .body(Body::from(
            serde_json::json!({ "model": "summarize", "messages": [{ "role": "user", "content": "hi" }] })
                .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn count_tokens_ok() {
    let (app, _) = make_app("ok").await;
    let req = post_json(
        "/v1/messages/count_tokens",
        serde_json::json!({ "model": "summarize", "messages": [{ "role": "user", "content": "hello there count my tokens" }] }),
    );
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert!(v["input_tokens"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn healthz_ok() {
    let (app, _) = make_app("ok").await;
    let resp = app
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["models"], 62);
}
