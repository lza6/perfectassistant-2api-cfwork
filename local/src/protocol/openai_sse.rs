//! OpenAI chat.completions 兼容输出：非流式 JSON + 伪流式 SSE
//!
//! 上游 `/ai/free` 一次性返回完整文本，本模块将其切块、按间隔逐帧下发，
//! 为客户端模拟打字机体验（帧结构符合 OpenAI SSE 规范）。

use crate::errors::ApiError;
use axum::body::Body;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures::StreamExt;
use std::convert::Infallible;

pub struct SseResponse {
    pub body: Body,
}

impl IntoResponse for SseResponse {
    fn into_response(self) -> Response {
        Response::builder()
            .header("content-type", "text/event-stream; charset=utf-8")
            .header("cache-control", "no-cache")
            .header("x-accel-buffering", "no")
            .body(self.body)
            .unwrap()
    }
}

fn frame(payload: &serde_json::Value) -> String {
    format!("data: {payload}\n\n")
}

/// 伪流式帧序列（含 role 帧、内容块、结束帧、[DONE]）
pub fn build_frames(
    content: &str,
    model: &str,
    id: &str,
    created: i64,
    chunk_chars: usize,
) -> Vec<String> {
    let mut frames = Vec::new();
    // 首帧：role
    frames.push(frame(&serde_json::json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{ "index": 0, "delta": { "role": "assistant" }, "finish_reason": null }]
    })));
    // 内容块
    for part in crate::protocol::char_chunks(content, chunk_chars) {
        if part.is_empty() {
            continue;
        }
        frames.push(frame(&serde_json::json!({
            "id": id,
            "object": "chat.completion.chunk",
            "created": created,
            "model": model,
            "choices": [{ "index": 0, "delta": { "content": part }, "finish_reason": null }]
        })));
    }
    // 结束帧
    frames.push(frame(&serde_json::json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{ "index": 0, "delta": {}, "finish_reason": "stop" }]
    })));
    frames.push("data: [DONE]\n\n".to_string());
    frames
}

/// 构造伪流式 SSE 响应体
pub fn openai_stream_body(
    content: String,
    model: String,
    id: String,
    created: i64,
    chunk_chars: usize,
    delay_ms: u64,
) -> Body {
    let frames = build_frames(&content, &model, &id, created, chunk_chars);
    let delay = crate::protocol::delay_of(delay_ms);
    let s = futures::stream::iter(frames).then(move |f| async move {
        tokio::time::sleep(delay).await;
        Ok::<Bytes, Infallible>(Bytes::from(f))
    });
    Body::from_stream(s)
}

/// 伪流式响应（错误也以 SSE 尾帧形式表达，避免连接中断）
pub fn openai_stream_error(content: &str, model: &str, id: &str, created: i64) -> String {
    format!(
        "{}{}",
        frame(&serde_json::json!({
            "id": id,
            "object": "chat.completion.chunk",
            "created": created,
            "model": model,
            "choices": [{ "index": 0, "delta": { "content": content }, "finish_reason": "stop" }]
        })),
        "data: [DONE]\n\n"
    )
}

/// 非流式 JSON 响应
pub fn openai_nonstream(
    content: &str,
    model: &str,
    id: &str,
    created: i64,
    prompt_tokens: u64,
    completion_tokens: u64,
) -> String {
    serde_json::json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": content },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens
        }
    })
    .to_string()
}

/// 便捷：把 ApiError 转成 SSE 错误体（流中不可回退 HTTP 状态码时用）
pub fn sse_error_from(err: &ApiError, model: &str, id: &str, created: i64) -> String {
    openai_stream_error(&format!("[上游错误: {}]", err.message()), model, id, created)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_contain_role_content_and_done() {
        let f = build_frames("hello world", "summarize", "chatcmpl-1", 1, 5);
        assert!(f[0].contains("\"role\":\"assistant\""));
        assert!(f.iter().any(|x| x.contains("hello")));
        assert!(f.last().unwrap().contains("[DONE]"));
    }

    #[test]
    fn char_chunks_utf8_safe() {
        let chunks = crate::protocol::char_chunks("你好世界", 2);
        assert_eq!(chunks, vec!["你好", "世界"]);
        let combined: String = chunks.concat();
        assert_eq!(combined, "你好世界");
    }
}
