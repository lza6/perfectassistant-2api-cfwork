// =================================================================================
//  perfectassistant-2api — Cloudflare Worker (单文件版)
//
//  将 perfectassistant.ai 的免费 AI 服务 (/ai/free) 转换为兼容
//  OpenAI (/v1/chat/completions) 与 Anthropic (/v1/messages) 的 API。
//
//  上游为非流式 JSON，本 Worker 内置伪流式 (Pseudo-Streaming) 引擎，
//  为客户端模拟打字机体验。协议依据见 docs/PROTOCOL.md（逆向自站点 JS + 实测）。
//
//  [本地版] 同一上游协议的 Rust 本地网关见 local/ 目录（tokenharbor2api 架构）。
// =================================================================================

// --- [第一部分: 配置] ---
export const CONFIG = {
  PROJECT_NAME: "perfectassistant-2api",
  PROJECT_VERSION: "2.0.0",

  // 安全: 优先读取环境变量 API_MASTER_KEY；默认 "1" 为弱密钥(仅本地/自用)
  API_MASTER_KEY: "1",

  // 上游
  UPSTREAM_URL: "https://perfectassistant.ai/ai/free",
  ORIGIN_URL: "https://perfectassistant.ai",
  USER_AGENT:
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36",

  // 上游表单默认值
  DEFAULT_TONE: "professional",
  DEFAULT_LANGUAGE: "en", // 上游语言枚举无中文: en,fr,de,it,es,ru,id,ar,th,vi,tr,nl,pl,ko,hi

  // 伪流式分块
  CHUNK_CHARS: 120,
  CHUNK_DELAY_MS: 0,

  // 默认工具
  DEFAULT_MODEL: "brainstorm-tool",
};

// 上游额度哨兵: responses[0] 含此串表示免费额度用尽/需登录
const QUOTA_SENTINEL = "Sign up to continue using Perfect Assistant!";

// --- [第二部分: 工具目录 (62 个, 来源 __manifest 路由表)] ---
export const CATALOG = {
  business: ["brainstorm-tool","brand-name-generator","company-profile","interview-questions-generator","keywords-generator","market-research","marketing-campaign-ideas","meeting-minutes-generator","ocr-text","okr-generator","review-generator","sop-generator","startup-ideas","write-cover-letter","write-resume"],
  content: ["blog-post-brief","blog-post-generator","blog-post-intro","continue-sentence","create-faq","cta-generator","fix-grammar","give-definition","headline-generator","humanize-text","improve","paraphrase","seo-title-and-description","shorter","simplify","speech-writer","summarize","title","translate","write-paragraph"],
  social: ["blog-post-to-tweet-thread","content-calendar","instagram-captions","social-media-post-ideas","tiktok-instagram-reel-script","tiktok-or-instagram-reel-ideas","tweet-ideas"],
  mail: ["angry-customer-email","mail-improve-draft","mail-reply","mail-subject-line-creator","negative-reply-customer-email","positive-reply-customer-email"],
  chat: ["apology","birthday","greeting","invitation","reply-chat"],
  advertisement: ["facebook-ads","google-ads"],
  slides: ["slides-outline","text-to-slide"],
  spreadsheets: ["explain-excel-formula","generate-excel-formula"],
  video: ["video-script-generator","youtube-titles-generator"],
  advanced: ["query-chat-gpt"],
};

// 扁平化工具 id 列表
const ALL_MODELS = Object.values(CATALOG).flat();
const MODEL_SET = new Set(ALL_MODELS);

function isKnownModel(id) {
  return MODEL_SET.has(id);
}

