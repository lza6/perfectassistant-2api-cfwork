# AGENTS.md · perfectassistant-2api

> 本仓库是 `*-2api` 家族的**参考实现**，也是 [`2api-Template`](https://github.com/lza6/2api-Template) 的母本。
> 在此仓库工作时，先读本文件 + `docs/PROTOCOL.md`。

## 这是什么

把 [perfectassistant.ai](https://perfectassistant.ai) 的免费 AI 服务（`POST /ai/free`）转换为
**OpenAI + Anthropic 兼容 API** 的双形态网关：

| 形态 | 位置 | 运行时 |
|------|------|--------|
| Cloudflare Worker | `worker.js` | CF 边缘 |
| 本地网关 | `local/` | Rust 单二进制（axum） |

## 关键事实（上游契约）

- 端点 `POST /ai/free`，**匿名可用**，**非流式**
- 请求体 `{text, id, chatId, host, source, tone, language}`（字段名**大小写敏感**）
- 响应 `{response, responses[]}`
- **限流 60 次/小时/IP**，超限仍返回 **HTTP 200** + 文本哨兵
  `You've used your hourly request limit (60 requests). Sign up to continue using Perfect Assistant!`
  → **必须靠文本检测，不能靠状态码**
- 工具目录 62 个（来源上游 `__manifest`）

完整契约见 [`docs/PROTOCOL.md`](docs/PROTOCOL.md)，实测可用性见 [`docs/E2E.md`](docs/E2E.md)。

## 修改须知

- **改上游适配时**：CF 版改 `worker.js` 的 `CONFIG`/`CATALOG`/`callUpstream`；本地版改
  `local/src/{config,models,upstream,api}.rs`。两者共享 `docs/PROTOCOL.md`，改一处文档、两处代码同步。
- **改协议引擎时**：确保 CF 版与本地版行为一致（双协议、伪流式、错误映射）。
- **测试**：`npm test`（CF，27 项）与 `cd local && cargo test`（Rust，28 项）必须全过。
- 详细流程与硬规则见 `2api-Template/AGENTS.md`。

## 硬规则

- 模型 id / 字段名 / 枚举值必须来自实测或抓包原文（本仓库历史上曾编造 `social-media-post`、
  用非法 `language:"chinese"` —— 见 CHANGELOG）
- 限流靠文本检测
- 零硬编码密钥
- 声称"可用"必须有实测证据

## 相关

- 模板与方法论：[`2api-Template`](https://github.com/lza6/2api-Template)
- 同系列：TokenHarbor（Cookie 登录）、Tryingopen（代理池）、creen（图像/视频生成）
