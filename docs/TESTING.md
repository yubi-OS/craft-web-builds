# Testing & Puppeteer Verification Guide (`docs/TESTING.md`)

This document describes the three-layer verification pipeline for **`yubi-OS/craft-web-builds`**:
1. **Layer 1 — Native Rust Unit Tests, Strict Clippy Gate & `wasm32` Target Check** (`crates/craft-universal-bridge`)
2. **Layer 2 — 16-App Patch & AST Wiring Verification** (`scripts/apply-universal-bridge.sh` & `scripts/wire_universal_bridge.py`)
3. **Layer 3 — End-to-End Headless Chrome + Puppeteer WASM Binary Verification** (`scripts/puppeteer/*`)

---

## 1. Layer 1: Rust Unit Tests, Clippy & `wasm32` Check

Every upstream `storytold/*` repository enforces `#![forbid(unsafe_code)]` and `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`. Run the native unit tests and target checks before any build:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown
```

What the unit tests in `crates/craft-universal-bridge/src/universal_bridge.rs` cover:
- All 16 `AppProfile` entries (`ALL_APPS`) have distinct IDs, canvas IDs, and valid method tables.
- Dialect adaptation between `engine.execute {command, params}`, `ui.command {id, params}`, and `command.run {id, params}`.
- Host mode detection (`?host=parent`, `?embed`, `?host=https://...`, `data-host="page"`) and origin validation (including rejection of `"null"` opaque origins and non-localhost `http://`).
- Revision-gated `RevisionGate` dirty tracking (`begin_save`, `record_edit`, `confirm_saved`).
- Filename sanitization (stripping path components, null/control characters, and enforcing fallback filenames).
- Pure-Rust RGBA8 shared agent cursor reticle compositing (`CursorState::composite_rgba`) and base64 decoding (`base64_decode`).
- WebMCP `inputSchema` JSON Schema synthesis on both flat `result: [...]` and nested `result: { commands: [...] }` command replies.

---

## 2. Layer 2: Applying the Universal Bridge to an Upstream Checkout

To apply `universal_bridge.rs` and wire `Cargo.toml` + `src/` on any of the 16 `storytold/<app>` checkouts:

```bash
bash scripts/apply-universal-bridge.sh <app-id> /path/to/<app>-checkout
```

`scripts/wire_universal_bridge.py` performs deterministic TOML (`tomllib`) and Rust source wiring:
- Ensures `serde` (with `features = ["derive"]`), `serde_json`, `wasm-bindgen`, `wasm-bindgen-futures`, and `js-sys` are present in `apps/<app>-web/Cargo.toml` using `{ workspace = true }` whenever the root workspace defines them (so `Cargo.lock` stays untouched).
- Inserts `#[cfg(any(target_arch = "wasm32", test))] pub mod universal_bridge;` **after** all `#![...]` inner attributes and top-of-file `//!` / `//` comments (preventing `rustc` E0753 on crates like `craft-anything-web`).
- Wires `UniversalBridge::install_with_channel` or `UniversalBridge::install_auto` plus `ubridge.set_waker(...)` and `ubridge.post_ready(...)` into each app's `eframe` creation closure.

---

## 3. Layer 3: Headless Chrome + Puppeteer Verification (`scripts/puppeteer/`)

All scripts used during our live verification sessions are checked into [`scripts/puppeteer/`](../scripts/puppeteer/):

