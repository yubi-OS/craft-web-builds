# Patch Series Manifest (`yubi-OS/craft-web-builds`)

Tracks the unified library patch series and universal Rust insert across `storytold/craft-libs` and all 16 `storytold/*` WebAssembly applications.

## Root Library Series (`storytold/craft-libs` & Universal Insert)

| # | Patch File | Target | Base Commit | Effect |
|---|---|---|---|---|
| `0001` | `patches/0001-craft-libs-universal-web-bridge.patch` ✅ AUTHORED | `storytold/craft-libs` (`Cargo.toml`, `crates/craft-universal-bridge/**`) | `fd5e2feff6ea25bed371a99cc732b3968d621505` | Rebases and widens `storytold/photocraft#1614`, `#9593`, `#3101`, `#6450`, and `craft-libs` (`craft-web-host` + `craft-web-api`) into `craft-universal-bridge`: same-origin host-frame JSON control channel (`request`, `execute`, `commands` with WebMCP `inputSchema`, `inspect`, `pointer`, `cursor`), revision-acked saves (`saved`/`saveAck`), shared DOM/RGBA cursor overlay, and dual-path frame capture (`ui.screenshot` + `<canvas>` readback + `captureStream`). |
| `0002` | `patches/0002-universal-rust-insert-all-16-apps.patch` ✅ AUTHORED | All 16 `storytold/<app>` web crates (`apps/<app>-web/src/universal_bridge.rs` + entry hooks) | See table below | Inserts `universal_bridge.rs` into all 16 `apps/<app>-web` crates and wires both `window.<app>` and the same-origin host frame (`?host=parent`, `?embed`, `?host=<origin>`, `data-host="page"`) to every app's `ControlRequest` channel and file I/O hooks. |

## Per-App Patch Members (`patches/apps/<app>.patch`)

| # | App ID | Patch File | Upstream Repo & Base SHA | Control Channel & File Hook Wiring |
|---|---|---|---|---|
| `0002.01` | `photocraft` | `patches/apps/photocraft.patch` | `storytold/photocraft` @ `aae9b7ea6119` | `PhotocraftApp::with_control(rx)` (`photocraft_ui_egui::ControlRequest`) + `Services::{write, inbox}` |
| `0002.02` | `vectorcraft` | `patches/apps/vectorcraft.patch` | `storytold/vectorcraft` @ `0a702cc0a5f3` | Bridges `window.vectorcraft` (`vectorcraft_ui_egui::ControlRequest`) + host-frame file/save/snapshot/cursor |
| `0002.03` | `lightcraft` | `patches/apps/lightcraft.patch` | `storytold/lightcraft` @ `b3b49987d4b2` | `LightcraftApp` (`lightcraft_ui_egui::ControlRequest` + `app.run`) + host-frame JSON & snapshot bridge |
| `0002.04` | `pdfcraft` | `patches/apps/pdfcraft.patch` | `storytold/pdfcraft` @ `adbf1dc01fef` | Enables `PdfCraftApp::attach_control` (`ui.command`, `ui.commands`, `ui.state`, `ui.screenshot`) in host mode + universal bridge |
| `0002.05` | `effectcraft` | `patches/apps/effectcraft.patch` | `storytold/effectcraft` @ `3e0c77ca6860` | Bridges `EffectcraftApp` (`effectcraft_ui_egui::ControlRequest` + `files::STORE`) to host-frame JSON & snapshot bridge |
| `0002.06` | `designcraft` | `patches/apps/designcraft.patch` | `storytold/designcraft` @ `d848948b112d` | `DesignApp::with_control(rx)` (`designcraft_ui_egui::ControlRequest`) + `Services::{write, download, inbox}` |
| `0002.07` | `figcraft` | `patches/apps/figcraft.patch` | `storytold/figcraft` @ `3635c2307099` | Bridges `FigApp` (`figcraft_ui_egui::ControlRequest`) + unconditional host-mode JSON control & frame capture |
| `0002.08` | `filmcraft` | `patches/apps/filmcraft.patch` | `storytold/filmcraft` @ `4a56be9cff41` | Bridges `FilmcraftApp::with_control(rx)` (`filmcraft_ui_egui::ControlRequest`) + `fs::WebServices` to host frame |
| `0002.09` | `texturecraft` | `patches/apps/texturecraft.patch` | `storytold/texturecraft` @ `c46e9b3f3c97` | `TextureCraftApp.control = Some(rx)` (`command.run`, `command.list`, `session.inspect`, `ui.screenshot`) + `web_files` |
| `0002.10` | `materialcraft` | `patches/apps/materialcraft.patch` | `storytold/materialcraft` @ `7019d44f95ca` | `MaterialCraftApp.control = Some(rx)` (`command.run`, `command.list`, `session.inspect`, `ui.input`, `ui.screenshot`) + `web_files` |
| `0002.11` | `wordcraft` | `patches/apps/wordcraft.patch` | `storytold/wordcraft` @ `04217419b67e` | Bridges `WordApp::with_control(rx)` (`wordcraft_ui_egui::ControlRequest`) + host-frame JSON & snapshot bridge |
| `0002.12` | `soundcraft` | `patches/apps/soundcraft.patch` | `storytold/soundcraft` @ `76cc2aeac657` | `SoundApp::with_control(rx)` (`soundcraft_ui_egui::ControlRequest`) + `soundcraft_engine::io::set_deliver` |
| `0002.13` | `deckcraft` | `patches/apps/deckcraft.patch` | `storytold/deckcraft` @ `02fbb6a976e7` | Bridges `SlideApp` (`deckcraft_ui_egui::ControlRequest`) + host-frame JSON & snapshot bridge |
| `0002.14` | `cadcraft` | `patches/apps/cadcraft.patch` | `storytold/cadcraft` @ `7a9ec9bae323` | Bridges `CadApp::with_control(rx)` (`cadcraft_ui_egui::ControlRequest`) + host-frame JSON & snapshot bridge |
| `0002.15` | `gridcraft` | `patches/apps/gridcraft.patch` | `storytold/gridcraft` @ `640fbf22f64e` | Bridges `SheetApp` (`gridcraft_ui_egui::ControlRequest`) + host-frame JSON & snapshot bridge |
| `0002.16` | `craft-anything` | `patches/apps/craft-anything.patch` | `storytold/craft-anything` @ `56aa31976b4f` | Wires `Viewer` + `triage::look(&name, &bytes)` to `engine.commands`, `engine.execute` (`file.triage`/`file.open`), `document.inspect`, and frame capture |
