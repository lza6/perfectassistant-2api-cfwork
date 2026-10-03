//! 内置控制面板（单页 HTML）：端点/Key/模型目录 + 在线测试

use crate::config::Config;
use crate::models::ModelRegistry;

pub fn dashboard_html(cfg: &Config, registry: &ModelRegistry, origin: &str) -> String {
    let models: Vec<serde_json::Value> = registry
        .all()
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id, "name": m.name, "category": m.category, "is_default": m.is_default
            })
        })
        .collect();
    let models_json = serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string());
    let default_model = &cfg.default_model;
    let version = env!("CARGO_PKG_VERSION");

    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>PerfectAssistant2API · 控制面板</title>
<style>
  :root {{
    --bg:#0f1115; --panel:#171a21; --card:#1d2129; --border:#2a2f3a;
    --text:#e6e8ec; --sec:#8b93a3; --primary:#f0b429; --primary-h:#ffc94d;
    --ok:#3fb950; --err:#f85149; --input:#242833;
  }}
  * {{ box-sizing:border-box; }}
  body {{ margin:0; font-family:'Segoe UI',system-ui,-apple-system,sans-serif;
    background:var(--bg); color:var(--text); display:flex; height:100vh; overflow:hidden; }}
  .side {{ width:340px; background:var(--panel); border-right:1px solid var(--border);
    padding:20px; overflow-y:auto; flex-shrink:0; }}
  .main {{ flex:1; display:flex; flex-direction:column; padding:20px; overflow:hidden; }}
  h1 {{ font-size:17px; margin:0; }} .ver {{ font-size:12px; color:var(--sec); font-weight:400; }}
  .card {{ background:var(--card); border:1px solid var(--border); border-radius:8px;
    padding:12px; margin-bottom:12px; }}
  .lbl {{ font-size:12px; color:var(--sec); margin-bottom:6px; }}
  .box {{ background:#0b0d11; border-radius:5px; padding:8px; font-family:ui-monospace,Consolas,monospace;
    font-size:12px; color:var(--primary); word-break:break-all; cursor:pointer; }}
  .box:hover {{ background:#000; }}
  input,textarea {{ width:100%; background:var(--input); border:1px solid var(--border); color:var(--text);
    padding:8px; border-radius:5px; font-size:13px; font-family:inherit; }}
  input:focus,textarea:focus {{ outline:none; border-color:var(--primary); }}
  button {{ background:var(--primary); color:#000; border:none; padding:8px 16px; border-radius:5px;
    font-weight:600; cursor:pointer; }}
  button:hover {{ background:var(--primary-h); }}
  button.ghost {{ background:transparent; color:var(--sec); border:1px solid var(--border); font-weight:400; }}
  .term {{ flex:1; background:var(--panel); border:1px solid var(--border); border-radius:8px;
    display:flex; flex-direction:column; overflow:hidden; }}
  .out {{ flex:1; padding:14px; overflow-y:auto; font-size:14px; line-height:1.6; }}
  .inp {{ border-top:1px solid var(--border); padding:12px; display:flex; gap:8px; background:var(--card); }}
  .m {{ margin-bottom:10px; white-space:pre-wrap; }} .m.u {{ color:var(--primary); font-weight:600; }}
  .m.s {{ color:var(--sec); font-size:12px; }} .m.e {{ color:var(--err); }}
  .dot {{ display:inline-block; width:8px; height:8px; border-radius:50%; background:#555; margin-right:6px; }}
  .dot.ok {{ background:var(--ok); }} .dot.err {{ background:var(--err); }}
  select {{ width:100%; background:var(--input); border:1px solid var(--border); color:var(--text);
    padding:8px; border-radius:5px; }}
  .hdr {{ display:flex; justify-content:space-between; align-items:center; margin-bottom:16px;
    border-bottom:1px solid var(--border); padding-bottom:14px; }}
  @media (max-width:760px) {{ body {{ flex-direction:column; overflow:auto; }} .side {{ width:100%; }} }}
</style>
</head>
<body>
<div class="side">
  <div class="hdr"><h1>PerfectAssistant2API <span class="ver">v{version}</span></h1>
    <div style="font-size:12px"><span class="dot" id="dot"></span><span id="st">检测中</span></div></div>

  <div class="card"><div class="lbl">OpenAI 端点 (Base URL)</div>
    <div class="box" onclick="cp(this)" id="openai-ep">/v1</div></div>
  <div class="card"><div class="lbl">Anthropic 端点 (Base URL)</div>
    <div class="box" onclick="cp(this)" id="anthropic-ep">/</div></div>
  <div class="card"><div class="lbl">API Key</div>
    <div class="box" onclick="cp(this)" id="keybox">(未设置 · 本地免鉴权)</div>
    <button class="ghost" style="margin-top:8px" onclick="genKey()">生成 Key</button></div>
  <div class="card"><div class="lbl">默认模型</div>
    <div class="box" onclick="cp(this)">{default_model}</div></div>
  <div class="card"><div class="lbl">工具模型 ({count})</div>
    <select id="modelsel" onchange="onModel()"></select>
    <div style="font-size:11px;color:var(--sec);margin-top:6px">共 {count} 个工具（上游 /ai/free 的 id）</div></div>

  <details style="font-size:12px;color:var(--sec)">
    <summary style="cursor:pointer">客户端接入示例</summary>
    <pre style="white-space:pre-wrap;font-size:11px;color:var(--text)">OpenAI SDK:
base_url = "http://127.0.0.1:47832/v1"
api_key  = "sk-local"

Claude Code:
ANTHROPIC_BASE_URL=http://127.0.0.1:47832
ANTHROPIC_API_KEY=sk-local</pre>
  </details>
</div>

<div class="main">
  <div class="term">
    <div class="out" id="out">
      <div class="m s">就绪。上游: __UPSTREAM__</div>
      <div class="m s">提示：上游为非流式，本网关用伪流式模拟打字机。</div>
    </div>
    <div class="inp">
      <textarea id="ta" rows="2" placeholder="输入…（Enter 发送 / Shift+Enter 换行）"></textarea>
      <button onclick="send()">发送</button>
    </div>
  </div>
</div>

<script>
const MODELS = {models_json};
const BASE = location.origin;
let KEY = "";
const out = document.getElementById('out');
document.getElementById('openai-ep').textContent = BASE + '/v1';
document.getElementById('anthropic-ep').textContent = BASE;

function cp(el) {{ navigator.clipboard.writeText(el.textContent).then(()=>{{ el.style.color='var(--ok)'; setTimeout(()=>el.style.color='',600); }}); }}
function mk(t) {{ const d=document.createElement('div'); d.className='m '+t; out.appendChild(d); out.scrollTop=out.scrollHeight; return d; }}

const sel = document.getElementById('modelsel');
MODELS.forEach(m=>{{ const o=document.createElement('option'); o.value=m.id;
  o.textContent = m.name + '  ·  ' + m.category; sel.appendChild(o); }});
sel.value = "{default_model}";

async function health() {{
  try {{ const r = await fetch(BASE+'/healthz'); const j = await r.json();
    document.getElementById('dot').className='dot ok'; document.getElementById('st').textContent='正常 · '+j.models+' 模型';
  }} catch(e) {{ document.getElementById('dot').className='dot err'; document.getElementById('st').textContent='异常'; }}
}}
async function genKey() {{
  const r = await fetch(BASE+'/api/config/api-key', {{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{action:'generate'}})}});
  const j = await r.json(); KEY = j.key; document.getElementById('keybox').textContent = j.key;
}}

async function send() {{
  const ta = document.getElementById('ta'); const text = ta.value.trim(); if(!text) return;
  ta.value=''; mk('u').textContent = text;
  const ai = mk(''); const hdr = {{'Content-Type':'application/json'}};
  if (KEY) hdr['Authorization'] = 'Bearer '+KEY;
  try {{
    const r = await fetch(BASE+'/v1/chat/completions', {{method:'POST',headers:hdr,body:JSON.stringify({{
      model: sel.value, messages:[{{role:'user',content:text}}], stream:true }})}});
    if(!r.ok) {{ mk('e').textContent = '错误 '+r.status+': '+(await r.text()); return; }}
    const rd = r.body.getReader(); const dec = new TextDecoder(); let buf='';
    while(true) {{ const {{done,value}} = await rd.read(); if(done) break;
      buf += dec.decode(value,{{stream:true}}); const lines = buf.split('\n'); buf = lines.pop();
      for(const ln of lines) {{ if(!ln.startsWith('data: ')) continue; const d = ln.slice(6);
        if(d==='[DONE]') continue; try {{ const j=JSON.parse(d); const c=j.choices?.[0]?.delta?.content;
          if(c) {{ ai.textContent += c; out.scrollTop=out.scrollHeight; }} }} catch(e) {{}} }} }}
  }} catch(e) {{ mk('e').textContent = '请求失败: '+e.message; }}
}}
document.getElementById('ta').addEventListener('keydown', e=>{{
  if(e.key==='Enter' && !e.shiftKey) {{ e.preventDefault(); send(); }}
}});
health();
</script>
</body>
</html>"#,
        version = version,
        default_model = default_model,
        count = registry.len(),
        models_json = models_json,
    )
    .replace("__UPSTREAM__", origin)
}
