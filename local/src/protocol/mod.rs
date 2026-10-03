//! 协议转换层：上游非流式 JSON → OpenAI / Anthropic SSE（伪流式）

pub mod anthropic_sse;
pub mod openai_sse;

use std::time::Duration;

/// 把文本按字符切成固定大小的块（UTF-8 安全）
pub fn char_chunks(text: &str, size: usize) -> Vec<String> {
    let size = size.max(1);
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    chars
        .chunks(size)
        .map(|c| c.iter().collect::<String>())
        .collect()
}

/// 简单的 token 估算（无真实 tokenizer，按 ≈4 字符 1 token）
pub fn estimate_tokens(text: &str) -> u64 {
    ((text.chars().count() as u64) / 4).max(1)
}

pub fn delay_of(ms: u64) -> Duration {
    Duration::from_millis(ms)
}
