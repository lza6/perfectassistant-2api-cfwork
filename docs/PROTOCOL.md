# PerfectAssistant 上游协议逆向笔记

> 数据来源：CDP 抓取 `perfectassistant.ai` 首页 + `iframe/brainstorm-tool`（`GetSourceCode` 工具，见 `captures/`），
> 关键证据文件 `source/perfectassistant.ai/assets/index.client-Dy3rh5Fm.js`，以及 2026-10-03 对 `POST /ai/free` 的实测。
> 更新：2026-10-03。

本文件是 CF Worker 版（`worker.js`）与本地 Rust 版（`local/`）**共同的**上游协议依据。任何上游变更应在此记录。

---

## 1. 端点

| 端点 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/ai/free` | POST | 匿名 | **免费端点**，本文档目标。无需 Cookie / 登录 / API Key |
| `/ai` | POST | 需登录账号 | 付费/登录版（`account` 存在时前端走此路径） |

前端判定（`index.client-Dy3rh5Fm.js`）：

```js
const N = applicationContext?.account;         // 是否已登录
fetch(N ? "/ai" : "/ai/free", { method: "POST", body: JSON.stringify(payload) }).json()
```

免费网关一律使用 `/ai/free`。

## 2. 请求

```http
POST /ai/free HTTP/1.1
Content-Type: text/plain;charset=UTF-8
Origin: https://perfectassistant.ai
Referer: https://perfectassistant.ai/iframe/<toolId>?lang=en
User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) ... Chrome/151.0.0.0 Safari/537.36
Accept: */*

{"tone":"professional","language":"en","text":"<用户输入>","id":"<toolId>","chatId":"<uuid>","host":"perfectassistant.ai","source":"iframe"}
```

字段（来自前端 `JSON.stringify({...K, text, id, chatId, host, source})`）：

| 字段 | 类型 | 说明 |
|------|------|------|
| `text` | string | 用户输入正文 |
| `id` | string | **工具 ID**（见第 4 节目录），决定提示词模板 |
| `chatId` | string | 会话 id，任意 UUID；免费端点每次新生成 |
| `host` | string | 固定 `perfectassistant.ai` |
| `source` | string | 固定 `iframe` |
| `tone` | string | 语气（表单字段，如 `professional`） |
| `language` | string | 语言，枚举：`en,fr,de,it,es,ru,id,ar,th,vi,tr,nl,pl,ko,hi`（**无中文**） |

> ⚠️ 常见错误：旧实现使用 `language:"chinese"` — 上游语言枚举不含中文，应使用 `en` 或其它合法值。

## 3. 响应

`Content-Type: application/json; charset=utf-8`，**非流式**，一次性返回：

```json
{
  "response": "最终文本",
  "responses": ["变体1", "变体2", "变体3"]
}
```

实测（2026-10-03，`id:"summarize"`）：

```json
{"response":"The cat sat on the mat.","responses":["The cat sat on the mat.","A cat sat on a mat.","The cat rested on the mat."]}
```

| 字段 | 说明 |
|------|------|
| `response` | 最终单条文本（首选） |
| `responses` | 多个变体数组；取 `responses[0]` 作为回退 |

**伪流式**：上游非流式，网关把 `response` 切块、按间隔逐帧下发为 SSE，模拟打字机效果。

**额度哨兵**：`responses[0]` 若包含字符串 `Sign up to continue using Perfect Assistant!`，表示免费额度用尽 / 需登录注册。前端据此弹注册框；网关应转译为 `429 rate_limit_error`（勿把哨兵当正文透传）。

## 4. 工具目录（62 个）

来源：`GET /__manifest?paths=...` 路由表实测提取。每组 `/tools/<category>/<id>` 即一个工具，`id` 即请求字段 `id`。

| 分类 | 数量 | 工具 ID |
|------|------|---------|
| business | 15 | brainstorm-tool, brand-name-generator, company-profile, interview-questions-generator, keywords-generator, market-research, marketing-campaign-ideas, meeting-minutes-generator, ocr-text, okr-generator, review-generator, sop-generator, startup-ideas, write-cover-letter, write-resume |
| content | 20 | blog-post-brief, blog-post-generator, blog-post-intro, continue-sentence, create-faq, cta-generator, fix-grammar, give-definition, headline-generator, humanize-text, improve, paraphrase, seo-title-and-description, shorter, simplify, speech-writer, summarize, title, translate, write-paragraph |
| social media | 7 | blog-post-to-tweet-thread, content-calendar, instagram-captions, social-media-post-ideas, tiktok-instagram-reel-script, tiktok-or-instagram-reel-ideas, tweet-ideas |
| mail | 6 | angry-customer-email, mail-improve-draft, mail-reply, mail-subject-line-creator, negative-reply-customer-email, positive-reply-customer-email |
| chat | 5 | apology, birthday, greeting, invitation, reply-chat |
| advertisement | 2 | facebook-ads, google-ads |
| slides | 2 | slides-outline, text-to-slide |
| spreadsheets | 2 | explain-excel-formula, generate-excel-formula |
| video | 2 | video-script-generator, youtube-titles-generator |
| advanced | 1 | query-chat-gpt |

> ⚠️ **已废弃的错误 ID**（曾出现在旧 `worker.js`，上游不存在）：
> `social-media-post`（应为 `social-media-post-ideas`）、`essay-writer`、`paragraph-writer`（应为 `write-paragraph`）、`email-writer`。
> 这些请求会被上游静默接受但返回与预期不符的模板结果，属正确性缺陷。

## 5. 反爬/风控

- 首页由 Next.js SSR 提供；静态资源经 Cloudflare（`/cdn-cgi/`）。
- `/ai/free` 实测匿名可达，未强制 Cloudflare 挑战。
- 免费层按 IP / 会话限流，用尽后返回额度哨兵（见第 3 节）。
- 建议：遵守上游服务条款，仅作个人学习/研究用途；高并发场景自行限速。

## 6. 复现抓取

```bash
# 使用 一键CDP获取网站源代码 工具
node test/run-capture.js "https://perfectassistant.ai/iframe/brainstorm-tool?lang=en" ./captures/iframe-brainstorm
```

关键产物：`source/perfectassistant.ai/assets/index.client-*.js`（含上游调用代码）、`network.har`（网络包）、`page.html`。
