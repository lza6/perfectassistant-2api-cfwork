// Cloudflare Worker 本地冒烟测试：mock 上游 fetch，验证路由与协议转换。
// 用法: node scripts/worker-smoke.mjs
import worker, { CATALOG } from "../worker.js";

let pass = 0,
  fail = 0;
const ok = (name, cond, extra = "") => {
  if (cond) {
    pass++;
    console.log(`  ✓ ${name}`);
  } else {
    fail++;
    console.log(`  ✗ ${name} ${extra}`);
  }
};

// 捕获上游请求
let lastUpstream = null;
const realFetch = globalThis.fetch;
globalThis.fetch = async (url, opts) => {
  if (String(url).includes("/ai/free")) {
    lastUpstream = { url: String(url), body: JSON.parse(opts.body), opts };
    const mode = globalThis.__MODE || "ok";
    if (mode === "quota") {
      return new Response(
        JSON.stringify({ response: "Sign up to continue using Perfect Assistant!", responses: ["Sign up to continue using Perfect Assistant!"] }),
        { status: 200, headers: { "content-type": "application/json" } }
      );
    }
    if (mode === "empty") {
      return new Response(JSON.stringify({ response: "", responses: [] }), { status: 200 });
    }
    return new Response(
      JSON.stringify({ response: "hello world from mock", responses: ["hello world from mock", "b", "c"] }),
      { status: 200, headers: { "content-type": "application/json" } }
    );
  }
  return realFetch(url, opts);
};

const env = { API_MASTER_KEY: "1" };
const call = (path, init) => worker.fetch(new Request("https://gw.example" + path, init), env, {});

async function body(resp) {
  return await resp.text();
}

console.log("== 目录 ==");
ok("62 个工具", Object.values(CATALOG).flat().length === 62, `实际 ${Object.values(CATALOG).flat().length}`);

console.log("== /healthz ==");
{
  const r = await call("/healthz");
  const j = JSON.parse(await body(r));
  ok("状态 ok", j.status === "ok");
  ok("模型数 62", j.models === 62, JSON.stringify(j));
}

console.log("== /v1/models ==");
{
  const r = await call("/v1/models", { headers: { Authorization: "Bearer 1" } });
  const j = JSON.parse(await body(r));
  ok("返回 62 个", j.data.length === 62);
  ok("含 summarize", j.data.some((m) => m.id === "summarize"));
}

console.log("== OpenAI 非流式 ==");
{
  globalThis.__MODE = "ok";
  const r = await call("/v1/chat/completions", {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: "Bearer 1" },
    body: JSON.stringify({ model: "summarize", messages: [{ role: "user", content: "hi" }] }),
  });
  const j = JSON.parse(await body(r));
  ok("HTTP 200", r.status === 200);
  ok("content 正确", j.choices[0].message.content === "hello world from mock");
  ok("finish_reason stop", j.choices[0].finish_reason === "stop");
  ok("上游 payload 含 chatId/host/source",
    lastUpstream.body.chatId && lastUpstream.body.host === "perfectassistant.ai" && lastUpstream.body.source === "iframe");
  ok("上游 payload language 合法(非中文)", lastUpstream.body.language === "en");
}

console.log("== OpenAI 流式 ==");
{
  const r = await call("/v1/chat/completions", {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: "Bearer 1" },
    body: JSON.stringify({ model: "translate", messages: [{ role: "user", content: "hi" }], stream: true }),
  });
  const s = await body(r);
  ok("content-type 是 SSE", r.headers.get("content-type").includes("text/event-stream"));
  ok("含 role 帧", s.includes('"role":"assistant"'));
  ok("末尾 [DONE]", s.trimEnd().endsWith("[DONE]"));
  const rebuilt = s.split("\n").filter((l) => l.startsWith("data: ") && l.slice(6) !== "[DONE]")
    .map((l) => { try { return JSON.parse(l.slice(6)).choices[0].delta.content || ""; } catch { return ""; } }).join("");
  ok("流式内容可拼回", rebuilt === "hello world from mock", JSON.stringify(rebuilt));
}

console.log("== Anthropic 非流式 ==");
{
  globalThis.__MODE = "ok";
  const r = await call("/v1/messages", {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: "Bearer 1" },
    body: JSON.stringify({ model: "summarize", max_tokens: 100, messages: [{ role: "user", content: "hi" }] }),
  });
  const j = JSON.parse(await body(r));
  ok("type message", j.type === "message");
  ok("content text", j.content[0].text === "hello world from mock");
  ok("stop_reason end_turn", j.stop_reason === "end_turn");
}

console.log("== Anthropic 流式 ==");
{
  const r = await call("/v1/messages", {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: "Bearer 1" },
    body: JSON.stringify({ model: "summarize", max_tokens: 100, messages: [{ role: "user", content: "hi" }], stream: true }),
  });
  const s = await body(r);
  ok("含 message_start", s.includes("event: message_start"));
  ok("含 content_block_delta", s.includes("event: content_block_delta"));
  ok("含 message_stop", s.includes("event: message_stop"));
}

console.log("== 额度哨兵 → 429 ==");
{
  globalThis.__MODE = "quota";
  const r = await call("/v1/chat/completions", {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: "Bearer 1" },
    body: JSON.stringify({ model: "summarize", messages: [{ role: "user", content: "hi" }] }),
  });
  ok("HTTP 429", r.status === 429, String(r.status));
  globalThis.__MODE = "ok";
}

console.log("== 空上游 → 502 ==");
{
  globalThis.__MODE = "empty";
  const r = await call("/v1/chat/completions", {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: "Bearer 1" },
    body: JSON.stringify({ model: "summarize", messages: [{ role: "user", content: "hi" }] }),
  });
  ok("HTTP 502", r.status === 502, String(r.status));
  globalThis.__MODE = "ok";
}

console.log("== 空 messages → 400 ==");
{
  const r = await call("/v1/chat/completions", {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: "Bearer 1" },
    body: JSON.stringify({ messages: [] }),
  });
  ok("HTTP 400", r.status === 400, String(r.status));
}

console.log("== 未知模型透传 ==");
{
  const r = await call("/v1/chat/completions", {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: "Bearer 1" },
    body: JSON.stringify({ model: "brand-new-tool", messages: [{ role: "user", content: "hi" }] }),
  });
  const j = JSON.parse(await body(r));
  ok("model 回显", j.model === "brand-new-tool");
}

console.log("== UI ==");
{
  const r = await call("/");
  const s = await body(r);
  ok("HTML 返回", r.headers.get("content-type").includes("text/html"));
  ok("含控制台标题", s.includes("控制台"));
}

console.log(`\n结果: ${pass} 通过 / ${fail} 失败`);
process.exit(fail ? 1 : 0);
