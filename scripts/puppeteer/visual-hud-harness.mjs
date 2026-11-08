#!/usr/bin/env node
/**
 * Visual Split-Pane Host HUD Harness for yubi-OS/craft-web-builds.
 *
 * Embeds any of the 16 ArtCraft WebAssembly applications inside a 1280x800 split-pane host page
 * with a live telemetry HUD on the right displaying:
 *   - Same-origin postMessage protocol events (`<app>:ready`, `<app>:response`, `<app>:snapshot`)
 *   - Command catalog & WebMCP `inputSchema` count
 *   - Live command execution (`file.new`, `pdfcraft:open` with synthetic PDF, `file.open` with ELF bytes, etc.)
 *   - Shared Agent Cursor DOM overlay (`#<canvas_id>_agent_cursor`)
 *   - Dual-path frame capture thumbnail (`<app>:snapshot-request` rendered into `<img>`)
 *
 * Usage:
 *   node scripts/puppeteer/visual-hud-harness.mjs [--root=/path/to/unpacked] [--out-dir=/path/to/screenshots] [photocraft pdfcraft ...]
 */
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';

function resolvePuppeteer() {
  for (const base of [process.cwd() + '/', '/var/cache/node_modules/']) {
    try {
      return createRequire(base)('puppeteer-core');
    } catch {}
  }
  throw new Error('Could not resolve puppeteer-core; install puppeteer-core or set NODE_PATH');
}

const puppeteer = resolvePuppeteer();

const flags = {};
const positional = [];
for (const arg of process.argv.slice(2)) {
  if (arg.startsWith('--')) {
    const [k, v] = arg.slice(2).split('=');
    flags[k] = v ?? 'true';
  } else {
    positional.push(arg);
  }
}

const ROOT = path.resolve(flags.root || process.env.CRAFT_SUITE_ROOT || '.');
const OUT_DIR = path.resolve(flags['out-dir'] || ROOT);
const CHROME_PATH = flags.chrome || process.env.CHROME_PATH || '/usr/local/bin/chrome-headless-shell';
fs.mkdirSync(OUT_DIR, { recursive: true });

function resolveAppSiteDir(root, slug) {
  const pkgSite = path.join(root, `${slug}-pkg`, 'site');
  if (fs.existsSync(pkgSite)) {
    const sub = fs.readdirSync(pkgSite).find((d) => fs.statSync(path.join(pkgSite, d)).isDirectory());
    if (sub) return path.join(pkgSite, sub);
  }
  const sharedSite = path.join(root, 'site');
  if (fs.existsSync(sharedSite)) {
    const sub = fs.readdirSync(sharedSite).find((d) => d.startsWith(`${slug}-web-`) && fs.statSync(path.join(sharedSite, d)).isDirectory());
    if (sub) return path.join(sharedSite, sub);
  }
  const direct = path.join(root, slug);
  if (fs.existsSync(path.join(direct, 'index.html'))) return direct;
  return null;
}

const APPS = positional.length > 0 ? positional : ['photocraft', 'pdfcraft'];
const DIRS = {};
for (const slug of APPS) {
  const dir = resolveAppSiteDir(ROOT, slug);
  if (!dir) throw new Error(`Could not find unpacked site directory for ${slug} under ${ROOT}`);
  const appDir = path.join(dir, 'app');
  const snippetsDir = path.join(dir, 'snippets');
  const appSnippetsDir = path.join(appDir, 'snippets');
  if (fs.existsSync(appDir) && fs.existsSync(snippetsDir) && !fs.existsSync(appSnippetsDir)) {
    fs.cpSync(snippetsDir, appSnippetsDir, { recursive: true });
  }
  DIRS[slug] = dir;
}

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.ttf': 'font/ttf',
  '.otf': 'font/otf',
  '.woff2': 'font/woff2',
  '.webmanifest': 'application/manifest+json',
};

