#!/usr/bin/env node
/**
 * 16-App Command & Inspect Schema Extractor for yubi-OS/craft-web-builds.
 *
 * Launches each unpacked ArtCraft WebAssembly build in headless Chrome via Puppeteer,
 * queries `<app>:commands`, `<app>:inspect` (`docInspect`), and `<app>:inspect` (`target: "ui"`),
 * and writes `docs/COMMANDS_CATALOG.json` (used to generate `AGENTS.md`).
 *
 * Usage:
 *   node scripts/puppeteer/extract-commands-catalog.mjs [--root=/path/to/unpacked] [--out=docs/COMMANDS_CATALOG.json]
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

const APPS = [
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
for (const arg of process.argv.slice(2)) {
  if (arg.startsWith('--')) {
    const [k, v] = arg.slice(2).split('=');
    flags[k] = v ?? 'true';
  }
}

const ROOT = path.resolve(flags.root || process.env.CRAFT_SUITE_ROOT || '.');
const OUT_PATH = path.resolve(flags.out || 'docs/COMMANDS_CATALOG.json');
const CHROME_PATH = flags.chrome || process.env.CHROME_PATH || '/usr/local/bin/chrome-headless-shell';

const DIRS = {};
const META = {};
for (const slug of APPS) {
  const pkgRoot = path.join(ROOT, `${slug}-pkg`);
  const metaPath = path.join(pkgRoot, `${slug}.json`);
  if (fs.existsSync(metaPath)) {
    META[slug] = JSON.parse(fs.readFileSync(metaPath, 'utf-8'));
  }
  const sr = path.join(pkgRoot, 'site');
  if (fs.existsSync(sr)) {
    const sub = fs.readdirSync(sr).find((d) => fs.statSync(path.join(sr, d)).isDirectory());
    if (sub) DIRS[slug] = path.join(sr, sub);
  }
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
        window.pending = new Map();
        window.addEventListener('message', (ev) => {
          const d = ev.data;
          if (!d || typeof d !== 'object') return;
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
    if (DIRS[slug] && u.pathname.startsWith(`/${slug}/`)) {
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

const catalog = {};

for (const slug of APPS) {
  if (!DIRS[slug]) continue;
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
    await new Promise((r) => setTimeout(r, 700));

    const data = await page.evaluate(async (s) => {
      const f = document.getElementById('f');
      const w = f.contentWindow;
      const bridge = w.craftUniversalBridge;
      const appObj = w[s] || bridge;

      const cmdResp = await window.req({ type: `${s}:commands`, requestId: 'c1' }, 15000);
      const raw = cmdResp.result !== undefined ? cmdResp.result : cmdResp.commands;
      const list = Array.isArray(raw) ? raw : (raw && Array.isArray(raw.commands) ? raw.commands : []);

      const docInsp = await window.req({ type: `${s}:inspect`, requestId: 'i1' }, 10000);
      const uiInsp = await window.req({ type: `${s}:inspect`, requestId: 'i2', target: 'ui' }, 10000);

      return {
        bridgeMethods: Object.keys(bridge).sort(),
        appGlobalMethods: Object.keys(appObj).sort(),
        docInspectKeys: docInsp.result && typeof docInsp.result === 'object' ? Object.keys(docInsp.result).sort() : [],
        uiInspectKeys: uiInsp.result && typeof uiInsp.result === 'object' ? Object.keys(uiInsp.result).sort() : [],
        commands: list,
      };
    }, slug);

    catalog[slug] = {
      meta: META[slug] || { app: slug },
      ...data,
    };
    console.log(`[EXTRACTED] ${slug}: ${data.commands.length} commands, bridgeMethods=${data.bridgeMethods.length}, uiInspectKeys=${data.uiInspectKeys.length}`);
  } finally {
    await page.close();
  }
}

fs.mkdirSync(path.dirname(OUT_PATH), { recursive: true });
fs.writeFileSync(OUT_PATH, JSON.stringify(catalog, null, 2));
await browser.close();
server.close();
console.log(`Saved ${OUT_PATH}`);
