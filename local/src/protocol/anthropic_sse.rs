//! Anthropic Messages API 兼容输出：非流式 JSON + 伪流式 SSE
//!
//! 供 Claude Code / 任意 Anthropic 客户端直接接入网关。
//! 事件序列严格遵循 Anthropic streaming 规范：
//! message_start → content_block_start → content_block_delta* → content_block_stop
//! → message_delta → message_stop

use axum::body::Body;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures::StreamExt;
use std::convert::Infallible;

pub struct AnthropicSseResponse {
    pub body: Body,
}

impl IntoResponse for AnthropicSseResponse {
    fn into_response(self) -> Response {
        Response::builder()
            .header("content-type", "text/event-stream; charset=utf-8")
            .header("cache-control", "no-cache")
            .header("x-accel-buffering", "no")
            .body(self.body)
            .unwrap()
    }
}

fn event(name: &str, payload: serde_json::Value) -> String {
    format!("event: {name}\ndata: {payload}\n\n")
}

/// 构造 Anthropic 伪流式帧序列
pub fn build_frames(
    content: &str,
    model: &str,
    id: &str,
    input_tokens: u64,
    chunk_chars: usize,
) -> Vec<String> {
    let mut frames = Vec::new();
    frames.push(event(
        "message_start",
        serde_json::json!({
            "type": "message_start",
            "message": {
                "id": id,
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": [],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": { "input_tokens": input_tokens, "output_tokens": 0 }
            }
        }),
    ));
    frames.push(event(
        "content_block_start",
        serde_json::json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": { "type": "text", "text": "" }
        }),
    ));
    for part in crate::protocol::char_chunks(content, chunk_chars) {
        if part.is_empty() {
            continue;
        }
        frames.push(event(
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": part }
            }),
        ));
    }
    frames.push(event(
        "content_block_stop",
        serde_json::json!({ "type": "content_block_stop", "index": 0 }),
    ));
    frames.push(event(
        "message_delta",
        serde_json::json!({
            "type": "message_delta",
            "delta": { "stop_reason": "end_turn", "stop_sequence": null },
            "usage": { "output_tokens": crate::protocol::estimate_tokens(content) }
        }),
    ));
    frames.push(event("message_stop", serde_json::json!({ "type": "message_stop" })));
    frames
}

pub fn anthropic_stream_body(
    content: String,
    model: String,
    id: String,
    input_tokens: u64,
    chunk_chars: usize,
    delay_ms: u64,
) -> Body {
    let frames = build_frames(&content, &model, &id, input_tokens, chunk_chars);
    let delay = crate::protocol::delay_of(delay_ms);
    let s = futures::stream::iter(frames).then(move |f| async move {
        tokio::time::sleep(delay).await;
        Ok::<Bytes, Infallible>(Bytes::from(f))
    });
    Body::from_stream(s)
}

/// 非流式 Anthropic 响应
pub fn anthropic_nonstream(
    content: &str,
    model: &str,
    id: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> String {
    serde_json::json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [{ "type": "text", "text": content }],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": { "input_tokens": input_tokens, "output_tokens": output_tokens }
    })
    .to_string()
}

/// 流中错误：以 content_block_delta 文本 + 正常收尾表达，避免客户端连接异常
pub fn anthropic_stream_error(content: &str, model: &str, id: &str) -> String {
    let frames = build_frames(content, model, id, 1, 200);
    // message_delta 的 stop_reason 保持 end_turn 即可
    frames.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_sequence_order() {
        let f = build_frames("hi", "summarize", "msg_1", 3, 10);
        assert!(f[0].contains("message_start"));
        assert!(f[1].contains("content_block_start"));
        assert!(f[2].contains("content_block_delta"));
        assert!(f.iter().any(|x| x.contains("content_block_stop")));
        assert!(f.iter().any(|x| x.contains("message_delta")));
        assert!(f.last().unwrap().contains("message_stop"));
    }

    #[test]
    fn nonstream_shape() {
        let s = anthropic_nonstream("hello", "summarize", "msg_1", 2, 4);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["type"], "message");
        assert_eq!(v["content"][0]["text"], "hello");
        assert_eq!(v["usage"]["input_tokens"], 2);
    }
}
