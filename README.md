# PerfectAssistant-2API

把 [perfectassistant.ai](https://perfectassistant.ai) 的免费 AI 服务（`POST /ai/free`）转换为
**OpenAI 兼容**（`/v1/chat/completions`）与 **Anthropic 兼容**（`/v1/messages`）的 API。

提供两种部署形态，共享同一份上游协议（见 [`docs/PROTOCOL.md`](docs/PROTOCOL.md)）：

| 形态 | 目录 | 运行时 | 适用 |
|------|------|--------|------|
| **Cloudflare Worker** | 根目录 `worker.js` | Cloudflare 边缘 | 一键部署到公网，零运维 |
| **本地网关** | [`local/`](local/) | Rust 单二进制 | 本机 `127.0.0.1` 使用，零外部依赖 |

> 上游为非流式 JSON，两种形态都内置**伪流式**引擎，为客户端模拟打字机效果。
> 支持 **62 个上游工具**（business / content / social media / mail / chat / slides / spreadsheets / video / advertisement / advanced）。

---

## 一、Cloudflare Worker（公网）

### 一键部署

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/lza6/perfectassistant-2api-cfwork)

### 手动部署

```bash
npm install -g wrangler
wrangler login
wrangler deploy
```

**建议设置强密钥**（否则使用默认弱密钥 `1`，仅适合自用）：

```bash
wrangler secret put API_MASTER_KEY
```

### 使用

```python
from openai import OpenAI
client = OpenAI(base_url="https://<你的-worker>.workers.dev/v1", api_key="1")
resp = client.chat.completions.create(
    model="summarize",
    messages=[{"role": "user", "content": "总结：猫坐在垫子上。"}],
    stream=True,
)
for chunk in resp:
    print(chunk.choices[0].delta.content or "", end="")
```

---

## 二、本地网关（Rust，本机使用）

### 编译

```bash
# Windows
local\build.bat

# 或手动
cd local
cargo build --release
```

### 运行

```bash
cd local
copy config.example.json config.json
target\release\perfectassistant2api.exe --config config.json
```

默认监听 `http://127.0.0.1:47832`。控制面板：<http://127.0.0.1:47832/>

### 接入客户端

**OpenAI SDK / Cursor / Continue**

```
Base URL: http://127.0.0.1:47832/v1
API Key:  sk-local（未配置 api_keys 时随意填）
```

**Claude Code（Anthropic 协议）**

```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:47832
export ANTHROPIC_API_KEY=sk-local
claude
```

### 配置（`local/config.json`）

| 字段 | 默认 | 说明 |
|------|------|------|
| `listen_addr` | `127.0.0.1:47832` | 监听地址 |
| `upstream_base_url` | `https://perfectassistant.ai` | 上游 |
| `api_keys` | `[]` | 下游 Key；空 = 仅本机放行 |
| `default_model` | `brainstorm-tool` | 默认工具 |
| `default_tone` / `default_language` | `professional` / `en` | 上游表单默认值（语言枚举无中文） |
| `pseudo_chunk_chars` / `pseudo_chunk_delay_ms` | `120` / `8` | 伪流式分块 |
| `cors_allow_origins` | `[]` | 浏览器直连时配置；空 = 关闭 |

环境变量可覆盖：`LISTEN_ADDR`、`UPSTREAM_BASE_URL`、`API_KEYS`、`UI_PASSWORD`、`CORS_ALLOW_ORIGINS`。

---

## 三、API 端点

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/chat/completions` | POST | OpenAI 聊天（流式 / 非流式） |
| `/v1/messages` | POST | Anthropic 聊天（流式 / 非流式），供 Claude Code |
| `/v1/messages/count_tokens` | POST | token 预估（Claude Code 会调用） |
| `/v1/models` | GET | 62 个工具模型列表 |
| `/healthz` | GET | 健康检查 |
| `/api/guide` | GET | 接入信息（本地版） |
| `/api/config/api-key` | POST | 运行时生成 / 设置 / 清除 Key（本地版） |
| `/` · `/ui` | GET | 控制面板 |

**自定义参数**：请求体可带 `tone`（语气）、`language`（语言，枚举 `en,fr,de,it,es,ru,id,ar,th,vi,tr,nl,pl,ko,hi`）覆盖默认值。

---

## 四、工具目录（62 个）

`GET /v1/models` 返回全部。常用示例：

- `brainstorm-tool`（头脑风暴，默认）、`summarize`（摘要）、`translate`（翻译）、`write-paragraph`（段落写作）
- `blog-post-generator`、`social-media-post-ideas`、`okr-generator`、`instagram-captions` …

完整清单见 [`docs/PROTOCOL.md`](docs/PROTOCOL.md#4-工具目录62-个)。

> ⚠️ 请遵守上游服务条款，仅作个人学习 / 研究用途。
> **实测限流：60 次/小时/IP**（超限返回 429）；完整可用性评测见 [`docs/E2E.md`](docs/E2E.md)（62 个工具逐个真实验证）。

---

## 五、开发与测试

```bash
# CF Worker 冒烟测试（mock 上游，27 项）
npm test

# 全量模型真实 E2E（逐个打上游，约 8 分钟；会消耗 60/小时额度）
node scripts/e2e-all-models.mjs

# 本地 Rust 测试（28 项：单元 + 集成）
cd local && cargo test
```

逆向证据见 [`docs/PROTOCOL.md`](docs/PROTOCOL.md)，实测可用性见 [`docs/E2E.md`](docs/E2E.md)。

## 六、相关项目

本仓库属于 `*-2api` 系列，该系列把不同站点的免费服务统一为 OpenAI/Anthropic 兼容网关。
本项目是「**Cloudflare + 本地**双形态」的参考实现，其结构可沉淀为通用 2api 模板。

---

## 开源协议

[Apache-2.0](LICENSE)
