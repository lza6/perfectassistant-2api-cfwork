# 上游实测评测报告（62 工具逐个 E2E）

> 实测时间：2026-10-03
> 方法：对 `POST https://perfectassistant.ai/ai/free` 逐个工具发真实请求（每次间隔 900ms）
> 复现：`node scripts/e2e-all-models.mjs`（见 `../scripts/`）

## 结论

| 指标 | 结果 |
|------|------|
| **目录中工具总数** | 62 |
| **本轮实测可用** | **60** |
| **命中限流（非故障）** | 2（`youtube-titles-generator`、`query-chat-gpt`） |
| **真正失败（坏 id / 上游错误）** | **0** |

> 那 2 个「不可用」是**撞到每小时限流**（60 次/小时/IP），不是 id 失效 ——
> 它们排在最后，前面 60 次已耗尽额度。目录来自上游 `__manifest`（权威），id 有效。

## 关键发现：限流是「200 + 哨兵文本」

超限时上游**仍返回 HTTP 200**，响应体：

```json
{"responses":["You've used your hourly request limit (60 requests). Sign up to continue using Perfect Assistant!","...","..."]}
```

要点：
- **HTTP 状态码是 200**，不是 429 —— 靠状态码判断限流会漏
- 响应只有 `responses[]`，**没有** `response` 字段
- 哨兵文本：`Sign up to continue using Perfect Assistant!`（限流版前缀 `You've used your hourly request limit (60 requests).`）

**网关必须**：检测该哨兵 → 转译为 `429 rate_limit_error`，**不能**把哨兵当正文透传给用户。
本仓库 CF 版（`worker.js` 的 `QUOTA_SENTINEL`）与本地版（`local/src/upstream.rs` 的 `QUOTA_SENTINEL`）均已实现。

## 延迟

| 工具 | 耗时 | 说明 |
|------|------|------|
| 绝大多数 | 0.7 – 3.7s | 正常 |
| `write-resume` | 3.7s | 偏慢 |
| **`ocr-text`** | **19.8s** | 显著慢（OCR 链路），超时配置需 ≥ 30s |

→ 默认 `request_timeout_sec: 120` 足够；若收紧，最低不宜低于 30s。

## 逐工具结果