| Script | Purpose |
|---|---|
| [`scripts/puppeteer/verify-suite.mjs`](../scripts/puppeteer/verify-suite.mjs) | Automated 16-app Puppeteer verification runner. Boots each compiled `.wasm` binary inside a same-origin host `<iframe>`, verifies `<app>:ready`, `window.craftUniversalBridge`, non-DOM-shadowed `window.<app>`, `<app>:commands` + WebMCP `inputSchema`s, `<app>:cursor` (`#<canvas_id>_agent_cursor`), `<app>:inspect`, `<app>:snapshot-request` (`ArrayBuffer` + base64 PNG), `captureStream(15)`, and `saveAck(1)`. |
| [`scripts/puppeteer/visual-hud-harness.mjs`](../scripts/puppeteer/visual-hud-harness.mjs) | Split-pane `1280×800` visual HUD host harness. Renders the live WASM application on the left and a real-time protocol/command/inspect/snapshot telemetry HUD on the right, executes sample commands (including `file.new` on `photocraft`, `pdfcraft:open` with a synthetic PDF on `pdfcraft`, and `file.open` with ELF bytes on `craft-anything`), and saves full-page PNG screenshots. |
| [`scripts/puppeteer/extract-commands-catalog.mjs`](../scripts/puppeteer/extract-commands-catalog.mjs) | Boots all 16 compiled WASM binaries in Puppeteer and extracts every command (`8,835` commands total with WebMCP `inputSchema`s) plus `bridgeMethods`, `docInspectKeys`, and `uiInspectKeys` into [`docs/COMMANDS_CATALOG.json`](./COMMANDS_CATALOG.json). |

### 3.1 Required Headless Chrome Flags for WebGL2 / `egui_wgpu`

All 16 ArtCraft web apps render via `eframe` + `egui_wgpu` (WebGPU or WebGL2). In a headless Linux environment without a physical GPU, Chrome must be launched with SwiftShader ANGLE WebGL2 enabled:

```js
const browser = await puppeteer.launch({
  executablePath: '/usr/local/bin/chrome-headless-shell',
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
```

And the embedded app `<iframe>` URL should include:
```text
/<app>/index.html?host=parent&control&webgl&store=memory&workers=0&noworkers&nosw&empty
```
- `?host=parent`: activates same-origin parent `postMessage` framing.
- `?control`: enables upstream `window.<app>` control channel where gated.
- `?webgl`: forces `eframe::wgpu::Backends::GL` (WebGL2) instead of probing for WebGPU first.
- `?store=memory&workers=0&noworkers&nosw&empty`: keeps storage in-memory and runs rendering on the main thread so single-core headless runners do not spawn multiple background Web Workers that each recompile 50+ MB `.wasm` modules.

### 3.2 Running the 16-App Verification Suite

1. Unpack the `web-<app>` artifacts (or `wasm-suite-v0.1.0.zip`) into a directory `/tmp/wasm-suite`:
   ```bash
   mkdir -p /tmp/wasm-suite
   unzip -q wasm-suite-v0.1.0.zip -d /tmp/wasm-suite
   for z in /tmp/wasm-suite/*-web-*.zip; do
     app="${z##*/}"
     app="${app%%-web-*}"
     mkdir -p "/tmp/wasm-suite/${app}-pkg/site"
     unzip -q -o "$z" -d "/tmp/wasm-suite/${app}-pkg/site"
   done
   ```
2. Run the full 16-app verification sweep:
   ```bash
   node scripts/puppeteer/verify-suite.mjs \
     --root=/tmp/wasm-suite \
     --out=/tmp/wasm-suite/v2-results.json \
     --screenshots-dir=/tmp/wasm-suite/screenshots
   ```
3. Or run the visual split-pane HUD harness on specific apps:
   ```bash
   node scripts/puppeteer/visual-hud-harness.mjs \
     --root=/tmp/wasm-suite \
     --out-dir=/tmp/wasm-suite/hud-screenshots \
     photocraft pdfcraft figcraft craft-anything vectorcraft effectcraft
   ```
4. Or regenerate `docs/COMMANDS_CATALOG.json`:
   ```bash
   node scripts/puppeteer/extract-commands-catalog.mjs \
     --root=/tmp/wasm-suite \
     --out=docs/COMMANDS_CATALOG.json
   ```

---

## 4. Runtime & Build Lessons Caught by Puppeteer Verification

Testing the actual compiled `.wasm` binaries in Puppeteer uncovered eight concrete issues that static compilation could not catch, all of which are fixed in `crates/craft-universal-bridge` and `scripts/wire_universal_bridge.py`:

