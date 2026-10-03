# PerfectAssistant2API · 本地网关（Rust）

把 perfectassistant.ai 免费服务 (`/ai/free`) 转成 OpenAI/Anthropic 兼容的本地网关。
单二进制、零外部依赖，架构对齐同系列 `tokenharbor-2api` / `tryingopen-2api`。

## 编译运行

```bash
# Windows
build.bat

# 跨平台
cargo build --release
cp config.example.json config.json
./target/release/perfectassistant2api.exe --config config.json
```

默认监听 `http://127.0.0.1:47832`，控制面板 <http://127.0.0.1:47832/>。

## 模块

| 文件 | 职责 |
|------|------|
| `src/main.rs` | 入口：装配 state、启动 axum |
| `src/config.rs` | config.json + 环境变量 |
| `src/models.rs` | 62 个上游工具目录 |
| `src/upstream.rs` | 上游 `/ai/free` 客户端 + 契约解析 |
| `src/protocol/openai_sse.rs` | OpenAI 非流式 + 伪流式 SSE |
| `src/protocol/anthropic_sse.rs` | Anthropic 非流式 + 伪流式 SSE |
| `src/api.rs` | 路由、鉴权、CORS、请求/响应桥接 |
| `src/web.rs` | 内置控制面板 |
| `src/errors.rs` | OpenAI/Anthropic 兼容错误 |

## 测试

```bash
cargo test          # 27 项：单元 + 集成（mock 上游，不触网）
```

集成测试在 `tests/gateway.rs` 用本地 mock 上游覆盖：非流式/流式、双协议、
额度哨兵→429、空响应→502、鉴权、未知模型透传。

上游协议依据见 [`../docs/PROTOCOL.md`](../docs/PROTOCOL.md)。