| 工具 | 状态 | 耗时 | 返回片段 |
|------|------|------|----------|
| brainstorm-tool | ✅ | 2420ms | Please provide the actual problem statement… |
| brand-name-generator | ✅ | 1570ms | Sure—here are brand name ideas… |
| company-profile | ✅ | 1204ms | Please provide your company's name… |
| interview-questions-generator | ✅ | 1203ms | What key moment shaped your perspective… |
| keywords-generator | ✅ | 977ms | Sure—please share your topic or niche… |
| market-research | ✅ | 1003ms | Please provide the specific industry name… |
| marketing-campaign-ideas | ✅ | 1179ms | Please paste your business definition… |
| meeting-minutes-generator | ✅ | 1171ms | Sure—please provide the full meeting notes… |
| ocr-text | ✅ | 19865ms | No text found |
| okr-generator | ✅ | 1401ms | I can't generate OKRs without the missing goal… |
| review-generator | ✅ | 928ms | Please share the service or product you used… |
| sop-generator | ✅ | 1004ms | Follow the provided brief to write a professional SOP… |
| startup-ideas | ✅ | 1727ms | Cloud-based AI for home energy "peak-load shifting"… |
| write-cover-letter | ✅ | 1479ms | Of course—I can do that. Please paste the full job listing… |
| write-resume | ✅ | 3668ms | Please paste the person's details… |
| blog-post-brief | ✅ | 923ms | Please paste the blog post definition in brackets… |
| blog-post-generator | ✅ | 969ms | Sure—please paste the brief blog post description… |
| blog-post-intro | ✅ | 971ms | Sure—please share the blog post title… |
| continue-sentence | ✅ | 1248ms | Sure—please paste the text you provided… |
| create-faq | ✅ | 928ms | Sure—please paste the content from your brackets… |
| cta-generator | ✅ | 915ms | (echo) |
| fix-grammar | ✅ | 848ms | (echo) |
| give-definition | ✅ | 943ms | Please provide the word or sentence… |
| headline-generator | ✅ | 821ms | Here are professional, eye-catching headline options… |
| humanize-text | ✅ | 879ms | Sure—here's a short, useful single sentence… |
| improve | ✅ | 950ms | Please provide the paragraph text you want improved… |
| paraphrase | ✅ | 843ms | Please paste the paragraph inside <>… |
| seo-title-and-description | ✅ | 1031ms | Choose the most specific, value-driven sentence… |
| shorter | ✅ | 1102ms | I can help—just send the text you want shortened… |
| simplify | ✅ | 749ms | Please write a clear, simple sentence. |
| speech-writer | ✅ | 1031ms | Resist the easy choice today… |
| summarize | ✅ | 853ms | Please provide the paragraph you'd like summarized… |
| title | ✅ | 969ms | Ensure the response is concise and actionable. |
| translate | ✅ | 947ms | (echo) |
| write-paragraph | ✅ | 884ms | Could you please provide the exact topic text… |
| blog-post-to-tweet-thread | ✅ | 913ms | Sure—please paste the content in the brackets… |
| content-calendar | ✅ | 900ms | Sure—please paste your description… |
| instagram-captions | ✅ | 1037ms | Please share the post description in brackets… |
| social-media-post-ideas | ✅ | 853ms | Please paste your description from inside the brackets… |
| tiktok-instagram-reel-script | ✅ | 1016ms | Please paste the content you want in brackets… |
| tiktok-or-instagram-reel-ideas | ✅ | 865ms | Please paste the content you mentioned in brackets… |
| tweet-ideas | ✅ | 908ms | Please paste the content from your bracketed section… |
| angry-customer-email | ✅ | 1188ms | Thank you for your feedback—we're sorry for the inconvenience… |
| mail-improve-draft | ✅ | 1632ms | Please provide the draft text you mentioned in <>… |
| mail-reply | ✅ | 923ms | Thank you for your message; I will review it… |
| mail-subject-line-creator | ✅ | 902ms | Please paste the email description… |
| negative-reply-customer-email | ✅ | 1031ms | I don't agree with your assessment… |
| positive-reply-customer-email | ✅ | 862ms | Thank you for sharing this information… |
| apology | ✅ | 909ms | I apologize for the mistake I made… |
| birthday | ✅ | 990ms | Happy Birthday—wishing you continued success… |
| greeting | ✅ | 1009ms | Welcome to the team—we're glad to have you onboard… |
| invitation | ✅ | 1219ms | Please paste the event details… |
| reply-chat | ✅ | 1000ms | Sure—please paste the message inside <>… |
| facebook-ads | ✅ | 890ms | Sure—please paste the ad description… |
| google-ads | ✅ | 939ms | Please paste your ad description… |
| slides-outline | ✅ | 1269ms | Please provide the topic definition… |
| text-to-slide | ✅ | 885ms | ### Slide Title: Quick Insight **Key Point:** … |
| explain-excel-formula | ✅ | 935ms | Please paste the Excel formula… |
| generate-excel-formula | ✅ | 2143ms | =IF(A1<>"","Your formula here","") |
| video-script-generator | ✅ | 1831ms | Please paste the brief description in brackets… |
| youtube-titles-generator | ⏳限流 | 357ms | You've used your hourly request limit (60 requests)… |
| query-chat-gpt | ⏳限流 | 278ms | You've used your hourly request limit (60 requests)… |

## 复现注意

- 全量跑一遍会耗尽 60 次/小时额度，之后 1 小时内该 IP 全部限流。
- 复现时：`node scripts/e2e-all-models.mjs`（约 8-9 分钟，含间隔）。
- 若中途开始限流，等 1 小时后再测剩余项；或更换出口 IP。