1. **HTML5 `<canvas id="<app>">` Named-Window Access Shadowing (`pdfcraft`):**
   When `<canvas id="pdfcraft">` matches `window.pdfcraft`, HTML5 named property access on `window` returns the `HTMLCanvasElement` DOM node before JS globals are defined. Mutating that object in place and setting `.captureStream` shadowed `HTMLCanvasElement.prototype.captureStream` on the `<canvas>` itself, causing a recursive `FnMut` trap (`closure invoked recursively or after being dropped`) when `captureStream(15)` ran.
   - **Fix:** `install_window_api` checks `nodeType` on any existing `window.<app>` object, copies any function properties onto a fresh `js_sys::Object`, and uses `Reflect::define_property` on `window` (`globalIsCanvas: false`), while `canvas_capture_stream` resolves `captureStream` from `Object::get_prototype_of(&canvas)` first.
2. **Automatic `eframe` Repaint Wake Loop (`call_control`):**
   On the web, `eframe` sleeps between input events unless `egui_ctx.request_repaint()` or a DOM event wakes the canvas, and `ui.screenshot` requires **two** frames (Frame 1 queues `ViewportCommand::Screenshot`; Frame 2 receives `Event::Screenshot` and PNG-encodes the image).
   - **Fix:** `UniversalBridge::call_control` maintains an `in_flight` counter and runs a `spawn_local` loop every `16ms` that invokes both the registered `egui_ctx.request_repaint()` waker and `wake_canvas` (alternating pointer coordinates so `eframe` does not coalesce zero-delta mousemoves) until all in-flight requests resolve.
3. **Nested `result.commands` WebMCP `inputSchema` Synthesis (`pdfcraft`):**
   While 15 apps return their command array directly under `result: [...]`, `pdfcraft`'s `ui.commands` returns `result: { commands: [...] }`.
   - **Fix:** `enrich_commands_with_schema` inspects both `result` (array) and `result.commands` (nested array).
4. **`document.inspect` Engine-Command & Start-Screen Fallback (`photocraft`, `designcraft`, `wordcraft`, `cadcraft`):**
   In `photocraft`, `document.inspect` is an engine command (`engine.execute {"command": "document.inspect"}`) rather than a top-level control method, and before a document is opened, document inspection returns `"no document open"`.
   - **Fix:** `UniversalBridge::call_doc_inspect` retries via `format_execute_params` when an `"unknown method"` / `"engine command"` error is returned, and falls back to `ui.inspect` when no document is open yet.
5. **Two-Stage Boot Loader Subpath `app/snippets/` Resolution (`photocraft`):**
   `photocraft` builds a two-stage loader (`photocraft-web-boot` -> `app/photocraft-web.js`), and `app/photocraft-web.js` imports `./snippets/eframe-.../inline0.js` relative to `app/`. When served under `/photocraft/index.html`, `/photocraft/app/snippets/...` 404s unless `snippets/` is also copied into `app/snippets/`.
   - **Fix:** Both `craft-web-builds.yml` and `verify-suite.mjs` copy `snippets/` into `app/snippets/` when `app/` is present.
6. **`vectorcraft` & `figcraft` `if let Err(e) = crate::api::install()` Placement:**
   Upstream wraps `crate::api::install()` in `if let Err(e) = crate::api::install() { ... }` (and `figcraft` gated `api::connect` on `?control`), which caused a naive line-after anchor insert to place `UniversalBridge::install_auto` inside the `Err` branch.
   - **Fix:** `wire_universal_bridge.py` wires `vectorcraft` and `figcraft` unconditionally outside the `if let Err` block.
7. **`effectcraft` `src/app.rs` Entry & `?webgl` Worker Gating:**
   `effectcraft` defines `EffectcraftApp::new` and `api::set_sender(tx)` in `src/app.rs` (not `src/lib.rs` or `src/web.rs`).
   - **Fix:** `wire_universal_bridge.py` patches `src/app.rs` to wire `UniversalBridge::install_auto("effectcraft")`, honor `?webgl`, and default `frameworkers` to `0` under `?webgl`.