// --- [第三部分: Worker 入口与路由] ---
export default {
  async fetch(request, env) {
    const apiKey = env.API_MASTER_KEY || CONFIG.API_MASTER_KEY;
    const url = new URL(request.url);

    if (request.method === "OPTIONS") return handleCorsPreflight();

    switch (url.pathname) {
      case "/":
        return handleUI(request, apiKey);
      case "/v1/models":
        return handleModels(request, apiKey);
      case "/v1/chat/completions":
        return handleOpenAI(request, apiKey);
      case "/v1/messages":
        return handleAnthropic(request, apiKey);
      case "/v1/messages/count_tokens":
        return handleCountTokens(request, apiKey);
      case "/healthz":
        return new Response(
          JSON.stringify({ status: "ok", models: ALL_MODELS.length, upstream: CONFIG.ORIGIN_URL }),
          { headers: corsHeaders({ "Content-Type": "application/json" }) }
        );
      default:
        return createErrorResponse(`未找到路径: ${url.pathname}`, 404, "not_found");
    }
  },
};

// --- [第四部分: 上游调用] ---

/**
 * 调用上游 /ai/free，返回纯文本
 * @returns {Promise<{ok:true,text:string}|{ok:false,status:number,message:string}>}
 */
async function callUpstream(toolId, prompt, tone, language) {
  const payload = {
    text: prompt,
    id: toolId,
    chatId: crypto.randomUUID(),
    host: "perfectassistant.ai",
    source: "iframe",
    tone: tone || CONFIG.DEFAULT_TONE,
    language: language || CONFIG.DEFAULT_LANGUAGE,
  };

  let response;
  try {
    response = await fetch(CONFIG.UPSTREAM_URL, {
      method: "POST",
      headers: {
        "Content-Type": "text/plain;charset=UTF-8",
        Origin: CONFIG.ORIGIN_URL,
        Referer: `${CONFIG.ORIGIN_URL}/iframe/${toolId}?lang=${payload.language}`,
        "User-Agent": CONFIG.USER_AGENT,
        Accept: "*/*",
        "Accept-Language": "en-US,en;q=0.9",
      },
      body: JSON.stringify(payload),
    });
  } catch (e) {
    return { ok: false, status: 502, message: `上游请求失败: ${e.message}` };
  }

  if (!response.ok) {
    const errText = await response.text().catch(() => "");
    return { ok: false, status: 502, message: `上游服务错误: ${response.status} - ${errText.slice(0, 200)}` };
  }

  let data;
  try {
    data = await response.json();
  } catch (e) {
    return { ok: false, status: 502, message: "上游返回非 JSON" };
  }

  // 解析: 优先 response，回退 responses[0]
  const text =
    (data.response && String(data.response)) ||
    (Array.isArray(data.responses) ? data.responses.find((r) => r && String(r).trim()) : "") ||
    "";

  if (text.includes(QUOTA_SENTINEL)) {
    const hourly = /hourly request limit \(60 requests\)/.test(text);
    return {
      ok: false,
      status: 429,
      message: hourly
        ? "上游免费限流：已达 60 次/小时/IP。请稍后再试（约 1 小时）或更换网络出口"
        : "上游免费额度已用尽/需登录：请稍后再试或更换网络出口",
    };
  }
  if (!text.trim()) {
    return { ok: false, status: 502, message: "上游未返回有效内容" };
  }
  return { ok: true, text };
}

/** 解析请求模型 → 工具 id */
function resolveToolId(requested) {
  if (!requested) return CONFIG.DEFAULT_MODEL;
  if (isKnownModel(requested)) return requested;
  return requested; // 未知模型透传上游 (目录可能更新)
}

/** 从 messages 数组抽取纯文本 */
function extractPrompt(messages, system) {
  const parts = [];
  if (system) {
    const s = typeof system === "string" ? system : contentToText(system);
    if (s.trim()) parts.push(s);
  }
  for (const m of messages || []) {
    const t = contentToText(m.content);
    if (t.trim()) parts.push(t);
  }
  return parts.join("\n\n");
}

function contentToText(content) {
  if (content == null) return "";
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((p) => (typeof p === "string" ? p : p && typeof p.text === "string" ? p.text : ""))
      .filter(Boolean)
      .join("\n");
  }
  return "";
}

// --- [第五部分: OpenAI 兼容] ---