function makeMinimalPdfBytes() {
  const pdf = `%PDF-1.4
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 500 360] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >> endobj
4 0 obj << /Length 164 >> stream
0.09 0.13 0.22 rg 0 0 500 360 re f
0.22 0.74 0.97 rg 30 290 440 40 re f
BT /F1 18 Tf 0.05 0.08 0.14 rg 45 304 Td (yubi-OS Universal Bridge · PDFCraft Live Test) Tj ET
BT /F1 14 Tf 0.90 0.94 0.98 rg 45 210 Td (Opened via same-origin postMessage pdfcraft:open) Tj ET
endstream endobj
5 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >> endobj
xref
0 6
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
0000000241 00000 n 
0000000456 00000 n 
trailer << /Size 6 /Root 1 0 R >>
startxref
529
%%EOF`;
  return Buffer.from(pdf, 'utf-8');
}

function makeHostHtml(appSlug) {
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>yubi-OS Universal Bridge Host Harness — ${appSlug}</title>
<style>
  * { box-sizing: border-box; }
  body {
    margin: 0; padding: 0; background: #0b0f19; color: #e2e8f0;
    font: 12px/1.45 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    display: grid; grid-template-columns: 1fr 420px; height: 100vh; overflow: hidden;
  }
  #stage { position: relative; width: 100%; height: 100%; background: #18181b; }
  iframe { width: 100%; height: 100%; border: 0; display: block; }
  #hud {
    background: #0f172a; border-left: 1px solid #1e293b; padding: 12px;
    display: flex; flex-direction: column; gap: 10px; overflow-y: auto;
  }
  .title { font-size: 13px; font-weight: 700; color: #38bdf8; letter-spacing: 0.02em; }
  .badge {
    display: inline-block; padding: 2px 8px; border-radius: 999px;
    background: #065f46; color: #6ee7b7; font-size: 11px; font-weight: 600;
  }
  .card {
    background: #1e293b; border: 1px solid #334155; border-radius: 6px; padding: 8px 10px;
  }
  .card h4 { margin: 0 0 6px; font-size: 11px; text-transform: uppercase; color: #94a3b8; }
  pre {
    margin: 0; white-space: pre-wrap; word-break: break-word;
    font-size: 11px; color: #cbd5e1; max-height: 155px; overflow: hidden;
  }
  #snap-preview {
    width: 100%; height: 150px; object-fit: contain; background: #090d16;
    border: 1px solid #334155; border-radius: 4px; display: block;
  }
</style>
</head>
<body>
  <div id="stage">
    <iframe id="craft-frame" src="/${appSlug}/index.html?host=parent&control&webgl&store=memory&workers=0&noworkers&nosw&empty"></iframe>
  </div>
  <div id="hud">
    <div>
      <div class="title">yubi-OS/craft-web-builds · ${appSlug}</div>
      <div style="margin-top:4px"><span id="status-badge" class="badge">BOOTING WASM…</span></div>
    </div>
    <div class="card">
      <h4>Same-Origin Host Protocol Events</h4>
      <pre id="events-log">Waiting for ${appSlug}…</pre>
    </div>
    <div class="card">
      <h4>JSON Control Catalog (WebMCP Schema)</h4>
      <pre id="commands-log">—</pre>
    </div>
    <div class="card">
      <h4>Engine Execute + Inspect + Agent Cursor</h4>
      <pre id="inspect-log">—</pre>
    </div>
    <div class="card">
      <h4>Dual-Path Frame Capture (<span id="snap-meta">pending</span>)</h4>
      <img id="snap-preview" alt="Captured frame preview" />
    </div>
  </div>
  <script>
    window.hostEvents = [];
    window.pendingRequests = new Map();
    const logEl = document.getElementById('events-log');
    window.addEventListener('message', (ev) => {
      const d = ev.data;
      if (!d || typeof d !== 'object') return;
      window.hostEvents.push({
        type: d.type || d.event || d.kind || 'unknown',
        requestId: d.requestId || d.id || null,
        keys: Object.keys(d),
        hasBytes: d.bytes instanceof ArrayBuffer ? d.bytes.byteLength : 0,
        raw: d,
      });
      logEl.textContent = window.hostEvents
        .slice(-8)
        .map((e, i) => \`[\${i+1}] \${e.type}\${e.requestId ? ' ('+e.requestId+')' : ''}\${e.hasBytes ? ' ['+e.hasBytes+'B]' : ''}\`)
        .join('\\n');
      const reqId = d.requestId || d.id;
      if (reqId && window.pendingRequests.has(reqId)) {
        const resolve = window.pendingRequests.get(reqId);
        window.pendingRequests.delete(reqId);
        resolve(d);
      }
    });
    window.sendHostRequest = (msg, timeoutMs = 15000) => {
      const frame = document.getElementById('craft-frame');
      return new Promise((resolve, reject) => {
        const reqId = msg.requestId || msg.id;
        const timer = setTimeout(() => {
          window.pendingRequests.delete(reqId);
          reject(new Error('Timeout waiting for ' + reqId + ' (' + msg.type + ')'));
        }, timeoutMs);
        window.pendingRequests.set(reqId, (res) => {
          clearTimeout(timer);
          resolve(res);
        });
        frame.contentWindow.postMessage(msg, window.location.origin);
      });
    };
  </script>
</body>
</html>`;
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://127.0.0.1');
  if (url.pathname.startsWith('/host/')) {
    const slug = url.pathname.slice(6);
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    res.end(makeHostHtml(slug));
    return;
  }
  for (const slug of APPS) {
    if (url.pathname.startsWith(`/${slug}/`)) {
      const filePath = path.join(DIRS[slug], url.pathname.slice(slug.length + 2) || 'index.html');
      if (fs.existsSync(filePath) && fs.statSync(filePath).isFile()) {
        const ext = path.extname(filePath);
        res.writeHead(200, {
          'Content-Type': MIME[ext] || 'application/octet-stream',
          'Cache-Control': 'no-store',
        });
        fs.createReadStream(filePath).pipe(res);
        return;
      }
    }
  }
  res.writeHead(404);
  res.end('Not found: ' + url.pathname);
});

await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const port = server.address().port;

const browser = await puppeteer.launch({
  executablePath: CHROME_PATH,
  headless: true,
  args: [
    '--no-sandbox',
    '--disable-setuid-sandbox',
    '--use-gl=angle',
    '--use-angle=swiftshader',
    '--enable-unsafe-swiftshader',
    '--enable-webgl',
    '--ignore-gpu-blocklist',
    '--window-size=1280,800',
  ],
});

const samplePdfB64 = makeMinimalPdfBytes().toString('base64');
const results = {};

for (const slug of APPS) {
  const page = await browser.newPage();
  await page.setViewport({ width: 1280, height: 800, deviceScaleFactor: 1.5 });
  await page.goto(`http://127.0.0.1:${port}/host/${slug}`, { waitUntil: 'domcontentloaded' });

  await page.waitForFunction(
    (s) => {
      const frame = document.getElementById('craft-frame');
      const win = frame && frame.contentWindow;
      const bridgeReady = win && win.craftUniversalBridge && typeof win.craftUniversalBridge.commands === 'function';
      const canvas = win && win.document && win.document.querySelector('canvas');
      const loadingRemoved = win && win.document && !win.document.getElementById(`${s}_loading`) && !win.document.getElementById('loading');
      return bridgeReady && canvas && loadingRemoved && canvas.width > 300;
    },
    { timeout: 60000, polling: 250 },
    slug
  );
  await new Promise((r) => setTimeout(r, 1200));

  const report = await page.evaluate(async (s, pdfB64) => {
    const frame = document.getElementById('craft-frame');
    const appWin = frame.contentWindow;
    const bridge = appWin.craftUniversalBridge;
    document.getElementById('status-badge').textContent = 'UNIVERSAL BRIDGE ACTIVE (' + s.toUpperCase() + ')';

    if (s === 'pdfcraft' && pdfB64) {
      const bin = atob(pdfB64);
      const bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
      frame.contentWindow.postMessage({
        type: 'pdfcraft:open',
        requestId: 'open-pdf-1',
        name: 'universal-bridge-demo.pdf',
        bytes: bytes.buffer,
      }, window.location.origin);
      await new Promise((r) => setTimeout(r, 500));
    }

    const cmdResp = await window.sendHostRequest({ type: `${s}:commands`, requestId: 'cmd-list-1' });
    const rawCmds = cmdResp.result || cmdResp.commands;
    const commands = Array.isArray(rawCmds)
      ? rawCmds
      : (rawCmds && Array.isArray(rawCmds.commands) ? rawCmds.commands : []);
    const schemaCount = commands.filter((c) => c && c.inputSchema && c.inputSchema.type === 'object').length;
    const sampleCommands = commands.slice(0, 8).map((c) => c.id || c.name);
    document.getElementById('commands-log').textContent =
      `Total commands: ${commands.length} (WebMCP inputSchema: ${schemaCount}/${commands.length})\n` +
      `Sample IDs:\n  • ` + sampleCommands.join('\n  • ');

    frame.contentWindow.postMessage({
      type: `${s}:cursor`,
      x: 420,
      y: 280,
      visible: true,
      pressed: true,
      label: `Sauna-Agent · ${s}`,
      color: '#38bdf8',
    }, window.location.origin);

    await new Promise((r) => setTimeout(r, 100));
    const cursorState = bridge.getCursor();
    const cursorEl = appWin.document.querySelector('[id$="_agent_cursor"]');

    let execCmd = sampleCommands[0] || 'file.new';
    let execParams = {};
    if (s === 'photocraft' && commands.some((c) => (c.id || c.name) === 'file.new')) {
      execCmd = 'file.new';
      execParams = { width: 800, height: 600 };
    } else if (s === 'craft-anything') {
      execCmd = 'file.open';
      execParams = { name: 'hello.elf', data: btoa('\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00>\x00\x01\x00\x00\x00') };
    }
    const execResp = await window.sendHostRequest({
      type: `${s}:execute`,
      requestId: 'exec-1',
      command: execCmd,
      params: execParams,
    });

    const inspectResp = await window.sendHostRequest({ type: `${s}:inspect`, requestId: 'insp-1' });
    const uiInspectResp = await window.sendHostRequest({ type: `${s}:inspect`, requestId: 'insp-ui-1', target: 'ui' });
    document.getElementById('inspect-log').textContent =
      `Executed: ${execCmd}(${JSON.stringify(execParams)}) -> ok=${execResp.ok}\n` +
      `Cursor DOM: #${cursorEl ? cursorEl.id : 'none'} @ (${cursorState.x}, ${cursorState.y}) label="${cursorState.label}"\n` +
      `Doc Inspect (ok=${inspectResp.ok}): ` + JSON.stringify(inspectResp.result).slice(0, 140) + `\n` +
      `UI Inspect (ok=${uiInspectResp.ok}): ` + JSON.stringify(uiInspectResp.result).slice(0, 140);

    const snapResp = await window.sendHostRequest({
      type: `${s}:snapshot-request`,
      requestId: 'snap-1',
      includeCursor: true,
    });
    if (snapResp && snapResp.data) {
      document.getElementById('snap-preview').src = `data:image/png;base64,${snapResp.data}`;
      document.getElementById('snap-meta').textContent =
        `${snapResp.bytes ? snapResp.bytes.byteLength : 0} bytes (image/png, ${snapResp.width}x${snapResp.height})`;
    }

    const stream = bridge.captureStream(15);
    const tracks = stream ? stream.getVideoTracks().length : 0;
    const ackState = bridge.saveAck(1);

    return {
      app: s,
      totalCommands: commands.length,
      webMcpSchemas: schemaCount,
      executedCommand: execCmd,
      executeOk: execResp.ok,
      docInspectOk: inspectResp.ok,
      uiInspectOk: uiInspectResp.ok,
      cursorDomId: cursorEl ? cursorEl.id : null,
      snapshotOk: snapResp.ok,
      snapshotByteLength: snapResp.bytes ? snapResp.bytes.byteLength : 0,
      mediaStreamVideoTracks: tracks,
      ackState,
    };
  }, slug, samplePdfB64);

  await new Promise((r) => setTimeout(r, 400));
  const screenshotPath = path.join(OUT_DIR, `${slug}-puppeteer-live.png`);
  await page.screenshot({ path: screenshotPath });
  report.screenshotPath = screenshotPath;
  results[slug] = report;
  console.log(`[HUD-PASS] ${slug}:`, JSON.stringify(report));
  await page.close();
}

await browser.close();
server.close();