8. **`lightcraft` `LightcraftApp::with_control(ctrl_rx)` Wiring:**
   Upstream `lightcraft-web` only wired its legacy `INBOX` (`lightcraft.command(id, params)`), leaving `LightcraftApp::with_control` unconnected on the web.
   - **Fix:** `wire_universal_bridge.py` connects `LightcraftApp::with_control(ctrl_rx)` to `UniversalBridge::install_with_channel("lightcraft", ...)` via `lightcraft_ui_egui::ControlRequest::new`.

---

## 5. Verified 16-App Puppeteer Results (`v0.1.0`)

| App | `globalIsCanvas` | `commandsOk` | Commands | WebMCP Schemas | `inspectOk` | `cursorDomId` | `snapshotOk` | Snapshot Bytes | `streamTracks` | `ackOk` |
|---|---|---|---|---:|---:|---|---|---|---:|---:|---|
| `photocraft` | `false` | `true` | 1,003 | 1,003 | `true` | `photocraft_canvas_agent_cursor` | `true` | 17,405 | 1 | `true` |
| `vectorcraft` | `false` | `true` | 1,373 | 1,373 | `true` | `vectorcraft_canvas_agent_cursor` | `true` | 23,073 | 1 | `true` |
| `lightcraft` | `false` | `true` | 595 | 595 | `true` | `lightcraft_canvas_agent_cursor` | `true` | 17,405 | 1 | `true` |
| `pdfcraft` | `false` | `true` | 217 | 217 | `true` | `pdfcraft_agent_cursor` | `true` | 120,795 | 1 | `true` |
| `effectcraft` | `false` | `true` | 761 | 761 | `true` | `effectcraft_canvas_agent_cursor` | `true` | 76,823 | 1 | `true` |
| `designcraft` | `false` | `true` | 745 | 745 | `true` | `designcraft_canvas_agent_cursor` | `true` | 17,405 | 1 | `true` |
| `figcraft` | `false` | `true` | 641 | 641 | `true` | `figcraft_canvas_agent_cursor` | `true` | 195,175 | 1 | `true` |
| `filmcraft` | `false` | `true` | 717 | 717 | `true` | `filmcraft_canvas_agent_cursor` | `true` | 17,405 | 1 | `true` |
| `texturecraft` | `false` | `true` | 173 | 173 | `true` | `texturecraft_canvas_agent_cursor` | `true` | 18,212 | 1 | `true` |
| `materialcraft` | `false` | `true` | 93 | 93 | `true` | `materialcraft_canvas_agent_cursor` | `true` | 161,740 | 1 | `true` |
| `wordcraft` | `false` | `true` | 660 | 660 | `true` | `wordcraft_canvas_agent_cursor` | `true` | 98,707 | 1 | `true` |
| `soundcraft` | `false` | `true` | 553 | 553 | `true` | `soundcraft_canvas_agent_cursor` | `true` | 96,968 | 1 | `true` |
| `deckcraft` | `false` | `true` | 383 | 383 | `true` | `deckcraft_canvas_agent_cursor` | `true` | 17,405 | 1 | `true` |
| `cadcraft` | `false` | `true` | 516 | 516 | `true` | `cadcraft_canvas_agent_cursor` | `true` | 62,886 | 1 | `true` |
| `gridcraft` | `false` | `true` | 403 | 403 | `true` | `gridcraft_canvas_agent_cursor` | `true` | 17,405 | 1 | `true` |
| `craft-anything` | `false` | `true` | 2 | 2 | `true` | `craft_anything_canvas_agent_cursor` | `true` | 23,073 | 1 | `true` |
| **Total (16/16)** | **`false`** | **`true`** | **8,835** | **8,835** | **`true`** | **16 / 16 verified** | **`true`** | **16 / 16 PNGs** | **16 / 16** | **`true`** |