async function handleOpenAI(request, apiKey) {
  if (!verifyAuth(request, apiKey)) return createErrorResponse("未授权", 401, "unauthorized");

  let body;
  try {
    body = await request.json();
  } catch (e) {
    return createErrorResponse("无效的 JSON 请求体", 400, "invalid_json");
  }

  const messages = body.messages || [];
  if (!messages.length) return createErrorResponse("messages 不能为空", 400, "invalid_request");

  const model = resolveToolId(body.model);
  const prompt = extractPrompt(messages, null);
  const id = `chatcmpl-${crypto.randomUUID()}`;
  const created = Math.floor(Date.now() / 1000);

  const res = await callUpstream(model, prompt, body.tone, body.language);

  if (!res.ok) {
    if (body.stream) return streamFromFrames(errorFramesOpenAI(res.message, model, id, created));
    return createErrorResponse(res.message, res.status, res.status === 429 ? "rate_limit_error" : "upstream_error");
  }

  if (body.stream) {
    return streamFromFrames(openaiFrames(res.text, model, id, created));
  }

  const completion = {
    id,
    object: "chat.completion",
    created,
    model,
    choices: [{ index: 0, message: { role: "assistant", content: res.text }, finish_reason: "stop" }],
    usage: {
      prompt_tokens: estimateTokens(prompt),
      completion_tokens: estimateTokens(res.text),
      total_tokens: estimateTokens(prompt) + estimateTokens(res.text),
    },
  };
  return new Response(JSON.stringify(completion), {
    headers: corsHeaders({ "Content-Type": "application/json" }),
  });
}

function openaiFrames(content, model, id, created) {
  const frames = [];
  frames.push(sseData({ id, object: "chat.completion.chunk", created, model, choices: [{ index: 0, delta: { role: "assistant" }, finish_reason: null }] }));
  for (const part of chunkText(content, CONFIG.CHUNK_CHARS)) {
    if (!part) continue;
    frames.push(sseData({ id, object: "chat.completion.chunk", created, model, choices: [{ index: 0, delta: { content: part }, finish_reason: null }] }));
  }
  frames.push(sseData({ id, object: "chat.completion.chunk", created, model, choices: [{ index: 0, delta: {}, finish_reason: "stop" }] }));
  frames.push("data: [DONE]\n\n");
  return frames;
}

function errorFramesOpenAI(message, model, id, created) {
  return [
    sseData({ id, object: "chat.completion.chunk", created, model, choices: [{ index: 0, delta: { content: `[上游错误: ${message}]` }, finish_reason: "stop" }] }),
    "data: [DONE]\n\n",
  ];
}

// --- [第六部分: Anthropic 兼容 (/v1/messages)] ---

async function handleAnthropic(request, apiKey) {
  if (!verifyAuth(request, apiKey)) {
    return createErrorResponse("未授权", 401, "unauthorized", true);
  }

  let body;
  try {
    body = await request.json();
  } catch (e) {
    return createErrorResponse("无效的 JSON 请求体", 400, "invalid_json", true);
  }

  const messages = body.messages || [];
  if (!messages.length) return createErrorResponse("messages 不能为空", 400, "invalid_request", true);

  const model = resolveToolId(body.model);
  const prompt = extractPrompt(messages, body.system);
  const id = `msg_${crypto.randomUUID().replace(/-/g, "")}`;
  const inputTokens = estimateTokens(prompt);

  const res = await callUpstream(model, prompt, body.tone, body.language);

  if (!res.ok) {
    if (body.stream) {
      return streamFromFrames(anthropicFrames(`[上游错误: ${res.message}]`, model, id, inputTokens), true);
    }
    return createErrorResponse(res.message, res.status, res.status === 429 ? "rate_limit_error" : "upstream_error", true);
  }

  if (body.stream) {
    return streamFromFrames(anthropicFrames(res.text, model, id, inputTokens), true);
  }

  const msg = {
    id,
    type: "message",
    role: "assistant",
    model,
    content: [{ type: "text", text: res.text }],
    stop_reason: "end_turn",
    stop_sequence: null,
    usage: { input_tokens: inputTokens, output_tokens: estimateTokens(res.text) },
  };
  return new Response(JSON.stringify(msg), {
    headers: corsHeaders({ "Content-Type": "application/json" }),
  });
}

