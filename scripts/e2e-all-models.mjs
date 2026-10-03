// 全量 62 工具真实 E2E：逐个打上游 /ai/free，记录可用性。
// 用法: node scripts/e2e-all-models.mjs [--json]
import { CATALOG } from "../worker.js";

const MODELS = Object.values(CATALOG).flat();
const jsonMode = process.argv.includes("--json");

const UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36";
const PROMPT = "Reply with one short useful sentence.";

async function testOne(id) {
  const t0 = Date.now();
  try {
    const r = await fetch("https://perfectassistant.ai/ai/free", {
      method: "POST",
      headers: {
        "Content-Type": "text/plain;charset=UTF-8",
        Origin: "https://perfectassistant.ai",
        Referer: `https://perfectassistant.ai/iframe/${id}?lang=en`,
        "User-Agent": UA,
        Accept: "*/*",
      },
      body: JSON.stringify({
        tone: "professional",
        language: "en",
        text: PROMPT,
        id,
        chatId: crypto.randomUUID(),
        host: "perfectassistant.ai",
        source: "iframe",
      }),
      signal: AbortSignal.timeout(40000),
    });
    const ms = Date.now() - t0;
    const raw = await r.text();
    let text = "", quota = false, parsed = false;
    try {
      const j = JSON.parse(raw);
      parsed = true;
      text = (j.response && String(j.response)) ||
        (Array.isArray(j.responses) ? (j.responses.find((x) => x && String(x).trim()) || "") : "");
      quota = text.includes("Sign up to continue using Perfect Assistant!");
    } catch { /* non-json */ }
    return {
      id, ok: r.ok && !!text && !quota, status: r.status, ms,
      quota, parsed, len: text.length,
      snippet: text.slice(0, 60).replace(/\s+/g, " "),
    };
  } catch (e) {
    return { id, ok: false, status: 0, ms: Date.now() - t0, error: e.message };
  }
}

const results = [];
for (const id of MODELS) {
  const r = await testOne(id);
  results.push(r);
  if (!jsonMode) {
    const mark = r.ok ? "OK " : (r.quota ? "QUOTA" : "FAIL");
    process.stdout.write(`  ${mark.padEnd(6)} ${id.padEnd(32)} ${String(r.ms).padStart(6)}ms  ${r.snippet || r.error || ""}\n`);
  }
  await new Promise((s) => setTimeout(s, 900)); // 温和间隔，避免触发限流
}

const ok = results.filter((r) => r.ok);
const quota = results.filter((r) => r.quota);
const fail = results.filter((r) => !r.ok && !r.quota);

if (jsonMode) {
  console.log(JSON.stringify({ total: MODELS.length, ok: ok.length, quota: quota.length, fail: fail.length, results }, null, 2));
} else {
  console.log(`\n=== 汇总 ===`);
  console.log(`可用 ${ok.length} / ${MODELS.length}`);
  if (quota.length) console.log(`命中额度哨兵 ${quota.length}: ${quota.map((r) => r.id).join(", ")}`);
  if (fail.length) console.log(`失败 ${fail.length}: ${fail.map((r) => r.id + "(" + (r.status || "ERR") + ")").join(", ")}`);
}
