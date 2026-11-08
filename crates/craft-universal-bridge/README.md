# `craft-universal-bridge`

Single-file universal Rust bridge (`src/universal_bridge.rs` / `src/lib.rs`) for all 16 ArtCraft WebAssembly applications:

- `photocraft`, `vectorcraft`, `lightcraft`, `pdfcraft`, `effectcraft`, `designcraft`, `figcraft`, `filmcraft`, `texturecraft`, `materialcraft`, `wordcraft`, `soundcraft`, `deckcraft`, `cadcraft`, `gridcraft`, `craft-anything`

## Features

1. **Unified Host Frame + Page JSON Control Interface**: Every engine and UI command (`request`, `execute`, `commands`, `docInspect`, `uiInspect`, `pointer`, `screenshot`) is callable both via `window.<app>` (`window.craftUniversalBridge`) and via same-origin `postMessage` / `CustomEvent` messages alongside file I/O (`open`, `save`, `saved`/`saveAck`, `dirty`).
2. **16-App Dialect Normalization**: Automatically maps `execute(command, params)`, `commands()`, and `docInspect()` across `engine.execute`/`engine.commands`/`document.inspect`, `command.run`/`command.list`/`session.inspect` (`texturecraft`, `materialcraft`), `ui.command`/`ui.commands`/`ui.state` (`pdfcraft`), and `triage::look` (`craft-anything`).
3. **Revision-Gated Save Confirmation**: Implements `RevisionGate` so a document stays marked dirty while an async host upload is in flight until `saved { revision }` / `saveAck(revision)` confirms persistence of that revision.
4. **Shared Agent Cursor & Dual-Path Frame Capture**: Renders a non-blocking DOM reticle (`#<canvas>_agent_cursor`), composites cursor reticles onto RGBA framebuffers, captures frames via `ui.screenshot` with `HTMLCanvasElement.toDataURL("image/png")` fallback, and exposes `canvas.captureStream(fps)`.