function anthropicFrames(content, model, id, inputTokens) {
  const ev = (name, payload) => `event: ${name}\ndata: ${JSON.stringify(payload)}\n\n`;
  const frames = [];
  frames.push(ev("message_start", { type: "message_start", message: { id, type: "message", role: "assistant", model, content: [], stop_reason: null, stop_sequence: null, usage: { input_tokens: inputTokens, output_tokens: 0 } } }));
  frames.push(ev("content_block_start", { type: "content_block_start", index: 0, content_block: { type: "text", text: "" } }));
  for (const part of chunkText(content, CONFIG.CHUNK_CHARS)) {
    if (!part) continue;
    frames.push(ev("content_block_delta", { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: part } }));
  }
  frames.push(ev("content_block_stop", { type: "content_block_stop", index: 0 }));
  frames.push(ev("message_delta", { type: "message_delta", delta: { stop_reason: "end_turn", stop_sequence: null }, usage: { output_tokens: estimateTokens(content) } }));
  frames.push(ev("message_stop", { type: "message_stop" }));
  return frames;
}

// --- [第七部分: Claude Code 兼容] ---

/** Claude Code 会调用 /v1/messages/count_tokens 预统计。返回估算值。 */
async function handleCountTokens(request, apiKey) {
  if (!verifyAuth(request, apiKey)) return createErrorResponse("未授权", 401, "unauthorized", true);
  let body;
  try { body = await request.json(); } catch { return createErrorResponse("无效 JSON", 400, "invalid_json", true); }
  const prompt = extractPrompt(body.messages || [], body.system);
  return new Response(JSON.stringify({ input_tokens: estimateTokens(prompt) }), {
    headers: corsHeaders({ "Content-Type": "application/json" }),
  });
}

// --- [第八部分: 流/模型/辅助] ---
/** 把帧数组转成 SSE Response (带可选分块延迟) */
function streamFromFrames(frames, anthropic = false) {
  const encoder = new TextEncoder();
  const stream = new ReadableStream({
    async start(controller) {
      for (const f of frames) {
        controller.enqueue(encoder.encode(f));
        if (CONFIG.CHUNK_DELAY_MS > 0) {
          await new Promise((r) => setTimeout(r, CONFIG.CHUNK_DELAY_MS));
        }
      }
      controller.close();
    },
  });
  return new Response(stream, {
    headers: corsHeaders({
      "Content-Type": "text/event-stream; charset=utf-8",
      "Cache-Control": "no-cache",
      "X-Accel-Buffering": "no",
    }),
  });
}

function sseData(obj) {
  return `data: ${JSON.stringify(obj)}\n\n`;
}

/** 按字符切块 (UTF-8 安全) */
function chunkText(text, size) {
  const chars = Array.from(text);
  const out = [];
  for (let i = 0; i < chars.length; i += size) out.push(chars.slice(i, i + size).join(""));
  if (!out.length) out.push("");
  return out;
}

function estimateTokens(text) {
  return Math.max(1, Math.floor(Array.from(text || "").length / 4));
}

function handleModels(request, apiKey) {
  if (!verifyAuth(request, apiKey)) return createErrorResponse("未授权", 401, "unauthorized");
  const data = ALL_MODELS.map((id) => {
    const category = Object.keys(CATALOG).find((c) => CATALOG[c].includes(id)) || "other";
    return { id, object: "model", created: 1735689600, owned_by: "perfectassistant", perfectassistant: { category } };
  });
  return new Response(JSON.stringify({ object: "list", data }), {
    headers: corsHeaders({ "Content-Type": "application/json" }),
  });
}

// --- [第九部分: 认证 / CORS / 错误] ---

function verifyAuth(request, validKey) {
  if (validKey === "1" || !validKey) return true; // 弱密钥/未配置: 放行
  const auth = request.headers.get("Authorization");
  const xKey = request.headers.get("x-api-key");
  return auth === `Bearer ${validKey}` || xKey === validKey;
}

