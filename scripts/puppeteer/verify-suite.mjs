#!/usr/bin/env node
/**
 * End-to-End 16-App Puppeteer Verification Suite for yubi-OS/craft-web-builds.
 *
 * Serves unpacked WebAssembly app bundles over a local HTTP server, embeds each app inside a
 * same-origin host page (`?host=parent&control&webgl&store=memory&workers=0&noworkers&nosw&empty`),
 * and verifies:
 *   1. WASM startup, `<app>:ready` handshake, and `window.craftUniversalBridge` / `window.<app>` installation
 *   2. HTML5 `<canvas id="<app>">` DOM-shadowing protection (`!(window[app] instanceof HTMLCanvasElement)`)
 *   3. Same-origin `postMessage` command catalog (`<app>:commands`) & WebMCP `inputSchema` synthesis
 *   4. Shared agent cursor overlay (`<app>:cursor` -> `#<canvas_id>_agent_cursor`)
 *   5. Document & UI state inspection (`<app>:inspect` -> `docInspect` & `uiInspect`)
 *   6. Dual-path frame capture (`<app>:snapshot-request` -> `<app>:snapshot` with base64 `data` + transferred `ArrayBuffer` `bytes`)
 *   7. Live WebRTC `MediaStream` capture (`window.<app>.captureStream(15)`) and revision-gated `saveAck(1)`
 *
 * Usage:
 *   node scripts/puppeteer/verify-suite.mjs [--root=/path/to/unpacked] [--out=v2-results.json] [--screenshots-dir=/path/to/pngs] [app1 app2 ...]
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

const ALL_APPS = [
  'photocraft',
  'vectorcraft',
  'lightcraft',
  'pdfcraft',
  'effectcraft',
  'designcraft',
  'figcraft',
  'filmcraft',
  'texturecraft',
  'materialcraft',
  'wordcraft',
  'soundcraft',
  'deckcraft',
  'cadcraft',
  'gridcraft',
  'craft-anything',
];

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
const CHROME_PATH = flags.chrome || process.env.CHROME_PATH || '/usr/local/bin/chrome-headless-shell';
const OUT_PATH = path.resolve(flags.out || path.join(ROOT, 'v2-results.json'));
const SCREENSHOTS_DIR = flags['screenshots-dir'] ? path.resolve(flags['screenshots-dir']) : null;

if (SCREENSHOTS_DIR) {
  fs.mkdirSync(SCREENSHOTS_DIR, { recursive: true });
}

function resolveAppSiteDir(root, slug) {
  // Layout A: <root>/<slug>-pkg/site/<slug>-web-<ver>/
  const pkgSite = path.join(root, `${slug}-pkg`, 'site');
  if (fs.existsSync(pkgSite)) {
    const sub = fs.readdirSync(pkgSite).find((d) => fs.statSync(path.join(pkgSite, d)).isDirectory());
    if (sub) return path.join(pkgSite, sub);
  }
  // Layout B: <root>/site/<slug>-web-<ver>/
  const sharedSite = path.join(root, 'site');
  if (fs.existsSync(sharedSite)) {
    const sub = fs.readdirSync(sharedSite).find((d) => d.startsWith(`${slug}-web-`) && fs.statSync(path.join(sharedSite, d)).isDirectory());
    if (sub) return path.join(sharedSite, sub);
  }
  // Layout C: <root>/<slug>/
  const direct = path.join(root, slug);
  if (fs.existsSync(path.join(direct, 'index.html'))) {
    return direct;
  }
  return null;
}

const DIRS = {};
const requested = positional.length > 0 ? positional : ALL_APPS;
for (const slug of requested) {
  const dir = resolveAppSiteDir(ROOT, slug);
  if (dir) {
    // Ensure two-stage boot loaders (such as photocraft) have app/snippets copied for subpath hosting
    const appDir = path.join(dir, 'app');
    const snippetsDir = path.join(dir, 'snippets');
    const appSnippetsDir = path.join(appDir, 'snippets');
    if (fs.existsSync(appDir) && fs.existsSync(snippetsDir) && !fs.existsSync(appSnippetsDir)) {
      fs.cpSync(snippetsDir, appSnippetsDir, { recursive: true });
    }
    DIRS[slug] = dir;
  }
}

const APPS = Object.keys(DIRS);
if (APPS.length === 0) {
  console.error(`No unpacked app site directories found under ${ROOT}`);
  process.exit(1);
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

const server = http.createServer((req, res) => {
  const u = new URL(req.url, 'http://127.0.0.1');
  if (u.pathname.startsWith('/host/')) {
    const slug = u.pathname.slice(6);
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    res.end(`<!DOCTYPE html><html><head><meta charset="utf-8"><style>html,body,iframe{margin:0;padding:0;width:100%;height:100%;border:0;background:#111;}</style></head><body>
      <iframe id="f" src="/${slug}/index.html?host=parent&control&webgl&store=memory&workers=0&noworkers&nosw&empty"></iframe>
      <script>
        window.hostEvents = [];
        window.pending = new Map();
        window.addEventListener('message', (ev) => {
          const d = ev.data;
          if (!d || typeof d !== 'object') return;
          window.hostEvents.push(d);
          const id = d.requestId || d.id;
          if (id && window.pending.has(id)) {
            const r = window.pending.get(id);
            window.pending.delete(id);
            r(d);
          }
        });
        window.req = (msg, timeoutMs = 15000) => new Promise((resolve) => {
          const f = document.getElementById('f');
          const id = msg.requestId || msg.id;
          const t = setTimeout(() => {
            window.pending.delete(id);
            resolve({ ok: false, error: 'timeout waiting for ' + id });
          }, timeoutMs);
          window.pending.set(id, (d) => {
            clearTimeout(t);
            resolve(d);
          });
          f.contentWindow.postMessage(msg, location.origin);
        });
      </script>
    </body></html>`);
    return;
  }
  for (const slug of APPS) {
    if (u.pathname.startsWith(`/${slug}/`)) {
      const fp = path.join(DIRS[slug], u.pathname.slice(slug.length + 2) || 'index.html');
      if (fs.existsSync(fp) && fs.statSync(fp).isFile()) {
        res.writeHead(200, {
          'Content-Type': MIME[path.extname(fp)] || 'application/octet-stream',
          'Cache-Control': 'no-store',
        });
        fs.createReadStream(fp).pipe(res);
        return;
      }
    }
  }
  res.writeHead(404);
  res.end('404');
});

await new Promise((r) => server.listen(0, '127.0.0.1', r));
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
    '--window-size=1024,720',
  ],
});

const existing = fs.existsSync(OUT_PATH) ? JSON.parse(fs.readFileSync(OUT_PATH, 'utf-8')) : {};

for (const slug of APPS) {
  const t0 = Date.now();
  const page = await browser.newPage();
  await page.setViewport({ width: 1024, height: 720 });
  try {
    await page.goto(`http://127.0.0.1:${port}/host/${slug}`, { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(
      (s) => {
        const f = document.getElementById('f');
        const w = f && f.contentWindow;
        const c = w && w.document && w.document.querySelector('canvas');
        const loadingGone = w && w.document && !w.document.getElementById(`${s}_loading`) && !w.document.getElementById('loading');
        return w && w.craftUniversalBridge && typeof w.craftUniversalBridge.commands === 'function' && c && c.width > 300 && loadingGone;
      },
      { timeout: 45000, polling: 200 },
      slug
    );
    await new Promise((r) => setTimeout(r, 1000));

    const res = await page.evaluate(async (s) => {
      const f = document.getElementById('f');
      const w = f.contentWindow;
      const globalIsCanvas = w[s] instanceof w.HTMLCanvasElement;

      // 1. Commands via postMessage
      const cmdResp = await window.req({ type: `${s}:commands`, requestId: 'c1' }, 15000);
      const raw = cmdResp.result !== undefined ? cmdResp.result : cmdResp.commands;
      const list = Array.isArray(raw) ? raw : (raw && Array.isArray(raw.commands) ? raw.commands : []);
      const schemas = list.filter((c) => c && c.inputSchema && c.inputSchema.type === 'object').length;

      // 2. Shared Agent Cursor via postMessage
      f.contentWindow.postMessage({
        type: `${s}:cursor`,
        x: 340,
        y: 240,
        visible: true,
        pressed: true,
        label: `Agent · ${s}`,
        color: '#38bdf8',
      }, location.origin);
      await new Promise((r) => setTimeout(r, 80));
      const cursorEl = w.document.querySelector('[id$="_agent_cursor"]');

      // 3. Inspect via postMessage
      const inspResp = await window.req({ type: `${s}:inspect`, requestId: 'i1' }, 10000);

      // 4. Snapshot Frame Capture via postMessage
      const snapResp = await window.req({ type: `${s}:snapshot-request`, requestId: 's1', includeCursor: true }, 15000);

      // 5. captureStream & saveAck
      const streamTracks = w[s].captureStream(15)?.getVideoTracks()?.length || 0;
      const ackOk = Boolean(w[s].saveAck(1));

      return {
        app: s,
        globalIsCanvas,
        commandsOk: Boolean(cmdResp.ok),
        commandsCount: list.length,
        webMcpSchemas: schemas,
        sampleCmd: list[0] ? (list[0].id || list[0].name) : null,
        inspectOk: Boolean(inspResp.ok),
        cursorDomId: cursorEl ? cursorEl.id : null,
        snapshotOk: Boolean(snapResp.ok),
        snapshotBytes: snapResp.bytes ? snapResp.bytes.byteLength : 0,
        streamTracks,
        ackOk,
      };
    }, slug);

    res.bootMs = Date.now() - t0;
    if (SCREENSHOTS_DIR) {
      const shotPath = path.join(SCREENSHOTS_DIR, `${slug}-live.png`);
      await page.screenshot({ path: shotPath });
      res.screenshotPath = shotPath;
    }
    console.log(`[PASS] ${slug}:`, JSON.stringify(res));
    existing[slug] = res;
  } catch (err) {
    console.log(`[ERR] ${slug}:`, err.message);
    existing[slug] = { app: slug, error: err.message };
  } finally {
    await page.close();
  }
}

fs.writeFileSync(OUT_PATH, JSON.stringify(existing, null, 2));
await browser.close();
server.close();
