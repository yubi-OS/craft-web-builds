# `yubi-OS/craft-web-builds`

Aggregator repository housing the CI matrix workflow, unified library patch series, and universal Rust bridge insert (`craft-universal-bridge`) for building all 16 [ArtCraft](https://getartcraft.com/apps) WebAssembly applications into static-host-ready packages (Cloudflare Pages / Workers Static Assets compatible, `<= 24 MiB` per file).

## Repository Layout

- [`.github/workflows/craft-web-builds.yml`](./.github/workflows/craft-web-builds.yml) — 16-app matrix build workflow + `verify-universal-bridge` native/wasm check + `combine` suite artifact (`wasm-suite-<sha>`) + `attach-release-asset` release uploader.
- [`AGENTS.md`](./AGENTS.md) — Complete agent & host integration guide cataloging all 4 host transport modes, all `window.<app>` / `window.craftUniversalBridge` methods, all 15 incoming & 14 outgoing `postMessage` / `CustomEvent` schemas, `docInspect` / `uiInspect` key schemas, and **all 8,835 commands** across all 16 applications.
- [`PINNED.md`](./PINNED.md) — Pinned upstream commit SHAs and per-app font/i18n build settings.
- `crates/craft-universal-bridge/` — Standalone crate and single-file universal Rust insert ([`src/universal_bridge.rs`](./crates/craft-universal-bridge/src/universal_bridge.rs)) rebasing and widening `storytold/photocraft#1614`, `#9593`, `#3101`, `#6450`, and `storytold/craft-libs` (`craft-web-host` + `craft-web-api`).
- `docs/` — Protocol, testing, and machine-readable command catalog:
  - [`docs/PROTOCOL.md`](./docs/PROTOCOL.md) — Specification of the same-origin host-frame JSON control, file handoff, revision-acked save, shared cursor overlay, and dual-path frame capture protocol.
  - [`docs/TESTING.md`](./docs/TESTING.md) — Three-layer testing guide (native Rust unit tests + Clippy + `wasm32` check, 16-app AST/TOML patch verification, and headless Chrome + SwiftShader WebGL2 Puppeteer verification) plus the 8 runtime/build lessons caught by Puppeteer.
  - [`docs/COMMANDS_CATALOG.json`](./docs/COMMANDS_CATALOG.json) — Full machine-readable JSON catalog (`8,835` commands with WebMCP JSON Schema `inputSchema`, `bridgeMethods`, `docInspectKeys`, and `uiInspectKeys` across all 16 apps).
- `patches/` — Unified patch series:
  - [`patches/SERIES.md`](./patches/SERIES.md) — Patch series manifest and base commit table.
  - [`patches/0001-craft-libs-universal-web-bridge.patch`](./patches/0001-craft-libs-universal-web-bridge.patch) — Shared library patch against `storytold/craft-libs`.
  - [`patches/0002-universal-rust-insert-all-16-apps.patch`](./patches/0002-universal-rust-insert-all-16-apps.patch) — Combined 16-app universal insert patch.
  - `patches/apps/<app>.patch` — Individual per-app patches for all 16 apps.
- `scripts/` — Build, wiring, and Puppeteer verification tooling:
  - [`scripts/apply-universal-bridge.sh`](./scripts/apply-universal-bridge.sh) — Deterministic CI patcher and universal Rust inserter.
  - [`scripts/wire_universal_bridge.py`](./scripts/wire_universal_bridge.py) — TOML (`tomllib`) dependency wirer and per-app `eframe` control-channel / repaint-waker / `post_ready` hook injector.
  - [`scripts/puppeteer/verify-suite.mjs`](./scripts/puppeteer/verify-suite.mjs) — Automated 16-app headless Chrome + Puppeteer end-to-end verification suite.
  - [`scripts/puppeteer/visual-hud-harness.mjs`](./scripts/puppeteer/visual-hud-harness.mjs) — Split-pane `1280×800` visual telemetry HUD harness with synthetic PDF/ELF/document execution and screenshot capture.
  - [`scripts/puppeteer/extract-commands-catalog.mjs`](./scripts/puppeteer/extract-commands-catalog.mjs) — 16-app command & inspect schema extractor that generates `docs/COMMANDS_CATALOG.json`.

## 16-App WebAssembly Suite (`v0.1.0`)

| App | JS Global | Canvas ID | Native Ext | Commands (WebMCP) | Package |
|---|---|---|---|---:|---|
| **PhotoCraft** | `window.photocraft` | `photocraft_canvas` | `.pcraft` | 1,003 | `photocraft-web-0.20.1.zip` |
| **VectorCraft** | `window.vectorcraft` | `vectorcraft_canvas` | `.vcraft` | 1,373 | `vectorcraft-web-0.21.0.zip` |
| **LightCraft** | `window.lightcraft` | `lightcraft_canvas` | `.lcraft` | 595 | `lightcraft-web-0.18.0.zip` |
| **PDFCraft** | `window.pdfcraft` | `pdfcraft` | `.pdf` | 217 | `pdfcraft-web-0.18.1.zip` |
| **EffectCraft** | `window.effectcraft` | `effectcraft_canvas` | `.ecproj` | 761 | `effectcraft-web-0.21.0.zip` |
| **DesignCraft** | `window.designcraft` | `designcraft_canvas` | `.dcraft` | 745 | `designcraft-web-0.17.0.zip` |
| **FigCraft** | `window.figcraft` | `figcraft_canvas` | `.figcraft` | 641 | `figcraft-web-0.15.0.zip` |
| **FilmCraft** | `window.filmcraft` | `filmcraft_canvas` | `.fcproj` | 717 | `filmcraft-web-0.17.0.zip` |
| **TextureCraft** | `window.texturecraft` | `texturecraft_canvas` | `.tcraft` | 173 | `texturecraft-web-0.3.1.zip` |
| **MaterialCraft** | `window.materialcraft` | `materialcraft_canvas` | `.sbs` | 93 | `materialcraft-web-0.1.3.zip` |
| **WordCraft** | `window.wordcraft` | `wordcraft_canvas` | `.docx` | 660 | `wordcraft-web-0.15.1.zip` |
| **SoundCraft** | `window.soundcraft` | `soundcraft_canvas` | `.scraft` | 553 | `soundcraft-web-0.17.0.zip` |
| **DeckCraft** | `window.deckcraft` | `deckcraft_canvas` | `.deckcraft` | 383 | `deckcraft-web-0.15.0.zip` |
| **CADCraft** | `window.cadcraft` | `cadcraft_canvas` | `.dxf` | 516 | `cadcraft-web-0.17.0.zip` |
| **GridCraft** | `window.gridcraft` | `gridcraft_canvas` | `.xlsx` | 403 | `gridcraft-web-0.16.0.zip` |
| **Craft Anything** | `window.craftAnything` | `craft_anything_canvas` | `.bin` | 2 | `craft-anything-web-0.4.0.zip` |
| **Total (16 Apps)** | `window.craftUniversalBridge` | — | — | **8,835** | `wasm-suite-v0.1.0.zip` (`283.63 MiB`) |
