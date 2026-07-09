//! Operator web console (embedded HTML) + RBAC token checks.

use super::engagement::{Engagement, Role};

pub fn console_html(eng: &Engagement) -> String {
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<title>Anubis AOP Console — {name}</title>
<style>
  :root {{ --bg:#0b0f14; --panel:#121821; --fg:#e6edf3; --mut:#8b9bb4; --acc:#3dDC97; --warn:#f0a202; --bad:#ff5d5d; }}
  * {{ box-sizing:border-box; }}
  body {{ margin:0; font:14px/1.45 ui-sans-serif,system-ui; background:var(--bg); color:var(--fg); }}
  header {{ padding:16px 20px; border-bottom:1px solid #1e2633; display:flex; gap:16px; align-items:center; }}
  header h1 {{ margin:0; font-size:16px; letter-spacing:.04em; }}
  header .meta {{ color:var(--mut); font-size:12px; }}
  main {{ display:grid; grid-template-columns: 280px 1fr; min-height:calc(100vh - 58px); }}
  nav {{ border-right:1px solid #1e2633; padding:16px; background:var(--panel); }}
  nav button {{ display:block; width:100%; text-align:left; margin:0 0 8px; padding:10px 12px; border:1px solid #243044; background:#0f1520; color:var(--fg); border-radius:8px; cursor:pointer; }}
  nav button:hover {{ border-color:var(--acc); }}
  section {{ padding:20px; }}
  .card {{ background:var(--panel); border:1px solid #1e2633; border-radius:12px; padding:16px; margin-bottom:16px; }}
  label {{ display:block; color:var(--mut); font-size:12px; margin-bottom:4px; }}
  input, select, textarea {{ width:100%; background:#0b1018; color:var(--fg); border:1px solid #2a3548; border-radius:8px; padding:8px 10px; margin-bottom:10px; }}
  .row {{ display:grid; grid-template-columns:1fr 1fr; gap:12px; }}
  pre {{ background:#0b1018; border-radius:8px; padding:12px; overflow:auto; max-height:360px; font-size:12px; }}
  .ok {{ color:var(--acc); }} .bad {{ color:var(--bad); }}
  .pill {{ display:inline-block; padding:2px 8px; border-radius:999px; background:#1a2433; color:var(--mut); font-size:11px; }}
</style>
</head>
<body>
<header>
  <h1>ANUBIS AOP</h1>
  <div class="meta">
    engagement <span class="pill">{eid}</span>
    transport <span class="pill">{transport}</span>
    protocol <span class="pill">aop-2</span>
    encrypt <span class="pill">{enc}</span>
  </div>
</header>
<main>
  <nav>
    <button onclick="show('dash')">Dashboard</button>
    <button onclick="show('agents')">Agents</button>
    <button onclick="show('tasks')">Task queue</button>
    <button onclick="show('results')">Results</button>
    <button onclick="show('ops')">RBAC / ops</button>
  </nav>
  <section>
    <div id="dash" class="card">
      <h2>Dashboard</h2>
      <p class="meta">Operator console for engagement-scoped C2. Default loopback only.</p>
      <div class="row">
        <div><label>Health</label><pre id="health">…</pre></div>
        <div><label>Engagement</label><pre id="eng">{eng_json}</pre></div>
      </div>
    </div>
    <div id="agents" class="card" hidden>
      <h2>Agents</h2>
      <button onclick="loadAgents()">Refresh</button>
      <pre id="agentsPre">[]</pre>
    </div>
    <div id="tasks" class="card" hidden>
      <h2>Queue task</h2>
      <label>Operator identity (RBAC)</label>
      <input id="op" value="operator"/>
      <label>Agent id (* = broadcast)</label>
      <input id="agentId" value="*"/>
      <label>Module</label>
      <select id="module">
        <option>whoami</option><option>hostname</option><option>pwd</option>
        <option>id</option><option>uname</option><option>ls</option><option>die</option>
      </select>
      <label>Args (comma-separated)</label>
      <input id="args" placeholder="optional"/>
      <button onclick="queueTask()">Queue</button>
      <pre id="queueOut"></pre>
    </div>
    <div id="results" class="card" hidden>
      <h2>Results</h2>
      <button onclick="loadResults()">Refresh</button>
      <pre id="resultsPre">[]</pre>
    </div>
    <div id="ops" class="card" hidden>
      <h2>RBAC</h2>
      <pre id="rbac">{rbac}</pre>
      <p class="meta">Admin can queue any module; ReadOnly can only view.</p>
    </div>
  </section>
</main>
<script>
const views = ['dash','agents','tasks','results','ops'];
function show(id) {{ views.forEach(v => document.getElementById(v).hidden = v !== id); }}
async function jget(p) {{ const r = await fetch(p); return r.json(); }}
async function jpost(p, body) {{
  const r = await fetch(p, {{ method:'POST', headers:{{'Content-Type':'application/json','X-Anubis-Operator': document.getElementById('op')?.value || 'operator'}}, body: JSON.stringify(body)}});
  return r.json();
}}
async function loadHealth() {{ document.getElementById('health').textContent = JSON.stringify(await jget('/health'), null, 2); }}
async function loadAgents() {{ document.getElementById('agentsPre').textContent = JSON.stringify(await jget('/agents'), null, 2); }}
async function loadResults() {{ document.getElementById('resultsPre').textContent = JSON.stringify(await jget('/results'), null, 2); }}
async function queueTask() {{
  const args = (document.getElementById('args').value || '').split(',').map(s=>s.trim()).filter(Boolean);
  const body = {{ agent_id: document.getElementById('agentId').value, module: document.getElementById('module').value, args, operator: document.getElementById('op').value }};
  document.getElementById('queueOut').textContent = JSON.stringify(await jpost('/task', body), null, 2);
}}
loadHealth(); setInterval(loadHealth, 5000);
</script>
</body>
</html>
"##,
        name = eng.name,
        eid = eng.engagement_id,
        transport = eng.transport,
        enc = eng.encrypt_beacons,
        eng_json = serde_json::to_string_pretty(&serde_json::json!({
            "id": eng.engagement_id,
            "c2": eng.c2_bind,
            "kill_date": eng.kill_date,
            "jitter_pct": eng.jitter_pct,
            "mtls_ready": eng.mtls_ready,
        }))
        .unwrap_or_else(|_| "{}".into()),
        rbac = serde_json::to_string_pretty(&eng.operators).unwrap_or_else(|_| "[]".into()),
    )
}

pub fn role_can_queue(eng: &Engagement, operator: &str) -> Result<(), String> {
    eng.assert_role(operator, Role::Operator)
        .map_err(|e| e.to_string())
}

pub fn role_can_admin(eng: &Engagement, operator: &str) -> Result<(), String> {
    eng.assert_role(operator, Role::Admin)
        .map_err(|e| e.to_string())
}