function createErrorResponse(message, status, code, anthropic = false) {
  const body = anthropic
    ? { type: "error", error: { type: code, message } }
    : { error: { message, type: code, code } };
  return new Response(JSON.stringify(body), {
    status,
    headers: corsHeaders({ "Content-Type": "application/json" }),
  });
}

function handleCorsPreflight() {
  return new Response(null, { status: 204, headers: corsHeaders() });
}

function corsHeaders(headers = {}) {
  return {
    ...headers,
    "Access-Control-Allow-Origin": "*",
    "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
    "Access-Control-Allow-Headers": "Content-Type, Authorization, X-API-Key, anthropic-version",
  };
}

// --- [第十部分: 开发者控制台 UI] ---
function handleUI(request, apiKey) {
  const origin = new URL(request.url).origin;
  const modelsJson = JSON.stringify(
    ALL_MODELS.map((id) => ({
      id,
      category: Object.keys(CATALOG).find((c) => CATALOG[c].includes(id)) || "other",
    }))
  );

  const html = `<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>${CONFIG.PROJECT_NAME} · 控制台</title>
<style>
  :root{--bg:#0f1115;--panel:#171a21;--card:#1d2129;--bd:#2a2f3a;--tx:#e6e8ec;--sec:#8b93a3;--pri:#f0b429;--prih:#ffc94d;--ok:#3fb950;--err:#f85149;--in:#242833}
  *{box-sizing:border-box}body{margin:0;font-family:'Segoe UI',system-ui,sans-serif;background:var(--bg);color:var(--tx);display:flex;height:100vh;overflow:hidden}
  .side{width:340px;background:var(--panel);border-right:1px solid var(--bd);padding:20px;overflow-y:auto;flex-shrink:0}
  .main{flex:1;display:flex;flex-direction:column;padding:20px;overflow:hidden}
  h1{font-size:17px;margin:0}.ver{font-size:12px;color:var(--sec);font-weight:400}
  .card{background:var(--card);border:1px solid var(--bd);border-radius:8px;padding:12px;margin-bottom:12px}
  .lbl{font-size:12px;color:var(--sec);margin-bottom:6px}
  .box{background:#0b0d11;border-radius:5px;padding:8px;font-family:ui-monospace,Consolas,monospace;font-size:12px;color:var(--pri);word-break:break-all;cursor:pointer}
  .box:hover{background:#000}
  textarea{width:100%;background:var(--in);border:1px solid var(--bd);color:var(--tx);padding:8px;border-radius:5px;font-family:inherit;resize:none}
  textarea:focus{outline:none;border-color:var(--pri)}
  button{background:var(--pri);color:#000;border:none;padding:8px 16px;border-radius:5px;font-weight:600;cursor:pointer}
  button:hover{background:var(--prih)}button.ghost{background:transparent;color:var(--sec);border:1px solid var(--bd);font-weight:400}
  select{width:100%;background:var(--in);border:1px solid var(--bd);color:var(--tx);padding:8px;border-radius:5px}
  .term{flex:1;background:var(--panel);border:1px solid var(--bd);border-radius:8px;display:flex;flex-direction:column;overflow:hidden}
  .out{flex:1;padding:14px;overflow-y:auto;font-size:14px;line-height:1.6}
  .inp{border-top:1px solid var(--bd);padding:12px;display:flex;gap:8px;background:var(--card)}
  .m{margin-bottom:10px;white-space:pre-wrap}.m.u{color:var(--pri);font-weight:600}.m.s{color:var(--sec);font-size:12px}.m.e{color:var(--err)}
  .dot{display:inline-block;width:8px;height:8px;border-radius:50%;background:#555;margin-right:6px}.dot.ok{background:var(--ok)}.dot.err{background:var(--err)}
  .hdr{display:flex;justify-content:space-between;align-items:center;margin-bottom:16px;border-bottom:1px solid var(--bd);padding-bottom:14px}
  @media(max-width:760px){body{flex-direction:column;overflow:auto}.side{width:100%}}
</style>
</head>
<body>
<div class="side">
  <div class="hdr"><h1>${CONFIG.PROJECT_NAME} <span class="ver">v${CONFIG.PROJECT_VERSION}</span></h1>
    <div style="font-size:12px"><span class="dot" id="dot"></span><span id="st">检测中</span></div></div>
  <div class="card"><div class="lbl">OpenAI 端点</div><div class="box" onclick="cp(this)" id="oep"></div></div>
  <div class="card"><div class="lbl">Anthropic 端点 (Claude Code)</div><div class="box" onclick="cp(this)" id="aep"></div></div>
  <div class="card"><div class="lbl">API Key</div><div class="box" onclick="cp(this)">${apiKey}</div></div>
  <div class="card"><div class="lbl">工具模型 (${ALL_MODELS.length})</div><select id="ms"></select></div>
</div>
<div class="main"><div class="term">
  <div class="out" id="out">
    <div class="m s">就绪。上游: ${CONFIG.ORIGIN_URL}（${ALL_MODELS.length} 个工具）</div>
    <div class="m s">提示: 上游非流式，本服务用伪流式模拟打字机。</div>
  </div>
  <div class="inp"><textarea id="ta" rows="2" placeholder="输入…（Enter 发送 / Shift+Enter 换行）"></textarea><button onclick="send()">发送</button></div>
</div></div>
<script>
const MODELS=${modelsJson};
const BASE=location.origin, ORIGIN='${origin}';
let KEY='${apiKey}';
const out=document.getElementById('out');
document.getElementById('oep').textContent=ORIGIN+'/v1';
document.getElementById('aep').textContent=ORIGIN;
function cp(el){navigator.clipboard.writeText(el.textContent).then(()=>{el.style.color='var(--ok)';setTimeout(()=>el.style.color='',600)})}
function mk(t){const d=document.createElement('div');d.className='m '+t;out.appendChild(d);out.scrollTop=out.scrollHeight;return d}
const sel=document.getElementById('ms');
MODELS.forEach(m=>{const o=document.createElement('option');o.value=m.id;o.textContent=m.id+'  ·  '+m.category;sel.appendChild(o)});
sel.value='${CONFIG.DEFAULT_MODEL}';
async function health(){try{const r=await fetch(ORIGIN+'/healthz');const j=await r.json();document.getElementById('dot').className='dot ok';document.getElementById('st').textContent='正常 · '+j.models+' 模型'}catch(e){document.getElementById('dot').className='dot err';document.getElementById('st').textContent='异常'}}
async function send(){const ta=document.getElementById('ta');const text=ta.value.trim();if(!text)return;ta.value='';mk('u').textContent=text;const ai=mk('');
  try{const r=await fetch(ORIGIN+'/v1/chat/completions',{method:'POST',headers:{'Content-Type':'application/json','Authorization':'Bearer '+KEY},body:JSON.stringify({model:sel.value,messages:[{role:'user',content:text}],stream:true})});
    if(!r.ok){mk('e').textContent='错误 '+r.status+': '+(await r.text());return}
    const rd=r.body.getReader(),dec=new TextDecoder();let buf='';
    while(true){const{done,value}=await rd.read();if(done)break;buf+=dec.decode(value,{stream:true});const lines=buf.split('\\n');buf=lines.pop();
      for(const ln of lines){if(!ln.startsWith('data: '))continue;const d=ln.slice(6);if(d==='[DONE]')continue;try{const j=JSON.parse(d);const c=j.choices?.[0]?.delta?.content;if(c){ai.textContent+=c;out.scrollTop=out.scrollHeight}}catch(e){}}}
  }catch(e){mk('e').textContent='请求失败: '+e.message}}
document.getElementById('ta').addEventListener('keydown',e=>{if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();send()}});
health();
</script>
</body></html>`;

  return new Response(html, {
    headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-cache" },
  });
}
