# Universal WebAssembly Host & JSON Control Protocol (`universal/1`)

One universal Rust module (`crates/craft-universal-bridge/src/universal_bridge.rs`) inserted across all 16 ArtCraft WebAssembly builds so a same-origin host page or agent can:
1. Open, edit, export, and revision-confirm documents without browser downloads (`open`, `save`, `saved`/`saveAck`, `dirty`).
2. Trigger **every** engine and UI function via the JSON control channel over the exact same host interface (`request`, `execute`, `commands`, `inspect`, `pointer`).
3. Drive a shared visual agent cursor (`cursor`, `pointer`, `setCursor`) and capture live frames (`snapshotRequest`, `captureFrame`, `captureStream`).

---

## 1. Enabling Host Mode

| Mode | Trigger | Transport | Origin Enforcement |
|---|---|---|---|
| **Parent iframe** | `?host=parent` | `window.parent.postMessage` | Requires `window.parent !== window` and `parent.location.origin === window.location.origin` |
| **Embed iframe** | `?embed` or `?embed=1` | `window.parent.postMessage` | Same-origin parent check; sets `photocraftEmbed: 1` and `<app>Embed: 1` on outgoing messages; enables `require_ack` revision gating |
| **Explicit origin** | `?host=https://host[:port]` | `window.parent.postMessage` | Validates `event.origin === expected_origin` and `event.source === window.parent`; never posts to `"*"` |
| **Same-document canvas** | `data-host="page"` on `<canvas>` | Non-bubbling DOM `CustomEvent` on `<canvas>` | Same document |

Additionally, `window.<app>` and `window.craftUniversalBridge` are always installed in the frame for direct same-origin invocation (`iframe.contentWindow.<app>`).

---

## 2. Host Frame Message Protocol (`postMessage` & `CustomEvent`)

### Host → Editor

| `type` | Fields | Action |
|---|---|---|
| `<app>:host` / `hello` / `ping` | `version?` | Replies with `<app>:ready` handshake |
| `<app>:open` | `name`, `bytes` (`ArrayBuffer` / `Uint8Array`), `fonts?`, `requestId?` | Validates size (`<= 512 MiB`), sanitizes filename, registers optional fonts, opens document, replies `<app>:opened` |
| `<app>:save` / `<app>:export` | `format?`, `requestId?` | Runs `File › Save` or format export; replies `<app>:save` with transferred `ArrayBuffer` `bytes` and `revision` |
| `<app>:saved` / `saveAck` | `revision`, `id?` | Confirms persistence of `revision`; clears `dirty` only if no newer edits occurred while upload was in flight |
| `<app>:request` | `requestId`, `method`, `params` | Executes **any** JSON control method (`engine.execute`, `engine.commands`, `document.inspect`, `session.inspect`, `ui.inspect`, `ui.set`, `ui.menu.list`, `ui.menu.invoke`, `ui.pointer`, `ui.click`, `ui.move`, `ui.drag`, `ui.key`, `ui.type`, `ui.dialog.*`, `ui.screenshot`, etc.) and replies `<app>:response` |
| `<app>:execute` / `<app>:cmd` | `requestId`, `command` (or `id`), `params` | Normalizes payload for the app's native execute method (`engine.execute`, `command.run`, or `ui.command`), runs with one undo step, increments `revision`, replies `<app>:response` |
| `<app>:commands` | `requestId`, `filter?` | Returns the full command catalog enriched with WebMCP JSON Schema `inputSchema` |
| `<app>:inspect` | `requestId`, `target?: "document" \| "ui"`, `params?` | Returns document/session or UI state |
| `<app>:pointer` | `requestId`, `events`, `x?`, `y?`, `label?`, `color?`, `space?` | Updates shared agent cursor overlay and dispatches synthetic pointer/mouse/keyboard input |
| `<app>:cursor` | `x`, `y`, `visible?`, `normalized?`, `label?`, `color?`, `pressed?` | Moves/updates the non-blocking DOM cursor overlay `#<canvas>_agent_cursor` |
| `<app>:snapshot-request` / `<app>:screenshot` | `requestId`, `includeCursor?`, `scale?`, `view?`, `params?` | Captures frame via `ui.screenshot` (with `<canvas>` readback fallback), composites shared cursor when requested, replies `<app>:snapshot` with both base64 `data` and transferred `ArrayBuffer` `bytes` |

### Editor → Host

| `type` | Fields | Meaning |
|---|---|---|
| `<app>:ready` | `version: "1"`, `bridgeVersion: "universal/1"`, `app`, `appVersion`, `formatExtension`, `rev`, `capabilities` | Editor initialized and listening |
| `<app>:opened` | `requestId?`, `name`, `ok: true`, `warnings`, `revision` | Document opened |
| `<app>:save` | `requestId?`, `name`, `saveAs`, `revision`, `bytes` (transferred `ArrayBuffer`) | Saved/exported document bytes |
| `<app>:dirty` | `dirty`, `revision` | Unsaved-changes state transition |
| `<app>:response` | `requestId`, `method`, `command`, `ok`, `result`, `error`, `revision` | Reply to `request`, `execute`, `commands`, `inspect`, `pointer` |
| `<app>:snapshot` | `requestId`, `ok`, `format: "png"`, `width`, `height`, `data` (base64), `bytes` (transferred `ArrayBuffer`), `cursor`, `revision` | Captured frame |
| `<app>:failed` / `<app>:open-failed` / `<app>:save-failed` | `error`, `id?` | Startup, open, or save error |
