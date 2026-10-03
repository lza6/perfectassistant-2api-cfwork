# 更新日志

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 与[语义化版本](https://semver.org/lang/zh-CN/)。

## [2.0.0] — 2026-10-03

双形态重构：Cloudflare Worker + 本地 Rust 网关，共享同一上游协议。
本版本修复了 1.x 的正确性缺陷，并把工具目录从 6 个扩展到上游真实的 62 个。

### 修复（1.x 的正确性缺陷）

- **模型 id 编造**：旧版 `MODELS` 中的 `social-media-post`、`essay-writer`、`paragraph-writer`、
  `email-writer` 在上游不存在。修正为真实 id（`social-media-post-ideas`、`write-paragraph` 等）。
  上游对未知 id 会静默返回默认模板结果，不会报错——因此这类错误**难以被发现**。
- **非法语言值**：旧版硬编码 `language:"chinese"`，但上游语言枚举为
  `en,fr,de,it,es,ru,id,ar,th,vi,tr,nl,pl,ko,hi`（**无中文**）。
- **请求体缺字段**：旧版未发送上游契约要求的 `host` 与 `source` 字段。

### 新增

- **本地 Rust 网关**（`local/`）：单二进制、零外部依赖，架构对齐同系列 `TokenHarbor-2api`。
- **Anthropic 兼容端点** `POST /v1/messages`（流式 + 非流式），供 Claude Code 直连。
- **`POST /v1/messages/count_tokens`**：Claude Code 会调用的 token 预估端点。
- **工具目录 6 → 62**：采集自上游 `__manifest` 路由表，覆盖
  business / content / social media / mail / chat / advertisement / slides / spreadsheets / video / advanced。
- **伪流式引擎**：上游非流式，网关切块 + 定时下发 SSE，模拟打字机。
- **契约归档** `docs/PROTOCOL.md`：上游协议逆向笔记（附抓包与实测证据）。
- **实测报告** `docs/E2E.md`：62 工具逐个真实 E2E 的结论。

### 变更

- 限流识别：上游限流返回 **HTTP 200** + 文本哨兵（非 429），据此改为文本检测，
  并区分「每小时限流」与「需登录」两种文案。
- 控制面板重写，展示 62 工具目录与双协议接入信息。

### 测试

- CF Worker 冒烟：27 项
- 本地 Rust：28 项（单元 + 集成，mock 上游）
- 62 工具真实 E2E：60 可用 / 2 限流 / 0 失效

---

## [1.0.0] — 2025-11-23

首个版本：单文件 Cloudflare Worker，把 perfectassistant.ai 的 `/ai/free` 转为 OpenAI 兼容 API，
内置伪流式与控制面板。仅暴露 6 个模型（其中 3 个 id 有误，见 2.0.0 修复记录）。
