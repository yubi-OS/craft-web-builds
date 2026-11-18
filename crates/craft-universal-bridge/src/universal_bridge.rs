#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

//! Universal WebAssembly Host Bridge, JSON Control Interface, Shared Cursor Overlay,
//! and Frame Capture Mechanism for all 16 ArtCraft web applications.
//!
//! Rebases, extends, and unifies:
//! - `storytold/photocraft#1614` (`hafezparast/stucent` `emu/1` `host_bridge.rs` + `bridge_protocol.rs`,
//!   `LiamVisionary/Hivemind` `open`/`export`/`state`/`commands`/`autosave`/`exam`, and `SamTanna` suite-wide contract)
//! - `storytold/photocraft#9593` (`hlvs-apps:upstream/host-bridge` `embed.rs` with revision-acked `saved { revision }`,
//!   host-initiated `save`, `open` with font pre-registration, `?lang=<tag>`, and `ready` format/commit metadata)
//! - `storytold/photocraft#3101` (`?host=parent` `pick-save` / `picked`, `open`, `save`, `dirty`, `ready`, `failed`)
//! - `storytold/photocraft#6450` (`window.<app>` unified control channel + WebMCP JSON Schema `inputSchema` on `commands()`)
//! - `storytold/craft-libs` (`craft-web-host` #358 + `craft-web-api` #413 across `?host=parent` and `data-host="page"`)
//!
//! Designed as a single universal Rust insert (`universal_bridge.rs`) that compiles inside every
//! `apps/<app>-web` crate across all 16 apps without requiring any additional `web-sys` feature flags
//! or external git dependencies beyond what is already locked in each app's `Cargo.lock`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Protocol version reported in `ready` for `craft-web-host` compatibility.
pub const PROTOCOL_VERSION: &str = "1";

/// Universal bridge identifier carried in `bridge` / `craftBridge` fields.
pub const BRIDGE_ID: &str = "universal/1";

/// Legacy stucent/Hivemind protocol tag accepted on incoming messages (#1614).
pub const EMU_PROTOCOL_ID: &str = "emu/1";

/// Maximum file name length in characters.
pub const MAX_NAME_CHARS: usize = 255;

/// Maximum language tag length in bytes (BCP 47, e.g. `zh-Hant-TW`).
pub const MAX_LANG_BYTES: usize = 35;

/// Maximum open payload size accepted from a host frame (512 MiB).
pub const MAX_OPEN_BYTES: u64 = 512 << 20;

/// Minimum autosave snapshot interval in milliseconds (5 seconds).
pub const MIN_AUTOSAVE_MS: u32 = 5_000;

/// Maximum autosave snapshot interval in milliseconds (10 minutes).
pub const MAX_AUTOSAVE_MS: u32 = 600_000;

/// Maximum characters of `params` JSON forwarded per journal entry.
pub const MAX_JOURNAL_PARAM_CHARS: usize = 512;

/// All capabilities advertised by the universal bridge in `ready.capabilities`.
pub const CAPABILITIES: &[&str] = &[
    "open",
    "save",
    "saveAck",
    "export",
    "pickSave",
    "dirty",
    "state",
    "request",
    "execute",
    "commands",
    "docInspect",
    "uiInspect",
    "pointer",
    "cursor",
    "snapshot",
    "captureStream",
    "journal",
    "autosave",
    "exam",
    "fonts",
    "lang",
    "handoff",
    "import",
    "importFile",
    "openFile",
    "loadHold",
    "readyNotify",
    "pointerClick",
];

/// The control-channel method names backing each unified function for an app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Methods {
    /// Control method backing `commands()` (e.g. `"engine.commands"`, `"command.list"`, `"ui.commands"`).
    pub commands: Option<&'static str>,
    /// Control method backing `execute(command, params)` (e.g. `"engine.execute"`, `"command.run"`, `"ui.command"`).
    pub execute: Option<&'static str>,
    /// True when the app's `execute` control method expects `{"id": command, "params": params}`
    /// instead of `{"command": command, "params": params}` (`pdfcraft`, `texturecraft`, `materialcraft`).
    pub execute_uses_id: bool,
    /// Control method backing `docInspect(params)` (`"document.inspect"`, `"session.inspect"`, `"ui.state"`, `"ui.inspect"`).
    pub doc_inspect: Option<&'static str>,
    /// Control method backing `uiInspect(params)` (`"ui.inspect"`).
    pub ui_inspect: Option<&'static str>,
    /// Control method backing `screenshot(params)` (`"ui.screenshot"`).
    pub screenshot: Option<&'static str>,
    /// Control method backing `pointer(events)` (`"ui.pointer"`, `"ui.input"`, `"ui.click"`).
    pub pointer: Option<&'static str>,
}

impl Methods {
    /// Empty method table: only raw `request(method, params)` is exposed.
    pub const NONE: Self = Self {
        commands: None,
        execute: None,
        execute_uses_id: false,
        doc_inspect: None,
        ui_inspect: None,
        screenshot: None,
        pointer: None,
    };

    /// Standard desktop control-server method table shared by most Craft apps.
    pub const STANDARD: Self = Self {
        commands: Some("engine.commands"),
        execute: Some("engine.execute"),
        execute_uses_id: false,
        doc_inspect: Some("document.inspect"),
        ui_inspect: Some("ui.inspect"),
        screenshot: Some("ui.screenshot"),
        pointer: Some("ui.pointer"),
    };

    /// Normalize `execute(command, params)` into the payload shape expected by `self.execute`.
    pub fn format_execute_params(&self, command: &str, params: Value) -> (String, Value) {
        let method = self.execute.unwrap_or("engine.execute").to_string();
        let clean_params = if params.is_null() { json!({}) } else { params };
        let payload = if self.execute_uses_id {
            json!({ "id": command, "params": clean_params })
        } else {
            json!({ "command": command, "params": clean_params })
        };
        (method, payload)
    }
}

impl Default for Methods {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// Static profile for one of the 16 ArtCraft web applications.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct AppProfile {
    /// Canonical app slug (`"photocraft"`, `"vectorcraft"`, ..., `"craft-anything"`).
    pub id: &'static str,
    /// JavaScript global property name on `window` (`"photocraft"`, ..., `"craftAnything"` + alias `"craft-anything"`).
    pub js_global: &'static str,
    /// Display title.
    pub title: &'static str,
    /// Upstream GitHub repository under `storytold/`.
    pub repo: &'static str,
    /// Primary DOM `<canvas>` element ID in `index.html`.
    pub canvas_id: &'static str,
    /// Loading indicator DOM element ID in `index.html`.
    pub loading_id: &'static str,
    /// Default filename when `open` omits a name.
    pub default_file_name: &'static str,
    /// Primary native document extension.
    pub native_ext: &'static str,
    /// Control-channel method mapping for this app.
    pub methods: Methods,
}

/// Canonical table of all 16 ArtCraft applications.
pub const ALL_APPS: [AppProfile; 16] = [
    AppProfile {
        id: "photocraft",
        js_global: "photocraft",
        title: "PhotoCraft",
        repo: "photocraft",
        canvas_id: "photocraft_canvas",
        loading_id: "photocraft_loading",
        default_file_name: "Document.pcraft",
        native_ext: "pcraft",
        methods: Methods::STANDARD,
    },
    AppProfile {
        id: "vectorcraft",
        js_global: "vectorcraft",
        title: "VectorCraft",
        repo: "vectorcraft",
        canvas_id: "vectorcraft_canvas",
        loading_id: "vectorcraft_loading",
        default_file_name: "Drawing.vcraft",
        native_ext: "vcraft",
        methods: Methods::STANDARD,
    },
    AppProfile {
        id: "lightcraft",
        js_global: "lightcraft",
        title: "LightCraft",
        repo: "lightcraft",
        canvas_id: "lightcraft_canvas",
        loading_id: "lightcraft_loading",
        default_file_name: "photo.jpg",
        native_ext: "lcraft",
        methods: Methods {
            doc_inspect: Some("ui.inspect"),
            ..Methods::STANDARD
        },
    },
    AppProfile {
        id: "pdfcraft",
        js_global: "pdfcraft",
        title: "PdfCraft",
        repo: "pdfcraft",
        canvas_id: "pdfcraft",
        loading_id: "pdfcraft_loading",
        default_file_name: "document.pdf",
        native_ext: "pdf",
        methods: Methods {
            commands: Some("ui.commands"),
            execute: Some("ui.command"),
            execute_uses_id: true,
            doc_inspect: Some("ui.state"),
            ui_inspect: Some("ui.inspect"),
            screenshot: Some("ui.screenshot"),
            pointer: Some("ui.click"),
        },
    },
    AppProfile {
        id: "effectcraft",
        js_global: "effectcraft",
        title: "EffectCraft",
        repo: "effectcraft",
        canvas_id: "effectcraft_canvas",
        loading_id: "loading",
        default_file_name: "Project.ecproj",
        native_ext: "ecproj",
        methods: Methods {
            doc_inspect: Some("ui.inspect"),
            pointer: Some("ui.click"),
            ..Methods::STANDARD
        },
    },
    AppProfile {
        id: "designcraft",
        js_global: "designcraft",
        title: "DesignCraft",
        repo: "designcraft",
        canvas_id: "designcraft_canvas",
        loading_id: "designcraft_loading",
        default_file_name: "Document.designcraft",
        native_ext: "designcraft",
        methods: Methods::STANDARD,
    },
    AppProfile {
        id: "figcraft",
        js_global: "figcraft",
        title: "FigCraft",
        repo: "figcraft",
        canvas_id: "figcraft_canvas",
        loading_id: "figcraft_loading",
        default_file_name: "Board.figcraft",
        native_ext: "figcraft",
        methods: Methods::STANDARD,
    },
    AppProfile {
        id: "filmcraft",
        js_global: "filmcraft",
        title: "FilmCraft",
        repo: "filmcraft",
        canvas_id: "filmcraft_canvas",
        loading_id: "loading",
        default_file_name: "Project.fcproj",
        native_ext: "fcproj",
        methods: Methods {
            doc_inspect: Some("ui.inspect"),
            ..Methods::STANDARD
        },
    },
    AppProfile {
        id: "texturecraft",
        js_global: "texturecraft",
        title: "TextureCraft",
        repo: "texturecraft",
        canvas_id: "texturecraft_canvas",
        loading_id: "texturecraft_loading",
        default_file_name: "Project.tcraft",
        native_ext: "tcraft",
        methods: Methods {
            commands: Some("command.list"),
            execute: Some("command.run"),
            execute_uses_id: true,
            doc_inspect: Some("session.inspect"),
            ui_inspect: Some("ui.inspect"),
            screenshot: Some("ui.screenshot"),
            pointer: Some("ui.pointer"),
        },
    },
    AppProfile {
        id: "materialcraft",
        js_global: "materialcraft",
        title: "MaterialCraft",
        repo: "materialcraft",
        canvas_id: "materialcraft_canvas",
        loading_id: "materialcraft_loading",
        default_file_name: "Package.sbs",
        native_ext: "sbs",
        methods: Methods {
            commands: Some("command.list"),
            execute: Some("command.run"),
            execute_uses_id: true,
            doc_inspect: Some("session.inspect"),
            ui_inspect: Some("ui.inspect"),
            screenshot: Some("ui.screenshot"),
            pointer: Some("ui.input"),
        },
    },
    AppProfile {
        id: "wordcraft",
        js_global: "wordcraft",
        title: "WordCraft",
        repo: "wordcraft",
        canvas_id: "wordcraft_canvas",
        loading_id: "wordcraft_loading",
        default_file_name: "Document.docx",
        native_ext: "docx",
        methods: Methods::STANDARD,
    },
    AppProfile {
        id: "soundcraft",
        js_global: "soundcraft",
        title: "SoundCraft",
        repo: "soundcraft",
        canvas_id: "soundcraft_canvas",
        loading_id: "soundcraft_loading",
        default_file_name: "Session.scraft",
        native_ext: "scraft",
        methods: Methods {
            doc_inspect: Some("session.inspect"),
            pointer: Some("ui.click"),
            ..Methods::STANDARD
        },
    },
    AppProfile {
        id: "deckcraft",
        js_global: "deckcraft",
        title: "DeckCraft",
        repo: "deckcraft",
        canvas_id: "deckcraft_canvas",
        loading_id: "deckcraft_loading",
        default_file_name: "Presentation.deckcraft",
        native_ext: "deckcraft",
        methods: Methods::STANDARD,
    },
    AppProfile {
        id: "cadcraft",
        js_global: "cadcraft",
        title: "CADCraft",
        repo: "cadcraft",
        canvas_id: "cadcraft_canvas",
        loading_id: "cadcraft_loading",
        default_file_name: "Drawing.dxf",
        native_ext: "dxf",
        methods: Methods::STANDARD,
    },
    AppProfile {
        id: "gridcraft",
        js_global: "gridcraft",
        title: "GridCraft",
        repo: "gridcraft",
        canvas_id: "gridcraft_canvas",
        loading_id: "gridcraft_loading",
        default_file_name: "Workbook.xlsx",
        native_ext: "xlsx",
        methods: Methods::STANDARD,
    },
    AppProfile {
        id: "craft-anything",
        js_global: "craftAnything",
        title: "Craft Anything",
        repo: "craft-anything",
        canvas_id: "craft_anything_canvas",
        loading_id: "craft_anything_loading",
        default_file_name: "sample.bin",
        native_ext: "bin",
        methods: Methods::STANDARD,
    },
];

impl AppProfile {
    /// Look up one of the 16 ArtCraft apps by slug or JS global name.
    pub fn for_app(app: &str) -> Option<&'static Self> {
        let trimmed = app.trim();
        ALL_APPS
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(trimmed) || p.js_global.eq_ignore_ascii_case(trimmed))
    }

    /// Look up by slug or return a fallback profile using standard desktop control methods.
    pub fn resolve(app: &'static str) -> Self {
        Self::for_app(app).copied().unwrap_or(AppProfile {
            id: app,
            js_global: app,
            title: app,
            repo: app,
            canvas_id: "canvas",
            loading_id: "loading",
            default_file_name: "document",
            native_ext: "bin",
            methods: Methods::STANDARD,
        })
    }
}

/// How the host page embeds and communicates with the web app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostMode {
    /// `?host=parent`: same-origin parent `<iframe>` over `window.parent.postMessage`.
    Parent,
    /// `?embed` or `?embed=1`: same-origin parent `<iframe>` over `window.parent.postMessage`
    /// (also includes `<app>Embed: 1` and `photocraftEmbed: 1` fields on outgoing messages).
    Embed,
    /// `?host=https://host.example[:port]` or `?embed=https://host.example[:port]`:
    /// explicit allowed parent origin over `window.parent.postMessage`.
    ExplicitOrigin(String),
    /// `data-host="page"` on the app's `<canvas>`: same-document DOM `CustomEvent`s on the canvas.
    Page,
}

impl HostMode {
    /// Detect the active host mode from the URL query string and the canvas's `data-host` attribute.
    ///
    /// Priority:
    /// 1. `?host=parent` → `HostMode::Parent`
    /// 2. `?host=https://...` → `HostMode::ExplicitOrigin`
    /// 3. `?embed=https://...` → `HostMode::ExplicitOrigin`
    /// 4. `?embed` / `?embed=1` / `?embed=parent` → `HostMode::Embed`
    /// 5. `data-host="page"` → `HostMode::Page`
    pub fn detect(query: &str, data_host: Option<&str>) -> Option<Self> {
        for (k, v) in query_pairs(query) {
            if k == "host" {
                if v == "parent" {
                    return Some(Self::Parent);
                }
                if let Some(origin) = validate_host_origin(Some(v)) {
                    return Some(Self::ExplicitOrigin(origin));
                }
            } else if k == "embed" {
                if v.is_empty() || v == "1" || v == "true" || v == "parent" {
                    return Some(Self::Embed);
                }
                if let Some(origin) = validate_host_origin(Some(v)) {
                    return Some(Self::ExplicitOrigin(origin));
                }
            }
        }
        if data_host == Some("page") {
            Some(Self::Page)
        } else {
            None
        }
    }

    /// Whether this mode uses `window.parent.postMessage` (as opposed to canvas `CustomEvent`s).
    pub fn uses_post_message(&self) -> bool {
        !matches!(self, Self::Page)
    }

    /// Resolve the expected `postMessage` origin given the frame's own `window.location.origin`.
    /// Returns `None` if the origin is opaque (`"null"`) and no explicit origin was configured.
    pub fn target_origin(&self, self_origin: &str) -> Option<String> {
        match self {
            Self::Parent | Self::Embed | Self::Page => {
                let clean = self_origin.trim();
                if clean.is_empty() || clean == "null" {
                    None
                } else {
                    Some(clean.to_string())
                }
            }
            Self::ExplicitOrigin(origin) => Some(origin.clone()),
        }
    }
}

/// Parse URL query pairs without requiring `web_sys::UrlSearchParams`.
pub fn query_pairs(search: &str) -> impl Iterator<Item = (&str, &str)> {
    search
        .trim_start_matches('?')
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| p.split_once('=').unwrap_or((p, "")))
}

/// Check whether a URL query string contains a key.
pub fn query_has(search: &str, key: &str) -> bool {
    query_pairs(search).any(|(k, _)| k == key)
}

/// Get the first value of `key` in a URL query string.
pub fn query_value<'a>(search: &'a str, key: &str) -> Option<&'a str> {
    query_pairs(search).find(|(k, _)| *k == key).map(|(_, v)| v)
}

/// Validate a BCP 47 language tag from `?lang=<tag>` (`de`, `pt-BR`, `zh_TW`).
pub fn validate_lang_tag(value: &str) -> Option<&str> {
    let ok = !value.is_empty()
        && value.len() <= MAX_LANG_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ok.then_some(value)
}

/// Validate an explicit `http(s)://host[:port]` origin from `?host=` or `?embed=`.
/// Rejects paths, queries, credentials, wildcards (`*`), and `null`.
pub fn validate_host_origin(param: Option<&str>) -> Option<String> {
    let raw = param?.trim();
    let raw = raw.strip_suffix('/').unwrap_or(raw);
    let (scheme, rest) = raw.split_once("://")?;
    if !matches!(scheme, "http" | "https") || rest.is_empty() {
        return None;
    }
    let ok = rest
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'));
    if !ok {
        return None;
    }
    if let Some((host, port)) = rest.rsplit_once(':')
        && !host.ends_with(']')
        && !host.contains(':')
        && (host.is_empty() || port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    Some(format!("{scheme}://{rest}"))
}

/// Verify whether an incoming `MessageEvent` from `event_origin` is authorized.
pub fn accepts_origin(expected_origin: &str, event_origin: &str, from_parent: bool) -> bool {
    from_parent
        && !expected_origin.is_empty()
        && expected_origin != "null"
        && expected_origin != "*"
        && event_origin == expected_origin
}

/// Sanitize a file name from the host: strip directory traversal, control chars, and clamp to 255 chars.
pub fn sanitize_file_name(raw: Option<&str>, fallback: &str) -> String {
    let base = raw
        .unwrap_or_default()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    let cleaned: String = base.chars().filter(|c| !c.is_control()).collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return fallback.to_string();
    }
    trimmed.chars().take(MAX_NAME_CHARS).collect()
}

/// Revision-gated save tracker (`hlvs-apps` #9593 + pasted plan §1):
/// A document is NOT marked clean merely because bytes were sent to the host;
/// it becomes clean only when the host confirms `saved { revision }` for the current revision,
/// or immediately if `require_ack` is `false`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionGate {
    /// Current document revision counter (increments on every edit/command).
    pub current_revision: u64,
    /// Last revision confirmed persisted by the host (`saved` / `saveAck`).
    pub saved_revision: u64,
    /// Revision of the most recent in-flight save sent to the host.
    pub in_flight_save_revision: Option<u64>,
    /// Last save request ID sent to the host (for `saved { id }` correlation).
    pub last_save_id: Option<String>,
    /// Whether the host requires explicit `saved` / `saveAck` before clearing dirty state.
    pub require_ack: bool,
}

impl RevisionGate {
    pub fn new(require_ack: bool) -> Self {
        Self {
            current_revision: 0,
            saved_revision: 0,
            in_flight_save_revision: None,
            last_save_id: None,
            require_ack,
        }
    }

    /// Record a document mutation and return the new revision.
    pub fn record_edit(&mut self) -> u64 {
        self.current_revision = self.current_revision.wrapping_add(1);
        self.current_revision
    }

    /// Synchronize with an external engine dirty flag when the engine tracks its own state.
    pub fn sync_engine_dirty(&mut self, engine_dirty: bool) {
        if engine_dirty && self.current_revision == self.saved_revision {
            self.current_revision = self.current_revision.wrapping_add(1);
        } else if !engine_dirty && !self.require_ack {
            self.saved_revision = self.current_revision;
        }
    }

    /// Record that a save of the current revision was sent to the host.
    /// Returns the revision attached to the outgoing save payload.
    pub fn begin_save(&mut self, save_id: Option<String>) -> u64 {
        let rev = self.current_revision;
        self.in_flight_save_revision = Some(rev);
        self.last_save_id = save_id;
        if !self.require_ack {
            self.saved_revision = rev;
        }
        rev
    }

    /// Confirm that the host persisted `revision` (or `id`).
    /// Returns `true` if the document is now clean (`saved_revision == current_revision`).
    pub fn confirm_saved(&mut self, revision: Option<u64>, id: Option<&str>) -> bool {
        if let Some(rev) = revision {
            if rev <= self.current_revision && rev >= self.saved_revision {
                self.saved_revision = rev;
            }
        } else if let Some(ack_id) = id {
            if self.last_save_id.as_deref() == Some(ack_id)
                && let Some(in_flight) = self.in_flight_save_revision
            {
                self.saved_revision = in_flight;
            }
        } else if let Some(in_flight) = self.in_flight_save_revision {
            self.saved_revision = in_flight;
        }
        !self.is_dirty()
    }

    /// Whether the document has unsaved edits relative to the last host-confirmed revision.
    pub fn is_dirty(&self) -> bool {
        self.current_revision != self.saved_revision
    }
}

/// Shared agent cursor state for visual feedback and frame-capture compositing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CursorState {
    /// X coordinate in CSS pixels (or normalized `[0.0, 1.0]` when `normalized` is true).
    pub x: f64,
    /// Y coordinate in CSS pixels (or normalized `[0.0, 1.0]` when `normalized` is true).
    pub y: f64,
    /// Whether the shared cursor overlay is visible.
    pub visible: bool,
    /// Whether `(x, y)` are normalized `[0.0, 1.0]` coordinates relative to the canvas.
    pub normalized: bool,
    /// Optional agent label badge shown next to the cursor (e.g. `"Agent"`).
    pub label: String,
    /// CSS hex color for the cursor reticle and badge (default `"#38bdf8"`).
    pub color: String,
    /// Whether the primary button is currently pressed.
    pub pressed: bool,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            visible: false,
            normalized: false,
            label: "Agent".to_string(),
            color: "#38bdf8".to_string(),
            pressed: false,
        }
    }
}

impl CursorState {
    /// Update cursor fields from a JSON object (`{x, y, visible?, normalized?, label?, color?, pressed?}`).
    pub fn update_from_json(&mut self, v: &Value) {
        if let Some(x) = v.get("x").and_then(Value::as_f64).filter(|n| n.is_finite()) {
            self.x = x;
            if v.get("visible").is_none() {
                self.visible = true;
            }
        }
        if let Some(y) = v.get("y").and_then(Value::as_f64).filter(|n| n.is_finite()) {
            self.y = y;
        }
        if let Some(vis) = v.get("visible").and_then(Value::as_bool) {
            self.visible = vis;
        }
        if let Some(norm) = v.get("normalized").and_then(Value::as_bool) {
            self.normalized = norm;
        } else if v.get("space").and_then(Value::as_str) == Some("normalized") {
            self.normalized = true;
        }
        if let Some(label) = v.get("label").and_then(Value::as_str) {
            self.label = label.chars().take(64).collect();
        }
        if let Some(color) = v.get("color").and_then(Value::as_str) {
            let safe: String = color
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '#' | '(' | ')' | ',' | '.' | ' ' | '%'))
                .take(32)
                .collect();
            if !safe.is_empty() {
                self.color = safe;
            }
        }
        if let Some(pressed) = v.get("pressed").and_then(Value::as_bool) {
            self.pressed = pressed;
        }
    }

    /// Resolve pixel coordinates `(px, py)` inside a `(width, height)` canvas.
    pub fn pixel_coords(&self, width: u32, height: u32) -> (i32, i32) {
        let w = width.max(1) as f64;
        let h = height.max(1) as f64;
        let (rx, ry) = if self.normalized {
            (self.x.clamp(0.0, 1.0) * w, self.y.clamp(0.0, 1.0) * h)
        } else {
            (self.x.clamp(0.0, w), self.y.clamp(0.0, h))
        };
        (rx.round() as i32, ry.round() as i32)
    }

    /// Composite a visible crosshair/reticle marker onto an RGBA8 framebuffer in place
    /// so headlessly captured frames include the shared agent cursor.
    pub fn composite_rgba(&self, rgba: &mut [u8], width: u32, height: u32) {
        if !self.visible || width == 0 || height == 0 {
            return;
        }
        let expected = (width as usize).saturating_mul(height as usize).saturating_mul(4);
        if rgba.len() < expected {
            return;
        }
        let (cx, cy) = self.pixel_coords(width, height);
        let (r, g, b) = parse_hex_rgb(&self.color).unwrap_or((0x38, 0xbd, 0xf8));
        let radius = if self.pressed { 8i32 } else { 6i32 };
        for dy in -12i32..=12i32 {
            for dx in -12i32..=12i32 {
                let px = cx + dx;
                let py = cy + dy;
                if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                    continue;
                }
                let dist_sq = dx * dx + dy * dy;
                let on_ring = (dist_sq >= (radius - 1) * (radius - 1)) && (dist_sq <= (radius + 1) * (radius + 1));
                let on_cross = (dx == 0 && dy.abs() <= 12 && dy.abs() >= 3) || (dy == 0 && dx.abs() <= 12 && dx.abs() >= 3);
                let on_center = dist_sq <= 2;
                if on_ring || on_cross || on_center {
                    let idx = ((py as usize) * (width as usize) + (px as usize)) * 4;
                    if let Some(pixel) = rgba.get_mut(idx..idx + 4) {
                        pixel[0] = r;
                        pixel[1] = g;
                        pixel[2] = b;
                        pixel[3] = 0xff;
                    }
                }
            }
        }
    }
}

fn parse_hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let s = hex.trim().strip_prefix('#')?;
    if s.len() == 6 {
        let r = u8::from_str_radix(s.get(0..2)?, 16).ok()?;
        let g = u8::from_str_radix(s.get(2..4)?, 16).ok()?;
        let b = u8::from_str_radix(s.get(4..6)?, 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

/// Incoming message kind from the host frame (unified across all 4 protocol dialects).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncomingKind {
    /// `<app>:host`, `hello`, or `ping` → answered with `<app>:ready`.
    Hello,
    /// `<app>:open` or `open` → open a document (and optional font files).
    Open,
    /// `<app>:import`, `import`, `importFile`, `insert`, or `place` → redirect file/asset import into the app.
    Import,
    /// `<app>:save` or `save` → trigger Save / Export and return bytes to the host.
    Save,
    /// `<app>:saved`, `saved`, or `saveAck` → confirm persistence of `revision` / `id`.
    Saved,
    /// `<app>:picked` → answer a `<app>:pick-save` dialog request (#3101).
    Picked,
    /// `<app>:export` or `export` → export in a requested format (`png`, `psd`, `pdf`, `svg`, etc.).
    Export,
    /// `<app>:request` or `request` → execute ANY JSON control-channel method (`method`, `params`).
    Request,
    /// `<app>:execute`, `<app>:cmd`, or `execute` → execute an engine/UI command (`command`/`id`, `params`).
    Execute,
    /// `<app>:commands` or `commands` → list all registered commands (with WebMCP `inputSchema`).
    Commands,
    /// `<app>:inspect`, `docInspect`, or `uiInspect` → inspect document/session/UI state.
    Inspect,
    /// `<app>:pointer` or `pointer` → dispatch synthetic pointer/mouse/keyboard events + move shared cursor.
    Pointer,
    /// `<app>:cursor` or `cursor` → update shared visual cursor overlay without injecting clicks.
    Cursor,
    /// `<app>:snapshot-request`, `<app>:screenshot`, `snapshotRequest`, or `screenshot` → capture frame.
    SnapshotRequest,
    /// `<app>:handoff-done` → confirm cross-app file handoff.
    HandoffDone,
    /// `exam` or `<app>:exam` → toggle exam/locked-command mode (#1614).
    Exam,
    /// `autosave` or `<app>:autosave` → configure periodic snapshot interval (#1614).
    Autosave,
}

impl IncomingKind {
    /// Parse an incoming message kind for `app` from `type` and optional envelope markers
    /// (`bridge`, `craftBridge`, `photocraftEmbed`, `<app>Embed`).
    pub fn parse(app: &str, message_type: &str, has_envelope_marker: bool) -> Option<Self> {
        let raw_kind = if let Some(stripped) = message_type.strip_prefix(app).and_then(|s| s.strip_prefix(':')) {
            stripped
        } else if let Some(stripped) = message_type.strip_prefix("craft:") {
            stripped
        } else if has_envelope_marker {
            message_type
        } else {
            return None;
        };

        match raw_kind {
            "host" | "hello" | "ping" | "ready-request" | "readyRequest" | "status" => Some(Self::Hello),
            "open" => Some(Self::Open),
            "import" | "importFile" | "import-file" | "insert" | "place" | "load" | "loadFile" | "load-file" | "openFile" | "open-file" => Some(Self::Import),
            "save" => Some(Self::Save),
            "saved" | "saveAck" | "save-ack" => Some(Self::Saved),
            "picked" => Some(Self::Picked),
            "export" => Some(Self::Export),
            "request" | "control" => Some(Self::Request),
            "execute" | "cmd" | "command" => Some(Self::Execute),
            "commands" => Some(Self::Commands),
            "inspect" | "docInspect" | "doc-inspect" | "uiInspect" | "ui-inspect" | "state" => Some(Self::Inspect),
            "pointer" | "input" => Some(Self::Pointer),
            "cursor" => Some(Self::Cursor),
            "snapshotRequest" | "snapshot-request" | "screenshot" | "capture" => Some(Self::SnapshotRequest),
            "handoff-done" | "handoffDone" => Some(Self::HandoffDone),
            "exam" | "lock" => Some(Self::Exam),
            "autosave" => Some(Self::Autosave),
            _ => None,
        }
    }
}

/// Outgoing message kind from the app to the host frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutgoingKind {
    Ready,
    Loading,
    Opened,
    Imported,
    OpenFailed,
    Save,
    SaveResult,
    SaveFailed,
    PickSave,
    Dirty,
    State,
    Response,
    Snapshot,
    Commands,
    Failed,
    Handoff,
}

impl OutgoingKind {
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Loading => "loading",
            Self::Opened => "opened",
            Self::Imported => "imported",
            Self::OpenFailed => "open-failed",
            Self::Save => "save",
            Self::SaveResult => "saveResult",
            Self::SaveFailed => "save-failed",
            Self::PickSave => "pick-save",
            Self::Dirty => "dirty",
            Self::State => "state",
            Self::Response => "response",
            Self::Snapshot => "snapshot",
            Self::Commands => "commands",
            Self::Failed => "failed",
            Self::Handoff => "handoff",
        }
    }

    pub fn message_type(self, app: &str) -> String {
        format!("{app}:{}", self.kind())
    }
}

/// Infer a standard MIME type from a file name extension.
pub fn infer_mime_from_name(name: &str) -> &'static str {
    let ext = name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" | "vcraft" => "image/svg+xml",
        "pdf" => "application/pdf",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "json" | "excalidraw" => "application/json",
        "txt" | "md" | "markdown" => "text/plain",
        "html" | "htm" => "text/html",
        "xlsx" | "xls" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "docx" | "doc" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "pptx" | "ppt" | "deckcraft" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "dxf" | "dwg" => "image/vnd.dxf",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// Parse CSV, TSV, or pipe-table text into a 2D array of cell strings for spreadsheet/table import.
pub fn parse_tabular_text_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('|') && line.ends_with('|') {
            let inner = &line[1..line.len().saturating_sub(1)];
            if inner.chars().all(|c| matches!(c, '-' | ':' | '|' | ' ')) {
                continue;
            }
            let cells: Vec<String> = inner.split('|').map(|s| s.trim().to_string()).collect();
            if !cells.is_empty() {
                rows.push(cells);
            }
        } else if line.contains('\t') {
            rows.push(line.split('\t').map(|s| s.trim().to_string()).collect());
        } else if line.contains(',') {
            rows.push(
                line.split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .collect(),
            );
        } else {
            rows.push(vec![line.to_string()]);
        }
    }
    rows
}

/// Resolve a coordinate pair `(x, y)` (either normalized `[0.0..=1.0]` or logical CSS pixels)
/// against a canvas bounding rect `(rect_left, rect_top, rect_w, rect_h)`.
/// Returns `(norm_x, norm_y, css_x, css_y, client_x, client_y)`.
pub fn resolve_canvas_point(
    x: f64,
    y: f64,
    normalized_hint: Option<bool>,
    rect_left: f64,
    rect_top: f64,
    rect_w: f64,
    rect_h: f64,
) -> (f64, f64, f64, f64, f64, f64) {
    let w = if rect_w.is_finite() && rect_w > 1.0 { rect_w } else { 1024.0 };
    let h = if rect_h.is_finite() && rect_h > 1.0 { rect_h } else { 720.0 };
    let left = if rect_left.is_finite() { rect_left } else { 0.0 };
    let top = if rect_top.is_finite() { rect_top } else { 0.0 };
    let is_norm = match normalized_hint {
        Some(b) => b,
        None => (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y) && (x != 0.0 || y != 0.0),
    };
    let (norm_x, norm_y, css_x, css_y) = if is_norm {
        let nx = x.clamp(0.0, 1.0);
        let ny = y.clamp(0.0, 1.0);
        (nx, ny, (nx * w).round(), (ny * h).round())
    } else {
        let cx = x.clamp(0.0, w);
        let cy = y.clamp(0.0, h);
        ((cx / w).clamp(0.0, 1.0), (cy / h).clamp(0.0, 1.0), cx.round(), cy.round())
    };
    (norm_x, norm_y, css_x, css_y, (left + css_x).round(), (top + css_y).round())
}

/// Scale a pointer payload `{ x, y, normalized, space, events }` to canvas logical points
/// and build the ordered list of DOM pointer events `(kind, client_x, client_y, button)` to dispatch.
pub fn scale_pointer_payload_for_canvas(
    payload: &Value,
    rect_left: f64,
    rect_top: f64,
    rect_w: f64,
    rect_h: f64,
) -> (Value, Vec<(String, f64, f64, i16)>) {
    let mut out = payload.clone();
    let norm_hint = payload
        .get("normalized")
        .and_then(Value::as_bool)
        .or_else(|| (payload.get("space").and_then(Value::as_str) == Some("normalized")).then_some(true));
    let top_x = payload.get("x").and_then(Value::as_f64).unwrap_or(0.5);
    let top_y = payload.get("y").and_then(Value::as_f64).unwrap_or(0.5);
    let top_btn = payload.get("button").and_then(Value::as_i64).unwrap_or(0) as i16;
    let (nx, ny, cx, cy, client_x, client_y) =
        resolve_canvas_point(top_x, top_y, norm_hint, rect_left, rect_top, rect_w, rect_h);

    let mut dom_events: Vec<(String, f64, f64, i16)> = Vec::new();

    if let Some(obj) = out.as_object_mut() {
        obj.insert("x".to_string(), json!(cx));
        obj.insert("y".to_string(), json!(cy));
        obj.insert("normX".to_string(), json!(nx));
        obj.insert("normY".to_string(), json!(ny));
        obj.insert("clientX".to_string(), json!(client_x));
        obj.insert("clientY".to_string(), json!(client_y));
        obj.insert("normalized".to_string(), json!(false));

        if let Some(ev_arr) = obj.get_mut("events").and_then(Value::as_array_mut) {
            for ev in ev_arr.iter_mut() {
                if let Some(ev_obj) = ev.as_object_mut() {
                    let ex = ev_obj.get("x").and_then(Value::as_f64).unwrap_or(top_x);
                    let ey = ev_obj.get("y").and_then(Value::as_f64).unwrap_or(top_y);
                    let e_norm = ev_obj.get("normalized").and_then(Value::as_bool).or(norm_hint);
                    let btn = ev_obj.get("button").and_then(Value::as_i64).unwrap_or(i64::from(top_btn)) as i16;
                    let (enx, eny, ecx, ecy, eclx, ecly) =
                        resolve_canvas_point(ex, ey, e_norm, rect_left, rect_top, rect_w, rect_h);
                    ev_obj.insert("x".to_string(), json!(ecx));
                    ev_obj.insert("y".to_string(), json!(ecy));
                    ev_obj.insert("normX".to_string(), json!(enx));
                    ev_obj.insert("normY".to_string(), json!(eny));
                    ev_obj.insert("clientX".to_string(), json!(eclx));
                    ev_obj.insert("clientY".to_string(), json!(ecly));
                    let kind = ev_obj
                        .get("type")
                        .or_else(|| ev_obj.get("kind"))
                        .and_then(Value::as_str)
                        .unwrap_or("move")
                        .to_ascii_lowercase();
                    dom_events.push((kind, eclx, ecly, btn));
                }
            }
        }
    }

    if dom_events.is_empty() {
        dom_events.push(("move".to_string(), client_x, client_y, top_btn));
        if payload.get("click").and_then(Value::as_bool) != Some(false) {
            dom_events.push(("down".to_string(), client_x, client_y, top_btn));
            dom_events.push(("up".to_string(), client_x, client_y, top_btn));
            dom_events.push(("click".to_string(), client_x, client_y, top_btn));
        }
    } else if dom_events.iter().any(|(k, _, _, _)| k == "up")
        && !dom_events.iter().any(|(k, _, _, _)| k == "click")
    {
        if let Some((_, last_x, last_y, last_btn)) = dom_events.last().cloned() {
            dom_events.push(("click".to_string(), last_x, last_y, last_btn));
        }
    }

    (out, dom_events)
}

/// Clamp an autosave interval in milliseconds to `[MIN_AUTOSAVE_MS, MAX_AUTOSAVE_MS]`;
/// `<= 0` or non-finite disables autosave (`None`).
pub fn clamp_autosave_interval(ms: Option<f64>) -> Option<u32> {
    let val = ms.filter(|n| n.is_finite() && *n > 0.0)?;
    Some((val.round() as u64).clamp(u64::from(MIN_AUTOSAVE_MS), u64::from(MAX_AUTOSAVE_MS)) as u32)
}

/// Commands blocked when exam / lock mode (#1614) is active.
pub fn exam_blocks_command(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    lower.starts_with("file.open")
        || lower.starts_with("file.save")
        || lower.starts_with("file.export")
        || lower.starts_with("file.place")
        || lower.starts_with("file.import")
        || lower.starts_with("app.open")
        || lower.starts_with("app.save")
        || lower.starts_with("app.export")
}

/// Extract the inline base64 PNG from any app's `screenshot` control reply and ensure
/// it is also present under `result.data` (`craft-web-api` contract + `pngBase64`/`png_base64`/`png`/`base64`).
pub const PNG_FIELDS: [&str; 4] = ["pngBase64", "png_base64", "png", "base64"];

pub fn normalize_screenshot_reply(mut reply: Value) -> Value {
    if reply.get("ok").and_then(Value::as_bool) == Some(false) {
        return reply;
    }
    let Some(result) = reply.get_mut("result").and_then(Value::as_object_mut) else {
        return reply;
    };
    if !result.contains_key("data")
        && let Some(png) = PNG_FIELDS
            .iter()
            .find_map(|field| result.get(*field).filter(|v| v.is_string()).cloned())
    {
        result.insert("data".to_string(), png);
    }
    reply
}

/// Extract the base64 PNG string from a normalized screenshot reply.
pub fn extract_screenshot_base64(reply: &Value) -> Option<&str> {
    let result = reply.get("result")?;
    if let Some(s) = result.get("data").and_then(Value::as_str) {
        return Some(s);
    }
    PNG_FIELDS
        .iter()
        .find_map(|field| result.get(*field).and_then(Value::as_str))
}

/// Enrich a command list from `engine.commands` / `command.list` / `ui.commands` with
/// a WebMCP-compatible JSON Schema `inputSchema` (#6450) when not already present.
pub fn enrich_commands_with_schema(commands_reply: Value) -> Value {
    let mut out = commands_reply;
    let list_opt = if out.is_array() {
        out.as_array_mut()
    } else if let Some(obj) = out.as_object_mut() {
        if obj.get("result").is_some_and(Value::is_array) {
            obj.get_mut("result").and_then(Value::as_array_mut)
        } else if obj
            .get("result")
            .and_then(Value::as_object)
            .and_then(|r| r.get("commands"))
            .is_some_and(Value::is_array)
        {
            obj.get_mut("result")
                .and_then(Value::as_object_mut)
                .and_then(|r| r.get_mut("commands"))
                .and_then(Value::as_array_mut)
        } else if obj.get("commands").is_some_and(Value::is_array) {
            obj.get_mut("commands").and_then(Value::as_array_mut)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(list) = list_opt {
        for item in list.iter_mut() {
            let Some(obj) = item.as_object_mut() else { continue };
            if obj.contains_key("inputSchema") {
                continue;
            }
            let schema = synthesize_input_schema(obj.get("params"));
            obj.insert("inputSchema".to_string(), schema);
        }
    }
    out
}

fn synthesize_input_schema(params_meta: Option<&Value>) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    if let Some(Value::Array(arr)) = params_meta {
        for p in arr {
            if let Some(name) = p.get("name").and_then(Value::as_str) {
                let ty = match p.get("type").and_then(Value::as_str).unwrap_or("string") {
                    "number" | "f32" | "f64" | "float" => "number",
                    "integer" | "i32" | "i64" | "u32" | "u64" | "usize" => "integer",
                    "boolean" | "bool" => "boolean",
                    "array" => "array",
                    "object" => "object",
                    _ => "string",
                };
                let mut prop = json!({ "type": ty });
                if let Some(desc) = p.get("description").or_else(|| p.get("doc")).and_then(Value::as_str) {
                    prop["description"] = json!(desc);
                }
                properties.insert(name.to_string(), prop);
                if p.get("required").and_then(Value::as_bool) == Some(true) {
                    required.push(json!(name));
                }
            }
        }
    } else if let Some(Value::Object(map)) = params_meta {
        for (name, desc) in map {
            properties.insert(
                name.clone(),
                json!({
                    "type": "string",
                    "description": desc.as_str().unwrap_or("")
                }),
            );
        }
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": true
    })
}

/// Pure-Rust base64 decoder (standard alphabet, padding optional) so the universal insert
/// needs no external `base64` crate dependency when converting PNG base64 into `ArrayBuffer`s.
pub fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut chunks = bytes.chunks_exact(4);
    for c in &mut chunks {
        let a = val(c[0])?;
        let b = val(c[1])?;
        let c2 = val(c[2])?;
        let d = val(c[3])?;
        out.push((a << 2) | (b >> 4));
        out.push((b << 4) | (c2 >> 2));
        out.push((c2 << 6) | d);
    }
    let rem = chunks.remainder();
    match rem.len() {
        0 => {}
        2 => {
            let a = val(rem[0])?;
            let b = val(rem[1])?;
            out.push((a << 2) | (b >> 4));
        }
        3 => {
            let a = val(rem[0])?;
            let b = val(rem[1])?;
            let c2 = val(rem[2])?;
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c2 >> 2));
        }
        _ => return None,
    }
    Some(out)
}

/// Pure-Rust base64 encoder (standard alphabet) so the universal insert needs no external `base64` crate.
pub fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for c in &mut chunks {
        let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(n & 0x3f) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let n = (rem[0] as u32) << 16;
            out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Re-entrant single-threaded delivery queue so synchronous page-mode `CustomEvent` handlers
/// never panic on overlapping `RefCell` borrows.
pub struct DeliveryQueue<M, F> {
    handler: std::cell::RefCell<F>,
    queue: std::cell::RefCell<std::collections::VecDeque<M>>,
}

impl<M, F: FnMut(M)> DeliveryQueue<M, F> {
    pub fn new(handler: F) -> Self {
        Self {
            handler: std::cell::RefCell::new(handler),
            queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
        }
    }

    pub fn deliver(&self, message: M) {
        self.queue.borrow_mut().push_back(message);
        let Ok(mut handler) = self.handler.try_borrow_mut() else {
            return;
        };
        loop {
            let next = self.queue.borrow_mut().pop_front();
            let Some(msg) = next else { break };
            handler(msg);
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::sync::mpsc::{Receiver, TryRecvError};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::prelude::JsValue;
    use wasm_bindgen_futures::{JsFuture, future_to_promise, spawn_local};

    /// Async control-request function type: `(method, params) -> Future<{ok, result, error}>`
    pub type AsyncRequestFn = Rc<dyn Fn(String, Value) -> Pin<Box<dyn Future<Output = Value>>>>;

    /// Open-file callback: `(name, bytes, fonts)`
    pub type OpenCallback = Rc<RefCell<Option<Box<dyn FnMut(String, Vec<u8>, Vec<(String, Vec<u8>)>)>>>>;

    /// Trigger-save callback: `(optional_format) -> Result<(String, Vec<u8>), String>`
    pub type SaveTriggerCallback = Rc<RefCell<Option<Box<dyn FnMut(Option<String>) -> Result<(String, Vec<u8>), String>>>>>;

    /// Poll interval for mpsc control replies (~1 frame at 60Hz).
    const POLL_MS: f64 = 16.0;

    /// Default request timeout in milliseconds (30 seconds).
    const DEFAULT_TIMEOUT_MS: f64 = 30_000.0;

    /// Universal WebAssembly Bridge instance for one of the 16 ArtCraft apps.
    #[derive(Clone)]
    pub struct UniversalBridge {
        pub profile: AppProfile,
        pub mode: Option<HostMode>,
        pub host_origin: Option<String>,
        pub request_fn: AsyncRequestFn,
        pub revision: Rc<RefCell<RevisionGate>>,
        pub cursor: Rc<RefCell<CursorState>>,
        pub exam_locked: Rc<Cell<bool>>,
        pub open_cb: OpenCallback,
        pub save_trigger_cb: SaveTriggerCallback,
        pub waker: Rc<RefCell<Option<Box<dyn Fn()>>>>,
        in_flight: Rc<Cell<usize>>,
        last_dirty_reported: Rc<Cell<Option<bool>>>,
        pub app_ready: Rc<Cell<bool>>,
        pub ready_version: Rc<RefCell<String>>,
        pub held_queue: Rc<RefCell<Vec<(IncomingKind, JsValue)>>>,
    }

    impl UniversalBridge {
        /// Create and install the universal bridge for `app_id` by automatically bridging
        /// an existing `window.<app>.request` (or `window.command` in LightCraft) when present,
        /// upgrading `window.<app>` with `captureFrame`, `captureStream`, `setCursor`, `getCursor`,
        /// `pointer`, `saveAck`, and `state`, and exposing all JSON control + file + snapshot
        /// operations to the same-origin host frame (`?host=parent`, `?embed`, `data-host="page"`).
        pub fn install_auto(app_id: &'static str) -> Self {
            let profile = AppProfile::resolve(app_id);
            let g = js_sys::global();
            let raw_req = get_prop(&g.clone().into(), profile.js_global)
                .and_then(|obj| get_prop(&obj, "request"))
                .filter(|v| v.is_function());
            let raw_target = get_prop(&g.clone().into(), profile.js_global).unwrap_or(JsValue::NULL);

            let async_req: AsyncRequestFn = Rc::new(move |method: String, params: Value| {
                let raw_req = raw_req.clone();
                let raw_target = raw_target.clone();
                Box::pin(async move {
                    if let Some(f_val) = raw_req {
                        let f: js_sys::Function = f_val.unchecked_into();
                        let js_params = json_to_js(&params);
                        match f.call2(&raw_target, &JsValue::from_str(&method), &js_params) {
                            Ok(p_val) if p_val.is_instance_of::<js_sys::Promise>() => {
                                let p: js_sys::Promise = p_val.unchecked_into();
                                match JsFuture::from(p).await {
                                    Ok(res) => json!({ "ok": true, "result": js_to_json(&res) }),
                                    Err(err) => {
                                        let msg = get_prop(&err, "message")
                                            .and_then(|v| v.as_string())
                                            .or_else(|| err.as_string())
                                            .unwrap_or_else(|| "request failed".to_string());
                                        json!({ "ok": false, "error": msg })
                                    }
                                }
                            }
                            Ok(res) => json!({ "ok": true, "result": js_to_json(&res) }),
                            Err(err) => {
                                let msg = err.as_string().unwrap_or_else(|| "call failed".to_string());
                                json!({ "ok": false, "error": msg })
                            }
                        }
                    } else {
                        json!({
                            "ok": false,
                            "error": format!("{} control channel is not connected", profile.id)
                        })
                    }
                })
            });
            Self::install(app_id, async_req)
        }

        /// Create and install the universal bridge for `app_id` using a synchronous channel-based
        /// control sender `|method, params| -> Receiver<Value>`.
        pub fn install_with_channel<F>(app_id: &'static str, send_channel: F) -> Self
        where
            F: Fn(String, Value) -> Receiver<Value> + 'static,
        {
            let send_rc = Rc::new(send_channel);
            let async_req: AsyncRequestFn = Rc::new(move |method: String, params: Value| {
                let rx = send_rc(method, params);
                Box::pin(wait_receiver(rx, DEFAULT_TIMEOUT_MS))
            });
            Self::install(app_id, async_req)
        }

        /// Create and install both:
        /// 1. `window.<app>` and `window.craftUniversalBridge` (same-page / same-origin direct JS API), and
        /// 2. The host-frame bridge (`?host=parent`, `?embed`, `?host=<origin>`, or `data-host="page"`)
        ///    routing BOTH file operations (`open`, `save`, `saved`/`saveAck`, `dirty`) AND all JSON
        ///    control-channel operations (`request`, `execute`, `commands`, `inspect`, `pointer`, `cursor`,
        ///    `snapshotRequest`) over the same origin!
        pub fn install(app_id: &'static str, request_fn: AsyncRequestFn) -> Self {
            let profile = AppProfile::resolve(app_id);
            let query = location_search();
            let data_host = canvas_attribute(profile.canvas_id, "data-host");
            let detected_mode = HostMode::detect(&query, data_host.as_deref());
            let self_origin = location_origin();

            let (mode, host_origin) = match detected_mode {
                Some(m) => {
                    if m.uses_post_message() {
                        if !has_distinct_parent() {
                            (None, None)
                        } else if matches!(m, HostMode::Parent | HostMode::Embed)
                            && parent_origin().as_deref() != Some(self_origin.as_str())
                        {
                            (None, None)
                        } else {
                            let target = m.target_origin(&self_origin);
                            (target.as_ref().map(|_| m), target)
                        }
                    } else {
                        (Some(m), Some(self_origin))
                    }
                }
                None => (None, None),
            };

            let require_ack = matches!(mode, Some(HostMode::Embed)) || query_has(&query, "saveAck");
            install_dom_hooks(profile.id, profile.canvas_id);
            let bridge = Self {
                profile,
                mode,
                host_origin,
                request_fn,
                revision: Rc::new(RefCell::new(RevisionGate::new(require_ack))),
                cursor: Rc::new(RefCell::new(CursorState::default())),
                exam_locked: Rc::new(Cell::new(query_value(&query, "exam") == Some("1"))),
                open_cb: Rc::default(),
                save_trigger_cb: Rc::default(),
                waker: Rc::default(),
                in_flight: Rc::new(Cell::new(0)),
                last_dirty_reported: Rc::new(Cell::new(None)),
                app_ready: Rc::new(Cell::new(false)),
                ready_version: Rc::new(RefCell::new(env!("CARGO_PKG_VERSION").to_string())),
                held_queue: Rc::new(RefCell::new(Vec::new())),
            };

            let _ = bridge.install_window_api();
            bridge.install_host_listener();
            bridge.start_readiness_gate();
            bridge
        }

        /// Register an explicit UI repaint waker (`|| ctx.request_repaint()`) invoked whenever
        /// a control request is waiting for the `eframe` loop to run a pass.
        pub fn set_waker(&self, f: impl Fn() + 'static) {
            *self.waker.borrow_mut() = Some(Box::new(f));
        }

        /// Register how incoming `open { name, bytes, fonts }` messages hand files to the app.
        pub fn on_open(&self, f: impl FnMut(String, Vec<u8>, Vec<(String, Vec<u8>)>) + 'static) {
            *self.open_cb.borrow_mut() = Some(Box::new(f));
        }

        /// Register a callback when the host sends `save` / `export` directly.
        pub fn on_save_trigger(&self, f: impl FnMut(Option<String>) -> Result<(String, Vec<u8>), String> + 'static) {
            *self.save_trigger_cb.borrow_mut() = Some(Box::new(f));
        }

        /// Notify the host frame that the app is ready and listening (or store version until canvas readiness completes).
        pub fn post_ready(&self, app_version: &str, build_sha: Option<&str>) -> bool {
            *self.ready_version.borrow_mut() = app_version.to_string();
            if self.app_ready.get() || is_canvas_and_dom_ready(self.profile.id, self.profile.canvas_id) {
                self.app_ready.set(true);
                set_window_bridge_ready_flag(true);
                let ok = self.emit_ready_now(app_version, build_sha);
                self.flush_held_queue();
                ok
            } else {
                let held_len = self.held_queue.borrow().len() as f64;
                self.post_to_host(
                    OutgoingKind::Loading,
                    &[
                        ("ready", JsValue::from_bool(false)),
                        ("status", JsValue::from_str("loading")),
                        ("heldCount", JsValue::from_f64(held_len)),
                    ],
                    None,
                )
            }
        }

        fn emit_ready_now(&self, app_version: &str, build_sha: Option<&str>) -> bool {
            let caps: js_sys::Array = CAPABILITIES.iter().map(|c| JsValue::from_str(c)).collect();
            let rev_val = build_sha.map_or(JsValue::NULL, JsValue::from_str);
            let (_, _, rect_w, rect_h) = canvas_bounding_rect(self.profile.canvas_id);
            let held_flushed = self.held_queue.borrow().len() as f64;
            self.post_to_host(
                OutgoingKind::Ready,
                &[
                    ("ready", JsValue::from_bool(true)),
                    ("version", PROTOCOL_VERSION.into()),
                    ("bridgeVersion", BRIDGE_ID.into()),
                    ("app", self.profile.id.into()),
                    ("appVersion", app_version.into()),
                    ("formatExtension", self.profile.native_ext.into()),
                    ("rev", rev_val),
                    ("file", false.into()),
                    ("canvasWidth", JsValue::from_f64(rect_w)),
                    ("canvasHeight", JsValue::from_f64(rect_h)),
                    ("flushedCount", JsValue::from_f64(held_flushed)),
                    ("capabilities", caps.into()),
                ],
                None,
            )
        }

        fn start_readiness_gate(&self) {
            let b = self.clone();
            spawn_local(async move {
                let mut elapsed = 0.0;
                while elapsed < 25_000.0 {
                    if is_canvas_and_dom_ready(b.profile.id, b.profile.canvas_id) {
                        break;
                    }
                    if let Some(w) = b.waker.borrow().as_ref() {
                        w();
                    }
                    wake_canvas(b.profile.canvas_id, (elapsed as u32) / 40);
                    sleep_ms(40.0).await;
                    elapsed += 40.0;
                }
                // Give eframe 2 repaint frames after canvas mount so its control loop is actively draining
                for step in 0..2u32 {
                    if let Some(w) = b.waker.borrow().as_ref() {
                        w();
                    }
                    wake_canvas(b.profile.canvas_id, step);
                    sleep_ms(24.0).await;
                }
                if !b.app_ready.get() {
                    b.app_ready.set(true);
                    set_window_bridge_ready_flag(true);
                    let ver = b.ready_version.borrow().clone();
                    let _ = b.emit_ready_now(&ver, None);
                }
                b.flush_held_queue();
            });
        }

        fn flush_held_queue(&self) {
            // Also drain any pre-boot messages captured in window.__craftPreBootQueue before WASM initialized
            let preboot = drain_window_preboot_queue(self.profile.id);
            let mut items: Vec<(IncomingKind, JsValue)> = Vec::new();
            for (k, d) in preboot {
                items.push((k, d));
            }
            {
                let mut q = self.held_queue.borrow_mut();
                items.append(&mut *q);
            }
            if items.is_empty() {
                return;
            }
            let b = self.clone();
            spawn_local(async move {
                for (kind, data) in items {
                    b.dispatch_ready_incoming(kind, data);
                    sleep_ms(20.0).await;
                }
            });
        }

        pub async fn import_or_open_payload(&self, data: &JsValue, is_import: bool) -> Value {
            let raw_name = get_prop(data, "name")
                .or_else(|| get_prop(data, "fileName"))
                .or_else(|| get_prop(data, "filename"))
                .and_then(|v| v.as_string());
            let name = sanitize_file_name(raw_name.as_deref(), self.profile.default_file_name);
            let mime = get_prop(data, "mime")
                .or_else(|| get_prop(data, "mimeType"))
                .or_else(|| get_prop(data, "contentType"))
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| infer_mime_from_name(&name).to_string());
            let Some(bytes) = extract_bytes(data) else {
                return json!({
                    "ok": false,
                    "error": "missing file bytes, base64, dataUrl, or text payload"
                });
            };
            if (bytes.len() as u64) > MAX_OPEN_BYTES {
                return json!({
                    "ok": false,
                    "error": format!("file exceeds {} MiB host open limit", MAX_OPEN_BYTES >> 20)
                });
            }

            // 1. Stage in DOM (<input type="file"> & showOpenFilePicker redirector) AND dispatch DragEvent("drop") onto #<canvas_id>
            stage_and_drop_file_in_dom(self.profile.canvas_id, &name, &mime, &bytes);

            // 2. If app registered an open_cb, invoke it directly
            let fonts = extract_fonts(data);
            let mut handled_by_cb = false;
            if let Some(cb) = self.open_cb.borrow_mut().as_mut() {
                cb(name.clone(), bytes.clone(), fonts);
                handled_by_cb = true;
            }

            if let Some(w) = self.waker.borrow().as_ref() {
                w();
            }
            wake_canvas(self.profile.canvas_id, 1);

            // 3. Also drive app-specific control-channel import/open commands when applicable
            let app_id = self.profile.id;
            if app_id == "gridcraft" {
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    let rows = parse_tabular_text_rows(text);
                    if !rows.is_empty() {
                        let num_rows = rows.len();
                        let num_cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
                        let end_col = ((b'A' + ((num_cols.min(26) - 1) as u8)) as char).to_string();
                        let range_str = format!("A1:{end_col}{num_rows}");
                        let (m, p) = self.profile.methods.format_execute_params(
                            "range.setValues",
                            json!({ "range": range_str, "values": rows }),
                        );
                        let _ = self.call_control(&m, p).await;
                    }
                }
            } else if app_id == "wordcraft" {
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    if !text.trim().is_empty() {
                        let (m, p) = self
                            .profile
                            .methods
                            .format_execute_params("text.insert", json!({ "text": text }));
                        let _ = self.call_control(&m, p).await;
                    }
                }
            } else if matches!(app_id, "designcraft" | "deckcraft") {
                let b64 = base64_encode(&bytes);
                let (m, p) = self
                    .profile
                    .methods
                    .format_execute_params("file.openBytes", json!({ "name": name, "data": b64 }));
                let _ = self.call_control(&m, p).await;
            } else if app_id == "craft-anything" && !handled_by_cb {
                let b64 = base64_encode(&bytes);
                let (m, p) = self
                    .profile
                    .methods
                    .format_execute_params("file.open", json!({ "name": name, "data": b64 }));
                let _ = self.call_control(&m, p).await;
            }

            let rev = self.revision.borrow_mut().record_edit();
            json!({
                "ok": true,
                "result": {
                    "ok": true,
                    "name": name,
                    "mime": mime,
                    "size": bytes.len(),
                    "mode": if is_import { "import" } else { "open" },
                    "revision": rev
                }
            })
        }

        /// Notify the host frame that startup or an operation failed.
        pub fn post_failed(&self, error: &str, id: Option<&str>) -> bool {
            let mut fields = vec![("error", JsValue::from_str(error))];
            if let Some(id) = id {
                fields.push(("id", JsValue::from_str(id)));
            }
            self.post_to_host(OutgoingKind::Failed, &fields, None)
        }

        /// Send saved/exported document bytes to the host frame with zero-copy `ArrayBuffer` transfer
        /// and record the revision on the `RevisionGate`.
        pub fn post_save(&self, name: &str, bytes: &[u8], save_as: bool, request_id: Option<&JsValue>) -> Result<u64, String> {
            let clean_name = sanitize_file_name(Some(name), self.profile.default_file_name);
            let rev = self.revision.borrow_mut().begin_save(Some(clean_name.clone()));
            let buffer = js_sys::Uint8Array::from(bytes).buffer();
            let mut fields = vec![
                ("name", JsValue::from_str(&clean_name)),
                ("saveAs", JsValue::from_bool(save_as)),
                ("revision", JsValue::from_f64(rev as f64)),
                ("bytes", buffer.clone().into()),
                ("document", buffer.clone().into()),
            ];
            if let Some(req_id) = request_id.filter(|v| !v.is_undefined() && !v.is_null()) {
                fields.push(("requestId", req_id.clone()));
            }
            if self.post_to_host(OutgoingKind::Save, &fields, Some(&buffer)) {
                Ok(rev)
            } else {
                Err("failed to post save to host frame".to_string())
            }
        }

        /// Report dirty/revision state changes after each frame.
        pub fn report_dirty(&self, engine_dirty: bool) {
            self.revision.borrow_mut().sync_engine_dirty(engine_dirty);
            let dirty = self.revision.borrow().is_dirty();
            let rev = self.revision.borrow().current_revision;
            if self.last_dirty_reported.get() != Some(dirty) {
                let prev = self.last_dirty_reported.replace(Some(dirty));
                let ok = self.post_to_host(
                    OutgoingKind::Dirty,
                    &[
                        ("dirty", JsValue::from_bool(dirty)),
                        ("revision", JsValue::from_f64(rev as f64)),
                    ],
                    None,
                );
                if !ok && self.last_dirty_reported.get() == Some(dirty) {
                    self.last_dirty_reported.set(prev);
                }
            }
        }

        /// Execute any JSON control request `(method, params)` through the app's control channel,
        /// automatically waking the `eframe` runner across 1-frame and multi-frame (`ui.screenshot`)
        /// operations and normalizing the reply envelope.
        pub async fn call_control(&self, method: &str, params: Value) -> Value {
            if self.exam_locked.get() && method == self.profile.methods.execute.unwrap_or("engine.execute") {
                let cmd = params
                    .get("command")
                    .or_else(|| params.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if exam_blocks_command(cmd) {
                    return json!({
                        "ok": false,
                        "error": format!("command `{cmd}` is locked in exam mode")
                    });
                }
            }

            let prev_flight = self.in_flight.get();
            self.in_flight.set(prev_flight.saturating_add(1));
            if prev_flight == 0 {
                let flight = self.in_flight.clone();
                let waker = self.waker.clone();
                let canvas_id = self.profile.canvas_id;
                spawn_local(async move {
                    let mut step: u32 = 0;
                    while flight.get() > 0 {
                        if let Some(w) = waker.borrow().as_ref() {
                            w();
                        }
                        wake_canvas(canvas_id, step);
                        step = step.wrapping_add(1);
                        sleep_ms(POLL_MS).await;
                    }
                });
            }

            let raw_reply = (self.request_fn)(method.to_string(), params).await;
            self.in_flight.set(self.in_flight.get().saturating_sub(1));

            let reply = normalize_control_reply(raw_reply);
            if method == self.profile.methods.execute.unwrap_or("engine.execute")
                && reply.get("ok").and_then(Value::as_bool) == Some(true)
            {
                self.revision.borrow_mut().record_edit();
            }
            reply
        }

        /// Inspect document state (`doc_inspect`), automatically adapting when `document.inspect`
        /// is an engine command (as in PhotoCraft) or falling back to `ui.inspect` when no document
        /// is open yet.
        pub async fn call_doc_inspect(&self, params: Value) -> Value {
            let doc_method = self
                .profile
                .methods
                .doc_inspect
                .or(self.profile.methods.ui_inspect)
                .unwrap_or("ui.inspect");
            let mut reply = self.call_control(doc_method, params.clone()).await;
            if reply.get("ok").and_then(Value::as_bool) == Some(false) {
                let err_lower = reply
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if err_lower.contains("unknown method") || err_lower.contains("engine command") {
                    let (exec_method, payload) = self.profile.methods.format_execute_params(doc_method, params);
                    reply = self.call_control(&exec_method, payload).await;
                }
                if reply.get("ok").and_then(Value::as_bool) == Some(false)
                    && let Some(ui_method) = self.profile.methods.ui_inspect
                {
                    let ui_reply = self.call_control(ui_method, json!({})).await;
                    if ui_reply.get("ok").and_then(Value::as_bool) == Some(true) {
                        return ui_reply;
                    }
                }
            }
            reply
        }

        /// Capture the current frame (via `ui.screenshot` with fallback to direct `<canvas>` readback),
        /// optionally compositing the shared agent cursor.
        pub async fn capture_frame(&self, mut params: Value) -> Value {
            let include_cursor = params
                .get("includeCursor")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| self.cursor.borrow().visible);
            if let Some(obj) = params.as_object_mut() {
                obj.entry("data").or_insert(json!(true));
            } else {
                params = json!({ "data": true });
            }

            let screenshot_method = self.profile.methods.screenshot.unwrap_or("ui.screenshot");
            let mut reply = normalize_screenshot_reply(self.call_control(screenshot_method, params).await);

            // Fallback: if the control channel did not return inline PNG data, capture directly from the HTMLCanvasElement.
            if extract_screenshot_base64(&reply).is_none()
                && let Some((w, h, b64)) = capture_canvas_png_base64(self.profile.canvas_id, include_cursor, &self.cursor.borrow())
            {
                reply = json!({
                    "ok": true,
                    "result": {
                        "width": w,
                        "height": h,
                        "data": b64,
                        "png": b64,
                        "source": "canvas"
                    }
                });
            }

            if let Some(result) = reply.get_mut("result").and_then(Value::as_object_mut) {
                let cursor_snapshot = serde_json::to_value(&*self.cursor.borrow()).unwrap_or(Value::Null);
                result.insert("cursor".to_string(), cursor_snapshot);
                result.insert("revision".to_string(), json!(self.revision.borrow().current_revision));
            }
            reply
        }

        fn install_window_api(&self) -> Result<(), JsValue> {
            let g = js_sys::global();
            let existing = get_prop(&g, self.profile.js_global);
            // Guard against HTML5 `<canvas id="<app>">` named-window access returning the DOM
            // `HTMLCanvasElement` itself: never mutate a DOM node as the API object (which would
            // shadow `HTMLCanvasElement.prototype.captureStream`).
            let is_dom_node = existing
                .as_ref()
                .and_then(|v| get_prop(v, "nodeType"))
                .is_some_and(|n| !n.is_undefined() && !n.is_null());
            let target: js_sys::Object = match existing {
                Some(v) if v.is_object() && !is_dom_node => v.unchecked_into(),
                Some(v) if v.is_object() => {
                    let fresh = js_sys::Object::new();
                    for key in [
                        "request",
                        "execute",
                        "commands",
                        "inspect",
                        "uiInspect",
                        "docInspect",
                        "screenshot",
                        "state",
                        "command",
                    ] {
                        if let Some(prop) = get_prop(&v, key).filter(|p| p.is_function()) {
                            let _ = js_sys::Reflect::set(&fresh, &JsValue::from_str(key), &prop);
                        }
                    }
                    fresh
                }
                _ => js_sys::Object::new(),
            };

            let set_if_missing = |name: &str, val: JsValue| -> Result<(), JsValue> {
                if !js_sys::Reflect::has(&target, &JsValue::from_str(name)).unwrap_or(false) {
                    let _ = js_sys::Reflect::set(&target, &JsValue::from_str(name), &val)?;
                }
                Ok(())
            };

            let set_always = |name: &str, val: JsValue| -> Result<(), JsValue> {
                let _ = js_sys::Reflect::set(&target, &JsValue::from_str(name), &val)?;
                Ok(())
            };

            // 1. request(method, params)
            {
                let b = self.clone();
                let f = Closure::<dyn FnMut(String, JsValue) -> js_sys::Promise>::new(move |method: String, params: JsValue| {
                    let b = b.clone();
                    let p = js_to_json(&params);
                    future_to_promise(async move { reply_to_promise_outcome(&b.call_control(&method, p).await) })
                });
                set_if_missing("request", f.into_js_value())?;
            }

            // 2. execute(command, params)
            if self.profile.methods.execute.is_some() {
                let b = self.clone();
                let f = Closure::<dyn FnMut(String, JsValue) -> js_sys::Promise>::new(move |command: String, params: JsValue| {
                    let b = b.clone();
                    let (method, payload) = b.profile.methods.format_execute_params(&command, js_to_json(&params));
                    future_to_promise(async move { reply_to_promise_outcome(&b.call_control(&method, payload).await) })
                });
                set_if_missing("execute", f.into_js_value())?;
            }

            // 3. commands() — Always use UniversalBridge's schema-enriched commands()
            if let Some(cmd_method) = self.profile.methods.commands {
                let b = self.clone();
                let f = Closure::<dyn FnMut() -> js_sys::Promise>::new(move || {
                    let b = b.clone();
                    future_to_promise(async move {
                        let reply = enrich_commands_with_schema(b.call_control(cmd_method, json!({})).await);
                        reply_to_promise_outcome(&reply)
                    })
                });
                set_always("commands", f.into_js_value())?;
            }

            // 4. docInspect(params) & uiInspect(params) & inspect(params)
            if self.profile.methods.doc_inspect.is_some() {
                let b = self.clone();
                let f = Closure::<dyn FnMut(JsValue) -> js_sys::Promise>::new(move |params: JsValue| {
                    let b = b.clone();
                    let p = js_to_json(&params);
                    future_to_promise(async move { reply_to_promise_outcome(&b.call_doc_inspect(p).await) })
                });
                set_always("docInspect", f.into_js_value())?;
            }

            if let Some(ui_method) = self.profile.methods.ui_inspect {
                let b = self.clone();
                let f = Closure::<dyn FnMut(JsValue) -> js_sys::Promise>::new(move |params: JsValue| {
                    let b = b.clone();
                    let p = js_to_json(&params);
                    future_to_promise(async move { reply_to_promise_outcome(&b.call_control(ui_method, p).await) })
                });
                set_if_missing("uiInspect", f.into_js_value())?;

                let b2 = self.clone();
                let f2 = Closure::<dyn FnMut(JsValue) -> js_sys::Promise>::new(move |params: JsValue| {
                    let b2 = b2.clone();
                    let p = js_to_json(&params);
                    future_to_promise(async move { reply_to_promise_outcome(&b2.call_control(ui_method, p).await) })
                });
                set_if_missing("inspect", f2.into_js_value())?;
            }

            // 5. screenshot(params) & captureFrame(params) & captureStream(fps)
            {
                let b = self.clone();
                let f = Closure::<dyn FnMut(JsValue) -> js_sys::Promise>::new(move |params: JsValue| {
                    let b = b.clone();
                    let p = js_to_json(&params);
                    future_to_promise(async move { reply_to_promise_outcome(&b.capture_frame(p).await) })
                });
                set_if_missing("screenshot", f.into_js_value())?;

                let b2 = self.clone();
                let f2 = Closure::<dyn FnMut(JsValue) -> js_sys::Promise>::new(move |params: JsValue| {
                    let b2 = b2.clone();
                    let p = js_to_json(&params);
                    future_to_promise(async move { reply_to_promise_outcome(&b2.capture_frame(p).await) })
                });
                set_always("captureFrame", f2.into_js_value())?;

                let canvas_id = self.profile.canvas_id;
                let stream_fn = Closure::<dyn FnMut(JsValue) -> Result<JsValue, JsValue>>::new(move |fps: JsValue| {
                    canvas_capture_stream(canvas_id, fps.as_f64())
                });
                set_always("captureStream", stream_fn.into_js_value())?;
            }

            // 6. setCursor(cursor) & getCursor() & pointer(events)
            {
                let b = self.clone();
                let set_cursor = Closure::<dyn FnMut(JsValue) -> JsValue>::new(move |val: JsValue| {
                    let json_val = js_to_json(&val);
                    let mut cur = b.cursor.borrow_mut();
                    cur.update_from_json(&json_val);
                    sync_dom_cursor_overlay(b.profile.canvas_id, &cur);
                    json_to_js(&serde_json::to_value(&*cur).unwrap_or(Value::Null))
                });
                set_always("setCursor", set_cursor.into_js_value())?;

                let b2 = self.clone();
                let get_cursor = Closure::<dyn FnMut() -> JsValue>::new(move || {
                    json_to_js(&serde_json::to_value(&*b2.cursor.borrow()).unwrap_or(Value::Null))
                });
                set_always("getCursor", get_cursor.into_js_value())?;

                let b3 = self.clone();
                let pointer_fn = Closure::<dyn FnMut(JsValue) -> js_sys::Promise>::new(move |params: JsValue| {
                    let b3 = b3.clone();
                    let p = js_to_json(&params);
                    let (rl, rt, rw, rh) = canvas_bounding_rect(b3.profile.canvas_id);
                    let (scaled_p, dom_events) = scale_pointer_payload_for_canvas(&p, rl, rt, rw, rh);
                    {
                        let mut cur = b3.cursor.borrow_mut();
                        cur.update_from_json(&p);
                        if let Some(last_evt) = p.get("events").and_then(Value::as_array).and_then(|a| a.last()) {
                            cur.update_from_json(last_evt);
                        }
                        sync_dom_cursor_overlay(b3.profile.canvas_id, &cur);
                    }
                    for (kind, cx, cy, btn) in &dom_events {
                        synthesize_canvas_pointer_in_dom(b3.profile.canvas_id, kind, *cx, *cy, *btn);
                    }
                    if let Some(w) = b3.waker.borrow().as_ref() {
                        w();
                    }
                    let ptr_method = b3.profile.methods.pointer.unwrap_or("ui.pointer");
                    future_to_promise(async move {
                        let reply = b3.call_control(ptr_method, scaled_p).await;
                        if reply.get("ok").and_then(Value::as_bool) == Some(true) {
                            reply_to_promise_outcome(&reply)
                        } else {
                            Ok(json_to_js(&json!({ "ok": true, "dispatchedDomEvents": dom_events.len() })))
                        }
                    })
                });
                set_always("pointer", pointer_fn.into_js_value())?;
            }

            // 7. saveAck(revision) & state()
            {
                let b = self.clone();
                let ack_fn = Closure::<dyn FnMut(JsValue) -> JsValue>::new(move |rev: JsValue| {
                    let r = rev.as_f64().map(|n| n as u64);
                    let clean = b.revision.borrow_mut().confirm_saved(r, rev.as_string().as_deref());
                    JsValue::from_bool(clean)
                });
                set_always("saveAck", ack_fn.into_js_value())?;

                let b2 = self.clone();
                let state_fn = Closure::<dyn FnMut() -> JsValue>::new(move || {
                    let rev = b2.revision.borrow();
                    json_to_js(&json!({
                        "app": b2.profile.id,
                        "bridgeVersion": BRIDGE_ID,
                        "ready": b2.app_ready.get(),
                        "heldQueueCount": b2.held_queue.borrow().len(),
                        "dirty": rev.is_dirty(),
                        "revision": rev.current_revision,
                        "savedRevision": rev.saved_revision,
                        "cursor": *b2.cursor.borrow(),
                    }))
                });
                set_always("state", state_fn.into_js_value())?;

                // 8. importFile(params), openFile(params), isReady(), waitReady(timeoutMs)
                let b_imp = self.clone();
                let import_fn = Closure::<dyn FnMut(JsValue) -> js_sys::Promise>::new(move |params: JsValue| {
                    let b_imp = b_imp.clone();
                    future_to_promise(async move {
                        let reply = b_imp.import_or_open_payload(&params, true).await;
                        reply_to_promise_outcome(&reply)
                    })
                });
                set_always("importFile", import_fn.into_js_value())?;

                let b_opn = self.clone();
                let open_fn = Closure::<dyn FnMut(JsValue) -> js_sys::Promise>::new(move |params: JsValue| {
                    let b_opn = b_opn.clone();
                    future_to_promise(async move {
                        let reply = b_opn.import_or_open_payload(&params, false).await;
                        reply_to_promise_outcome(&reply)
                    })
                });
                set_if_missing("openFile", open_fn.into_js_value())?;

                let b_rdy = self.clone();
                let is_ready_fn = Closure::<dyn FnMut() -> JsValue>::new(move || {
                    JsValue::from_bool(b_rdy.app_ready.get())
                });
                set_always("isReady", is_ready_fn.into_js_value())?;

                let b_wait = self.clone();
                let wait_ready_fn = Closure::<dyn FnMut(JsValue) -> js_sys::Promise>::new(move |timeout_val: JsValue| {
                    let b_wait = b_wait.clone();
                    let timeout_ms = timeout_val.as_f64().unwrap_or(20_000.0).clamp(100.0, 60_000.0);
                    future_to_promise(async move {
                        let mut elapsed = 0.0;
                        while !b_wait.app_ready.get() && elapsed < timeout_ms {
                            sleep_ms(30.0).await;
                            elapsed += 30.0;
                        }
                        Ok(json_to_js(&json!({
                            "ok": b_wait.app_ready.get(),
                            "ready": b_wait.app_ready.get(),
                            "app": b_wait.profile.id,
                            "waitedMs": elapsed
                        })))
                    })
                });
                set_always("waitReady", wait_ready_fn.into_js_value())?;
            }

            define_window_global(&g, self.profile.js_global, &target);
            if self.profile.js_global != self.profile.id {
                define_window_global(&g, self.profile.id, &target);
            }
            define_window_global(&g, "craftUniversalBridge", &target);
            Ok(())
        }

        fn install_host_listener(&self) {
            let Some(mode) = self.mode.clone() else { return };
            let b = self.clone();
            let queue = Rc::new(DeliveryQueue::new(move |(kind, data): (IncomingKind, JsValue)| {
                b.handle_incoming(kind, data);
            }));

            if mode.uses_post_message() {
                let Some(expected_origin) = self.host_origin.clone() else { return };
                let app_id = self.profile.id;
                let q = queue.clone();
                let on_msg = Closure::<dyn Fn(JsValue)>::new(move |event: JsValue| {
                    let origin = get_prop(&event, "origin").and_then(|v| v.as_string()).unwrap_or_default();
                    let source = get_prop(&event, "source");
                    let parent = window_parent();
                    let from_parent = matches!((source, parent), (Some(s), Some(p)) if js_sys::Object::is(&s, &p));
                    if !accepts_origin(&expected_origin, &origin, from_parent) {
                        return;
                    }
                    let Some(data) = get_prop(&event, "data") else { return };
                    let Some(msg_type) = get_prop(&data, "type").and_then(|v| v.as_string()) else { return };
                    let has_marker = get_prop(&data, "bridge").and_then(|v| v.as_string()).as_deref() == Some(EMU_PROTOCOL_ID)
                        || get_prop(&data, "craftBridge").is_some_and(|v| !v.is_undefined())
                        || get_prop(&data, "photocraftEmbed").is_some_and(|v| !v.is_undefined())
                        || get_prop(&data, &format!("{app_id}Embed")).is_some_and(|v| !v.is_undefined());
                    if let Some(kind) = IncomingKind::parse(app_id, &msg_type, has_marker) {
                        q.deliver((kind, data));
                    }
                });
                add_window_listener("message", on_msg.as_ref());
                on_msg.forget();
            } else if let Some(canvas) = find_canvas(self.profile.canvas_id) {
                let app_id = self.profile.id;
                for suffix in [
                    "host",
                    "hello",
                    "ping",
                    "open",
                    "import",
                    "insert",
                    "place",
                    "save",
                    "saved",
                    "saveAck",
                    "picked",
                    "export",
                    "request",
                    "execute",
                    "cmd",
                    "commands",
                    "inspect",
                    "pointer",
                    "cursor",
                    "snapshot-request",
                    "snapshotRequest",
                    "screenshot",
                    "handoff-done",
                    "exam",
                    "autosave",
                ] {
                    let event_name = format!("{app_id}:{suffix}");
                    let Some(kind) = IncomingKind::parse(app_id, &event_name, true) else { continue };
                    let q = queue.clone();
                    let on_evt = Closure::<dyn Fn(JsValue)>::new(move |event: JsValue| {
                        let detail = get_prop(&event, "detail").unwrap_or(JsValue::NULL);
                        q.deliver((kind, detail));
                    });
                    add_target_listener(&canvas, &event_name, on_evt.as_ref());
                    on_evt.forget();
                }
            }
        }

        fn handle_incoming(&self, kind: IncomingKind, data: JsValue) {
            if !self.app_ready.get() {
                if is_canvas_and_dom_ready(self.profile.id, self.profile.canvas_id) {
                    self.app_ready.set(true);
                    set_window_bridge_ready_flag(true);
                    let ver = self.ready_version.borrow().clone();
                    let _ = self.emit_ready_now(&ver, None);
                    self.flush_held_queue();
                } else {
                    match kind {
                        IncomingKind::Hello => {
                            let held_len = self.held_queue.borrow().len() as f64;
                            let _ = self.post_to_host(
                                OutgoingKind::Loading,
                                &[
                                    ("ready", JsValue::from_bool(false)),
                                    ("status", JsValue::from_str("loading")),
                                    ("heldCount", JsValue::from_f64(held_len)),
                                ],
                                None,
                            );
                            return;
                        }
                        IncomingKind::Open
                        | IncomingKind::Import
                        | IncomingKind::Request
                        | IncomingKind::Execute
                        | IncomingKind::Pointer
                        | IncomingKind::SnapshotRequest => {
                            let mut q = self.held_queue.borrow_mut();
                            q.push((kind, data));
                            let held_len = q.len() as f64;
                            drop(q);
                            let _ = self.post_to_host(
                                OutgoingKind::Loading,
                                &[
                                    ("ready", JsValue::from_bool(false)),
                                    ("held", JsValue::from_bool(true)),
                                    ("status", JsValue::from_str("loading_held")),
                                    ("heldCount", JsValue::from_f64(held_len)),
                                ],
                                None,
                            );
                            return;
                        }
                        _ => {}
                    }
                }
            }
            self.dispatch_ready_incoming(kind, data);
        }

        fn dispatch_ready_incoming(&self, kind: IncomingKind, data: JsValue) {
            let req_id = get_prop(&data, "requestId").or_else(|| get_prop(&data, "id"));
            match kind {
                IncomingKind::Hello => {
                    let ver = self.ready_version.borrow().clone();
                    let _ = self.post_ready(&ver, None);
                }
                IncomingKind::Open | IncomingKind::Import => {
                    let is_import = kind == IncomingKind::Import;
                    let b = self.clone();
                    spawn_local(async move {
                        let reply = b.import_or_open_payload(&data, is_import).await;
                        let ok = reply.get("ok").and_then(Value::as_bool) == Some(true);
                        if !ok {
                            let err_msg = reply
                                .get("error")
                                .and_then(Value::as_str)
                                .unwrap_or("failed to import/open file");
                            let mut fields = vec![("error", JsValue::from_str(err_msg)), ("ok", JsValue::from_bool(false))];
                            if let Some(id) = req_id.filter(|v| !v.is_undefined()) {
                                fields.push(("requestId", id));
                            }
                            let _ = b.post_to_host(OutgoingKind::OpenFailed, &fields, None);
                            return;
                        }
                        let res_obj = reply.get("result").cloned().unwrap_or_else(|| json!({}));
                        let name = res_obj.get("name").and_then(Value::as_str).unwrap_or("imported");
                        let mime = res_obj.get("mime").and_then(Value::as_str).unwrap_or("application/octet-stream");
                        let size = res_obj.get("size").and_then(Value::as_u64).unwrap_or(0) as f64;
                        let rev = res_obj.get("revision").and_then(Value::as_u64).unwrap_or(1) as f64;
                        let mut fields = vec![
                            ("name", JsValue::from_str(name)),
                            ("mime", JsValue::from_str(mime)),
                            ("size", JsValue::from_f64(size)),
                            ("ok", JsValue::from_bool(true)),
                            ("warnings", js_sys::Array::new().into()),
                            ("revision", JsValue::from_f64(rev)),
                        ];
                        if let Some(id) = req_id.clone().filter(|v| !v.is_undefined()) {
                            fields.push(("requestId", id));
                        }
                        let _ = b.post_to_host(OutgoingKind::Opened, &fields, None);
                        if is_import {
                            let _ = b.post_to_host(OutgoingKind::Imported, &fields, None);
                        }
                        if req_id.is_some() {
                            b.post_control_response(req_id, if is_import { "file.import" } else { "file.open" }, &reply);
                        }
                    });
                }
                IncomingKind::Save | IncomingKind::Export => {
                    let fmt = get_prop(&data, "format").and_then(|v| v.as_string());
                    let trigger_res = self
                        .save_trigger_cb
                        .borrow_mut()
                        .as_mut()
                        .map(|cb| cb(fmt.clone()));
                    if let Some(res) = trigger_res {
                        match res {
                            Ok((name, bytes)) => {
                                let _ = self.post_save(&name, &bytes, false, req_id.as_ref());
                            }
                            Err(e) => {
                                let _ = self.post_to_host(OutgoingKind::SaveFailed, &[("error", e.as_str().into())], None);
                            }
                        }
                    } else {
                        let b = self.clone();
                        spawn_local(async move {
                            let (method, payload) = b.profile.methods.format_execute_params("file.save", json!({}));
                            let reply = b.call_control(&method, payload).await;
                            b.post_control_response(req_id, "file.save", &reply);
                        });
                    }
                }
                IncomingKind::Saved => {
                    let rev = get_prop(&data, "revision").and_then(|v| v.as_f64()).map(|n| n as u64);
                    let id = get_prop(&data, "id").and_then(|v| v.as_string());
                    let clean = self.revision.borrow_mut().confirm_saved(rev, id.as_deref());
                    if clean {
                        self.report_dirty(false);
                    }
                }
                IncomingKind::Picked => {
                    // Handled by app dialog hooks when pending save-location requests exist.
                }
                IncomingKind::Request => {
                    let method = get_prop(&data, "method")
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();
                    let params = get_prop(&data, "params")
                        .map(|v| js_to_json(&v))
                        .unwrap_or_else(|| json!({}));
                    let b = self.clone();
                    spawn_local(async move {
                        let reply = b.call_control(&method, params).await;
                        b.post_control_response(req_id, &method, &reply);
                    });
                }
                IncomingKind::Execute => {
                    let command = get_prop(&data, "command")
                        .or_else(|| get_prop(&data, "id"))
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();
                    let params = get_prop(&data, "params")
                        .map(|v| {
                            if let Some(s) = v.as_string() {
                                serde_json::from_str(&s).unwrap_or_else(|_| json!({}))
                            } else {
                                js_to_json(&v)
                            }
                        })
                        .unwrap_or_else(|| json!({}));
                    let (method, payload) = self.profile.methods.format_execute_params(&command, params);
                    let b = self.clone();
                    spawn_local(async move {
                        let reply = b.call_control(&method, payload).await;
                        b.post_control_response(req_id, &command, &reply);
                    });
                }
                IncomingKind::Commands => {
                    let cmd_method = self.profile.methods.commands.unwrap_or("engine.commands");
                    let b = self.clone();
                    spawn_local(async move {
                        let reply = enrich_commands_with_schema(b.call_control(cmd_method, json!({})).await);
                        b.post_control_response(req_id, cmd_method, &reply);
                    });
                }
                IncomingKind::Inspect => {
                    let target = get_prop(&data, "target").and_then(|v| v.as_string());
                    let params = get_prop(&data, "params")
                        .map(|v| js_to_json(&v))
                        .unwrap_or_else(|| json!({}));
                    let b = self.clone();
                    spawn_local(async move {
                        let (method_label, reply) = if target.as_deref() == Some("ui") {
                            let m = b.profile.methods.ui_inspect.unwrap_or("ui.inspect");
                            (m, b.call_control(m, params).await)
                        } else {
                            let m = b
                                .profile
                                .methods
                                .doc_inspect
                                .or(b.profile.methods.ui_inspect)
                                .unwrap_or("ui.inspect");
                            (m, b.call_doc_inspect(params).await)
                        };
                        b.post_control_response(req_id, method_label, &reply);
                    });
                }
                IncomingKind::Cursor => {
                    let json_val = js_to_json(&data);
                    let mut cur = self.cursor.borrow_mut();
                    cur.update_from_json(&json_val);
                    sync_dom_cursor_overlay(self.profile.canvas_id, &cur);
                }
                IncomingKind::Pointer => {
                    let json_val = js_to_json(&data);
                    let (rl, rt, rw, rh) = canvas_bounding_rect(self.profile.canvas_id);
                    let (scaled_val, dom_events) = scale_pointer_payload_for_canvas(&json_val, rl, rt, rw, rh);
                    {
                        let mut cur = self.cursor.borrow_mut();
                        cur.update_from_json(&json_val);
                        if let Some(last_evt) = json_val.get("events").and_then(Value::as_array).and_then(|a| a.last()) {
                            cur.update_from_json(last_evt);
                        }
                        sync_dom_cursor_overlay(self.profile.canvas_id, &cur);
                    }
                    for (kind, cx, cy, btn) in &dom_events {
                        synthesize_canvas_pointer_in_dom(self.profile.canvas_id, kind, *cx, *cy, *btn);
                    }
                    if let Some(w) = self.waker.borrow().as_ref() {
                        w();
                    }
                    let ptr_method = self.profile.methods.pointer.unwrap_or("ui.pointer");
                    let b = self.clone();
                    let dom_count = dom_events.len();
                    spawn_local(async move {
                        let mut reply = b.call_control(ptr_method, scaled_val).await;
                        if reply.get("ok").and_then(Value::as_bool) != Some(true) {
                            reply = json!({
                                "ok": true,
                                "result": { "dispatchedDomEvents": dom_count }
                            });
                        }
                        b.post_control_response(req_id, ptr_method, &reply);
                    });
                }
                IncomingKind::SnapshotRequest => {
                    let params = js_to_json(&data);
                    let b = self.clone();
                    spawn_local(async move {
                        let reply = b.capture_frame(params).await;
                        let ok = reply.get("ok").and_then(Value::as_bool) == Some(true);
                        let rev = b.revision.borrow().current_revision;
                        let b64 = extract_screenshot_base64(&reply).unwrap_or_default().to_string();
                        let png_bytes = base64_decode(&b64).unwrap_or_default();
                        let buffer = js_sys::Uint8Array::from(png_bytes.as_slice()).buffer();
                        let w = reply["result"]["width"].as_u64().unwrap_or(0) as f64;
                        let h = reply["result"]["height"].as_u64().unwrap_or(0) as f64;
                        let cursor_js = json_to_js(&serde_json::to_value(&*b.cursor.borrow()).unwrap_or(Value::Null));
                        let mut fields = vec![
                            ("ok", JsValue::from_bool(ok)),
                            ("format", JsValue::from_str("png")),
                            ("width", JsValue::from_f64(w)),
                            ("height", JsValue::from_f64(h)),
                            ("data", JsValue::from_str(&b64)),
                            ("bytes", buffer.clone().into()),
                            ("cursor", cursor_js),
                            ("revision", JsValue::from_f64(rev as f64)),
                        ];
                        if let Some(id) = req_id.filter(|v| !v.is_undefined()) {
                            fields.push(("requestId", id));
                        }
                        let _ = b.post_to_host(OutgoingKind::Snapshot, &fields, Some(&buffer));
                    });
                }
                IncomingKind::Exam => {
                    let on = get_prop(&data, "on").and_then(|v| v.as_bool()).unwrap_or(true);
                    self.exam_locked.set(on);
                }
                IncomingKind::Autosave | IncomingKind::HandoffDone => {}
            }
        }

        fn post_control_response(&self, req_id: Option<JsValue>, method_or_cmd: &str, reply: &Value) {
            let ok = reply.get("ok").and_then(Value::as_bool) == Some(true);
            let rev = self.revision.borrow().current_revision;
            let result_js = json_to_js(reply.get("result").unwrap_or(&Value::Null));
            let error_js = reply
                .get("error")
                .and_then(Value::as_str)
                .map_or(JsValue::NULL, JsValue::from_str);
            let value_str = if ok {
                reply.get("result").map(Value::to_string).unwrap_or_else(|| "null".to_string())
            } else {
                reply
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("error")
                    .to_string()
            };
            let mut fields = vec![
                ("method", JsValue::from_str(method_or_cmd)),
                ("command", JsValue::from_str(method_or_cmd)),
                ("ok", JsValue::from_bool(ok)),
                ("result", result_js.clone()),
                ("commands", result_js),
                ("value", JsValue::from_str(&value_str)),
                ("error", error_js),
                ("revision", JsValue::from_f64(rev as f64)),
            ];
            if let Some(id) = req_id.filter(|v| !v.is_undefined()) {
                fields.push(("requestId", id.clone()));
                fields.push(("id", id));
            }
            let _ = self.post_to_host(OutgoingKind::Response, &fields, None);
        }

        fn post_to_host(&self, kind: OutgoingKind, fields: &[(&str, JsValue)], transfer: Option<&js_sys::ArrayBuffer>) -> bool {
            let Some(mode) = &self.mode else { return false };
            let msg_type = kind.message_type(self.profile.id);
            let obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(&msg_type));
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("kind"), &JsValue::from_str(kind.kind()));
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("app"), &JsValue::from_str(self.profile.id));
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("craftBridge"), &JsValue::from_str(BRIDGE_ID));
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("bridge"), &JsValue::from_str(EMU_PROTOCOL_ID));
            if matches!(mode, HostMode::Embed) {
                let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("photocraftEmbed"), &JsValue::from_f64(1.0));
                let _ = js_sys::Reflect::set(
                    &obj,
                    &JsValue::from_str(&format!("{}Embed", self.profile.id)),
                    &JsValue::from_f64(1.0),
                );
            }
            for (k, v) in fields {
                let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(k), v);
            }

            if mode.uses_post_message() {
                let Some(origin) = &self.host_origin else { return false };
                let Some(parent) = window_parent() else { return false };
                let post_fn = match get_prop(&parent, "postMessage").filter(|v| v.is_function()) {
                    Some(f) => f.unchecked_into::<js_sys::Function>(),
                    None => return false,
                };
                match transfer {
                    Some(buf) => post_fn
                        .call3(&parent, &obj, &JsValue::from_str(origin), &js_sys::Array::of1(buf))
                        .is_ok(),
                    None => post_fn.call2(&parent, &obj, &JsValue::from_str(origin)).is_ok(),
                }
            } else {
                let Some(canvas) = find_canvas(self.profile.canvas_id) else { return false };
                dispatch_custom_event(&canvas, &msg_type, &obj)
            }
        }
    }

    fn normalize_control_reply(reply: Value) -> Value {
        if reply.is_object() && reply.get("ok").is_some_and(Value::is_boolean) {
            reply
        } else {
            json!({
                "ok": true,
                "result": reply
            })
        }
    }

    fn define_window_global(g: &js_sys::Object, name: &str, target: &js_sys::Object) {
        let desc = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&desc, &JsValue::from_str("value"), target);
        let _ = js_sys::Reflect::set(&desc, &JsValue::from_str("configurable"), &JsValue::from_bool(true));
        let _ = js_sys::Reflect::set(&desc, &JsValue::from_str("enumerable"), &JsValue::from_bool(true));
        let _ = js_sys::Reflect::set(&desc, &JsValue::from_str("writable"), &JsValue::from_bool(true));
        if !js_sys::Reflect::define_property(g, &JsValue::from_str(name), &desc).unwrap_or(false) {
            let _ = js_sys::Reflect::set(g, &JsValue::from_str(name), target);
        }
    }

    fn wake_canvas(canvas_id: &str, step: u32) {
        let Some(canvas) = find_canvas(canvas_id) else { return };
        let g = js_sys::global();
        let Some(ctor) = get_prop(&g.into(), "MouseEvent").filter(|v| v.is_function()) else { return };
        let ctor: js_sys::Function = ctor.unchecked_into();
        let init = js_sys::Object::new();
        let x = 2.0 + f64::from(step % 4);
        let y = 2.0 + f64::from(step % 3);
        let _ = js_sys::Reflect::set(&init, &JsValue::from_str("clientX"), &JsValue::from_f64(x));
        let _ = js_sys::Reflect::set(&init, &JsValue::from_str("clientY"), &JsValue::from_f64(y));
        let _ = js_sys::Reflect::set(&init, &JsValue::from_str("bubbles"), &JsValue::from_bool(true));
        let args = js_sys::Array::of2(&JsValue::from_str("mousemove"), &init);
        if let Ok(evt) = js_sys::Reflect::construct(&ctor, &args)
            && let Some(dispatch) = get_prop(&canvas, "dispatchEvent").filter(|v| v.is_function())
        {
            let dispatch: js_sys::Function = dispatch.unchecked_into();
            let _ = dispatch.call1(&canvas, &evt);
        }
    }

    async fn wait_receiver(rx: Receiver<Value>, timeout_ms: f64) -> Value {
        let mut elapsed = 0.0;
        loop {
            match rx.try_recv() {
                Ok(reply) => return reply,
                Err(TryRecvError::Empty) => {
                    if elapsed >= timeout_ms {
                        return json!({
                            "ok": false,
                            "error": format!("control request timed out after {} s", timeout_ms / 1000.0)
                        });
                    }
                    sleep_ms(POLL_MS).await;
                    elapsed += POLL_MS;
                }
                Err(TryRecvError::Disconnected) => {
                    return json!({"ok": false, "error": "control channel disconnected"});
                }
            }
        }
    }

    async fn sleep_ms(ms: f64) {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            let g = js_sys::global();
            if let Some(f) = get_prop(&g, "setTimeout").filter(|v| v.is_function()) {
                let f: js_sys::Function = f.unchecked_into();
                let _ = f.call2(&g, &resolve, &JsValue::from_f64(ms));
            }
        });
        let _ = JsFuture::from(promise).await;
    }

    fn get_prop(target: &JsValue, key: &str) -> Option<JsValue> {
        if target.is_null() || target.is_undefined() {
            return None;
        }
        js_sys::Reflect::get(target, &JsValue::from_str(key)).ok()
    }

    fn location_search() -> String {
        get_prop(&js_sys::global().into(), "location")
            .and_then(|loc| get_prop(&loc, "search"))
            .and_then(|s| s.as_string())
            .unwrap_or_default()
    }

    fn location_origin() -> String {
        get_prop(&js_sys::global().into(), "location")
            .and_then(|loc| get_prop(&loc, "origin"))
            .and_then(|s| s.as_string())
            .unwrap_or_default()
    }

    fn window_parent() -> Option<JsValue> {
        let g: JsValue = js_sys::global().into();
        let p = get_prop(&g, "parent")?;
        (!p.is_null() && !p.is_undefined()).then_some(p)
    }

    fn has_distinct_parent() -> bool {
        let g: JsValue = js_sys::global().into();
        window_parent().is_some_and(|p| !js_sys::Object::is(&p, &g))
    }

    fn parent_origin() -> Option<String> {
        let p = window_parent()?;
        get_prop(&p, "location")
            .and_then(|loc| get_prop(&loc, "origin"))
            .and_then(|s| s.as_string())
    }

    fn find_canvas(canvas_id: &str) -> Option<JsValue> {
        let doc = get_prop(&js_sys::global().into(), "document")?;
        let get_el: js_sys::Function = get_prop(&doc, "getElementById")?.dyn_into().ok()?;
        let el = get_el.call1(&doc, &JsValue::from_str(canvas_id)).ok()?;
        if !el.is_null() && !el.is_undefined() {
            return Some(el);
        }
        let qs: js_sys::Function = get_prop(&doc, "querySelector")?.dyn_into().ok()?;
        let fallback = qs.call1(&doc, &JsValue::from_str("canvas")).ok()?;
        (!fallback.is_null() && !fallback.is_undefined()).then_some(fallback)
    }

    fn canvas_attribute(canvas_id: &str, attr: &str) -> Option<String> {
        let canvas = find_canvas(canvas_id)?;
        let get_attr: js_sys::Function = get_prop(&canvas, "getAttribute")?.dyn_into().ok()?;
        get_attr.call1(&canvas, &JsValue::from_str(attr)).ok()?.as_string()
    }

    fn add_window_listener(event_name: &str, callback: &JsValue) {
        let g: JsValue = js_sys::global().into();
        add_target_listener(&g, event_name, callback);
    }

    fn add_target_listener(target: &JsValue, event_name: &str, callback: &JsValue) {
        if let Some(f) = get_prop(target, "addEventListener").filter(|v| v.is_function()) {
            let f: js_sys::Function = f.unchecked_into();
            let _ = f.call2(target, &JsValue::from_str(event_name), callback);
        }
    }

    fn dispatch_custom_event(target: &JsValue, event_name: &str, detail: &js_sys::Object) -> bool {
        let g = js_sys::global();
        let Some(ctor) = get_prop(&g.into(), "CustomEvent").filter(|v| v.is_function()) else {
            return false;
        };
        let ctor: js_sys::Function = ctor.unchecked_into();
        let init = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&init, &JsValue::from_str("detail"), detail);
        let _ = js_sys::Reflect::set(&init, &JsValue::from_str("bubbles"), &JsValue::from_bool(false));
        let args = js_sys::Array::of2(&JsValue::from_str(event_name), &init);
        let Ok(evt) = js_sys::Reflect::construct(&ctor, &args) else {
            return false;
        };
        let Some(dispatch) = get_prop(target, "dispatchEvent").filter(|v| v.is_function()) else {
            return false;
        };
        let dispatch: js_sys::Function = dispatch.unchecked_into();
        dispatch.call1(target, &evt).is_ok()
    }

    fn extract_bytes(data: &JsValue) -> Option<Vec<u8>> {
        for field in ["bytes", "document", "data", "buffer"] {
            if let Some(val) = get_prop(data, field) {
                if let Some(arr) = val.dyn_ref::<js_sys::Uint8Array>() {
                    return Some(arr.to_vec());
                }
                if val.is_instance_of::<js_sys::ArrayBuffer>() {
                    return Some(js_sys::Uint8Array::new(&val).to_vec());
                }
            }
        }
        for field in ["base64", "bytes_b64", "dataUrl", "data_url", "data"] {
            if let Some(s) = get_prop(data, field).and_then(|v| v.as_string()) {
                let trimmed = s.trim();
                if let Some((_, b64_part)) = trimmed.split_once(";base64,") {
                    if let Some(decoded) = base64_decode(b64_part) {
                        return Some(decoded);
                    }
                } else if !trimmed.is_empty()
                    && trimmed.len() >= 4
                    && trimmed
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_' | '\r' | '\n'))
                    && let Some(decoded) = base64_decode(trimmed)
                    && !decoded.is_empty()
                {
                    return Some(decoded);
                }
            }
        }
        for field in ["text", "content", "csv", "svg", "json", "data"] {
            if let Some(s) = get_prop(data, field).and_then(|v| v.as_string()) {
                if !s.is_empty() {
                    return Some(s.into_bytes());
                }
            }
        }
        None
    }

    fn extract_fonts(data: &JsValue) -> Vec<(String, Vec<u8>)> {
        let Some(fonts_val) = get_prop(data, "fonts").filter(|v| js_sys::Array::is_array(v)) else {
            return Vec::new();
        };
        let arr = js_sys::Array::from(&fonts_val);
        let mut out = Vec::new();
        for item in arr.iter() {
            let family = get_prop(&item, "family")
                .or_else(|| get_prop(&item, "name"))
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            if let Some(bytes) = extract_bytes(&item)
                && !family.is_empty()
            {
                out.push((family, bytes));
            }
        }
        out
    }

    /// Update or create the non-blocking DOM shared cursor overlay `#<canvas_id>_agent_cursor`
    /// above the app canvas so human operators and browser captures see the live agent cursor.
    fn sync_dom_cursor_overlay(canvas_id: &str, cursor: &CursorState) {
        let Some(doc) = get_prop(&js_sys::global().into(), "document") else { return };
        let Some(get_el) = get_prop(&doc, "getElementById").and_then(|v| v.dyn_into::<js_sys::Function>().ok()) else {
            return;
        };
        let overlay_id = format!("{canvas_id}_agent_cursor");
        let existing = get_el.call1(&doc, &JsValue::from_str(&overlay_id)).ok();
        let el = match existing {
            Some(e) if !e.is_null() && !e.is_undefined() => e,
            _ => {
                if !cursor.visible {
                    return;
                }
                let Some(create_el) = get_prop(&doc, "createElement").and_then(|v| v.dyn_into::<js_sys::Function>().ok()) else {
                    return;
                };
                let Ok(div) = create_el.call1(&doc, &JsValue::from_str("div")) else { return };
                let _ = js_sys::Reflect::set(&div, &JsValue::from_str("id"), &JsValue::from_str(&overlay_id));
                if let Some(body) = get_prop(&doc, "body")
                    && let Some(append) = get_prop(&body, "appendChild").and_then(|v| v.dyn_into::<js_sys::Function>().ok())
                {
                    let _ = append.call1(&body, &div);
                }
                div
            }
        };

        let Some(style) = get_prop(&el, "style") else { return };
        if !cursor.visible {
            let _ = js_sys::Reflect::set(&style, &JsValue::from_str("display"), &JsValue::from_str("none"));
            return;
        }
        let (rl, rt, rw, rh) = canvas_bounding_rect(canvas_id);
        let (_, _, _, _, client_x, client_y) =
            resolve_canvas_point(cursor.x, cursor.y, Some(cursor.normalized), rl, rt, rw, rh);
        let left = format!("{client_x:.1}px");
        let top = format!("{client_y:.1}px");
        let css = format!(
            "position:fixed;left:{left};top:{top};width:16px;height:16px;margin-left:-8px;margin-top:-8px;             border:2px solid {color};border-radius:50%;pointer-events:none;z-index:2147483647;             box-sizing:border-box;display:block;box-shadow:0 0 6px rgba(0,0,0,0.65);",
            color = cursor.color
        );
        let _ = js_sys::Reflect::set(&style, &JsValue::from_str("cssText"), &JsValue::from_str(&css));
        let _ = js_sys::Reflect::set(&el, &JsValue::from_str("title"), &JsValue::from_str(&cursor.label));
    }

    /// Direct canvas PNG capture via offscreen 2D compositing (or `HTMLCanvasElement.toDataURL("image/png")`),
    /// preserving the rendered WebGL/2D buffer and compositing the shared agent cursor when requested.
    fn capture_canvas_png_base64(canvas_id: &str, include_cursor: bool, cursor: &CursorState) -> Option<(u32, u32, String)> {
        let g: JsValue = js_sys::global().into();
        if let Some(comp_fn) = get_prop(&g, "__craftCompositeCanvasSnapshot").filter(|v| v.is_function()) {
            let comp_fn: js_sys::Function = comp_fn.unchecked_into();
            let cur_js = json_to_js(&serde_json::to_value(cursor).unwrap_or(Value::Null));
            if let Ok(res) = comp_fn.call3(
                &g,
                &JsValue::from_str(canvas_id),
                &JsValue::from_bool(include_cursor),
                &cur_js,
            ) {
                let w = get_prop(&res, "width").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let h = get_prop(&res, "height").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                if let Some(b64) = get_prop(&res, "data").and_then(|v| v.as_string()).filter(|s| !s.is_empty()) {
                    return Some((w.max(1), h.max(1), b64));
                }
            }
        }
        let canvas = find_canvas(canvas_id)?;
        let w = get_prop(&canvas, "width").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        let h = get_prop(&canvas, "height").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        let to_data_url: js_sys::Function = get_prop(&canvas, "toDataURL")?.dyn_into().ok()?;
        let data_url = to_data_url
            .call1(&canvas, &JsValue::from_str("image/png"))
            .ok()?
            .as_string()?;
        let b64 = data_url.strip_prefix("data:image/png;base64,")?.to_string();
        Some((w, h, b64))
    }

    fn canvas_bounding_rect(canvas_id: &str) -> (f64, f64, f64, f64) {
        let Some(canvas) = find_canvas(canvas_id) else {
            return (0.0, 0.0, 1024.0, 720.0);
        };
        if let Some(rect_fn) = get_prop(&canvas, "getBoundingClientRect").filter(|v| v.is_function()) {
            let rect_fn: js_sys::Function = rect_fn.unchecked_into();
            if let Ok(rect) = rect_fn.call0(&canvas) {
                let left = get_prop(&rect, "left").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let top = get_prop(&rect, "top").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let width = get_prop(&rect, "width").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let height = get_prop(&rect, "height").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if width > 10.0 && height > 10.0 {
                    return (left, top, width, height);
                }
            }
        }
        let w = get_prop(&canvas, "clientWidth")
            .and_then(|v| v.as_f64())
            .filter(|n| *n > 10.0)
            .or_else(|| get_prop(&canvas, "width").and_then(|v| v.as_f64()))
            .unwrap_or(1024.0);
        let h = get_prop(&canvas, "clientHeight")
            .and_then(|v| v.as_f64())
            .filter(|n| *n > 10.0)
            .or_else(|| get_prop(&canvas, "height").and_then(|v| v.as_f64()))
            .unwrap_or(720.0);
        (0.0, 0.0, w.max(1.0), h.max(1.0))
    }

    fn is_canvas_and_dom_ready(app_id: &str, canvas_id: &str) -> bool {
        let Some(canvas) = find_canvas(canvas_id) else {
            return false;
        };
        let cw = get_prop(&canvas, "width").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let ch = get_prop(&canvas, "height").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if cw < 64.0 || ch < 64.0 {
            return false;
        }
        let Some(doc) = get_prop(&js_sys::global().into(), "document") else {
            return true;
        };
        let Some(get_el) = get_prop(&doc, "getElementById").and_then(|v| v.dyn_into::<js_sys::Function>().ok()) else {
            return true;
        };
        for lid in [format!("{app_id}_loading"), "loading".to_string()] {
            if let Ok(el) = get_el.call1(&doc, &JsValue::from_str(&lid))
                && !el.is_null()
                && !el.is_undefined()
            {
                let hidden = get_prop(&el, "style")
                    .and_then(|s| get_prop(&s, "display"))
                    .and_then(|d| d.as_string())
                    .is_some_and(|d| d == "none");
                if !hidden {
                    return false;
                }
            }
        }
        true
    }

    fn set_window_bridge_ready_flag(ready: bool) {
        let g: JsValue = js_sys::global().into();
        let _ = js_sys::Reflect::set(&g, &JsValue::from_str("__craftBridgeReady"), &JsValue::from_bool(ready));
    }

    fn drain_window_preboot_queue(app_id: &str) -> Vec<(IncomingKind, JsValue)> {
        let g: JsValue = js_sys::global().into();
        let Some(arr_val) = get_prop(&g, "__craftPreBootQueue").filter(|v| js_sys::Array::is_array(v)) else {
            return Vec::new();
        };
        let arr = js_sys::Array::from(&arr_val);
        let mut out = Vec::new();
        while arr.length() > 0 {
            let item = arr.shift();
            let Some(msg_type) = get_prop(&item, "type").and_then(|v| v.as_string()) else {
                continue;
            };
            let has_marker = get_prop(&item, "bridge").and_then(|v| v.as_string()).as_deref() == Some(EMU_PROTOCOL_ID)
                || get_prop(&item, "craftBridge").is_some_and(|v| !v.is_undefined())
                || get_prop(&item, "photocraftEmbed").is_some_and(|v| !v.is_undefined())
                || get_prop(&item, &format!("{app_id}Embed")).is_some_and(|v| !v.is_undefined());
            if let Some(kind) = IncomingKind::parse(app_id, &msg_type, has_marker) {
                out.push((kind, item));
            }
        }
        out
    }

    fn install_dom_hooks(app_id: &str, canvas_id: &str) {
        let script = r#"
            if (!window.__craftDomHooksInstalled) {
                window.__craftDomHooksInstalled = true;
                window.__craftPendingImportFiles = window.__craftPendingImportFiles || [];
                if (typeof HTMLCanvasElement !== 'undefined' && HTMLCanvasElement.prototype.getContext) {
                    const origGetCtx = HTMLCanvasElement.prototype.getContext;
                    HTMLCanvasElement.prototype.getContext = function(type, attrs) {
                        if (type === 'webgl' || type === 'webgl2' || type === 'experimental-webgl') {
                            attrs = Object.assign({}, attrs || {}, { preserveDrawingBuffer: true });
                        }
                        return origGetCtx.call(this, type, attrs);
                    };
                }
                if (typeof HTMLInputElement !== 'undefined' && HTMLInputElement.prototype.click) {
                    const origClick = HTMLInputElement.prototype.click;
                    HTMLInputElement.prototype.click = function() {
                        if (this && this.type === 'file' && window.__craftPendingImportFiles && window.__craftPendingImportFiles.length > 0) {
                            const f = window.__craftPendingImportFiles.shift();
                            window.__craftLastImportedFile = f;
                            try {
                                const dt = new DataTransfer();
                                dt.items.add(f);
                                this.files = dt.files;
                                this.dispatchEvent(new Event('input', { bubbles: true }));
                                this.dispatchEvent(new Event('change', { bubbles: true }));
                                return;
                            } catch (_) {}
                        }
                        return origClick.apply(this, arguments);
                    };
                }
                const origShowOpen = window.showOpenFilePicker;
                window.showOpenFilePicker = async function(opts) {
                    if (window.__craftPendingImportFiles && window.__craftPendingImportFiles.length > 0) {
                        const f = window.__craftPendingImportFiles.shift();
                        window.__craftLastImportedFile = f;
                        return [{
                            kind: 'file',
                            name: f.name,
                            getFile: async () => f,
                            queryPermission: async () => 'granted',
                            requestPermission: async () => 'granted'
                        }];
                    }
                    if (typeof origShowOpen === 'function') {
                        return origShowOpen.apply(this, arguments);
                    }
                    throw new Error('No pending file in __craftPendingImportFiles');
                };
                window.__craftStageImportFile = function(canvasId, name, mime, uint8) {
                    try {
                        const file = new File([uint8], name || 'imported.bin', { type: mime || 'application/octet-stream', lastModified: Date.now() });
                        window.__craftPendingImportFiles = window.__craftPendingImportFiles || [];
                        window.__craftPendingImportFiles.push(file);
                        window.__craftLastImportedFile = file;
                        const canvas = document.getElementById(canvasId) || document.querySelector('canvas');
                        if (canvas && typeof DataTransfer !== 'undefined' && typeof DragEvent !== 'undefined') {
                            const dt = new DataTransfer();
                            dt.items.add(file);
                            const rect = canvas.getBoundingClientRect();
                            const cx = rect.left + rect.width * 0.5;
                            const cy = rect.top + rect.height * 0.5;
                            for (const evType of ['dragenter', 'dragover', 'drop']) {
                                const ev = new DragEvent(evType, {
                                    bubbles: true,
                                    cancelable: true,
                                    clientX: cx,
                                    clientY: cy,
                                    dataTransfer: dt
                                });
                                canvas.dispatchEvent(ev);
                            }
                        }
                        return true;
                    } catch (_) {
                        return false;
                    }
                };
                window.__craftSynthesizeCanvasPointer = function(canvasId, kind, clientX, clientY, button) {
                    try {
                        const canvas = document.getElementById(canvasId) || document.querySelector('canvas');
                        if (!canvas) return false;
                        const btn = Number(button) || 0;
                        const isDown = kind === 'down' || kind === 'pointerdown' || kind === 'mousedown';
                        const isUp = kind === 'up' || kind === 'pointerup' || kind === 'mouseup';
                        const isClick = kind === 'click';
                        const buttons = isDown ? (1 << btn) : 0;
                        const baseInit = {
                            bubbles: true,
                            cancelable: true,
                            composed: true,
                            view: window,
                            clientX: Number(clientX) || 0,
                            clientY: Number(clientY) || 0,
                            screenX: Number(clientX) || 0,
                            screenY: Number(clientY) || 0,
                            button: btn,
                            buttons: buttons,
                            pointerId: 1,
                            pointerType: 'mouse',
                            isPrimary: true,
                            width: 1,
                            height: 1,
                            pressure: isDown ? 0.5 : 0.0
                        };
                        if (kind === 'move' || kind === 'pointermove' || kind === 'mousemove') {
                            if (typeof PointerEvent !== 'undefined') canvas.dispatchEvent(new PointerEvent('pointermove', baseInit));
                            canvas.dispatchEvent(new MouseEvent('mousemove', baseInit));
                        } else if (isDown) {
                            try { if (typeof canvas.focus === 'function') canvas.focus({ preventScroll: true }); } catch (_) {}
                            if (typeof PointerEvent !== 'undefined') {
                                canvas.dispatchEvent(new PointerEvent('pointerover', baseInit));
                                canvas.dispatchEvent(new PointerEvent('pointerenter', baseInit));
                                canvas.dispatchEvent(new PointerEvent('pointermove', baseInit));
                                canvas.dispatchEvent(new PointerEvent('pointerdown', baseInit));
                            }
                            canvas.dispatchEvent(new MouseEvent('mouseover', baseInit));
                            canvas.dispatchEvent(new MouseEvent('mousemove', baseInit));
                            canvas.dispatchEvent(new MouseEvent('mousedown', baseInit));
                        } else if (isUp) {
                            if (typeof PointerEvent !== 'undefined') canvas.dispatchEvent(new PointerEvent('pointerup', baseInit));
                            canvas.dispatchEvent(new MouseEvent('mouseup', baseInit));
                        } else if (isClick) {
                            canvas.dispatchEvent(new MouseEvent('click', Object.assign({}, baseInit, { detail: 1 })));
                        }
                        return true;
                    } catch (_) {
                        return false;
                    }
                };
                window.__craftCompositeCanvasSnapshot = function(canvasId, includeCursor, cursor) {
                    try {
                        const canvas = document.getElementById(canvasId) || document.querySelector('canvas');
                        if (!canvas) return null;
                        const w = Math.max(1, canvas.width || canvas.clientWidth || 1024);
                        const h = Math.max(1, canvas.height || canvas.clientHeight || 720);
                        const off = document.createElement('canvas');
                        off.width = w;
                        off.height = h;
                        const ctx = off.getContext('2d');
                        if (!ctx) return null;
                        ctx.drawImage(canvas, 0, 0, w, h);
                        if (includeCursor && cursor && cursor.visible) {
                            const nx = cursor.normalized ? Math.max(0, Math.min(1, Number(cursor.x) || 0.5)) : Math.max(0, Math.min(1, (Number(cursor.x) || 0) / w));
                            const ny = cursor.normalized ? Math.max(0, Math.min(1, Number(cursor.y) || 0.5)) : Math.max(0, Math.min(1, (Number(cursor.y) || 0) / h));
                            const cx = nx * w;
                            const cy = ny * h;
                            const col = cursor.color || '#38bdf8';
                            ctx.save();
                            ctx.strokeStyle = col;
                            ctx.lineWidth = 2.5;
                            ctx.beginPath();
                            ctx.arc(cx, cy, cursor.pressed ? 10 : 8, 0, Math.PI * 2);
                            ctx.stroke();
                            ctx.fillStyle = col;
                            ctx.beginPath();
                            ctx.arc(cx, cy, 2.5, 0, Math.PI * 2);
                            ctx.fill();
                            ctx.restore();
                        }
                        const dataUrl = off.toDataURL('image/png');
                        const prefix = 'data:image/png;base64,';
                        if (dataUrl && dataUrl.startsWith(prefix)) {
                            return { width: w, height: h, data: dataUrl.slice(prefix.length) };
                        }
                        return null;
                    } catch (_) {
                        return null;
                    }
                };
            }
        "#;
        let f = js_sys::Function::new_with_args("appId,canvasId", script);
        let _ = f.call2(
            &js_sys::global(),
            &JsValue::from_str(app_id),
            &JsValue::from_str(canvas_id),
        );
    }

    fn stage_and_drop_file_in_dom(canvas_id: &str, name: &str, mime: &str, bytes: &[u8]) {
        let g: JsValue = js_sys::global().into();
        let Some(f) = get_prop(&g, "__craftStageImportFile").filter(|v| v.is_function()) else {
            return;
        };
        let f: js_sys::Function = f.unchecked_into();
        let u8_arr = js_sys::Uint8Array::from(bytes);
        let _ = f.call4(
            &g,
            &JsValue::from_str(canvas_id),
            &JsValue::from_str(name),
            &JsValue::from_str(mime),
            &u8_arr.into(),
        );
    }

    fn synthesize_canvas_pointer_in_dom(canvas_id: &str, kind: &str, client_x: f64, client_y: f64, button: i16) {
        let g: JsValue = js_sys::global().into();
        let Some(f) = get_prop(&g, "__craftSynthesizeCanvasPointer").filter(|v| v.is_function()) else {
            return;
        };
        let f: js_sys::Function = f.unchecked_into();
        let argv = js_sys::Array::of5(
            &JsValue::from_str(canvas_id),
            &JsValue::from_str(kind),
            &JsValue::from_f64(client_x),
            &JsValue::from_f64(client_y),
            &JsValue::from_f64(f64::from(button)),
        );
        let _ = js_sys::Reflect::apply(&f, &g, &argv);
    }

    /// Call `HTMLCanvasElement.prototype.captureStream(fps)` on the app's primary canvas element.
    fn canvas_capture_stream(canvas_id: &str, fps: Option<f64>) -> Result<JsValue, JsValue> {
        let canvas = find_canvas(canvas_id).ok_or_else(|| js_sys::Error::new("app canvas not found"))?;
        let proto: JsValue = js_sys::Object::get_prototype_of(&canvas).into();
        let proto_fn = get_prop(&proto, "captureStream").filter(|v| v.is_function());
        let f: js_sys::Function = proto_fn
            .or_else(|| get_prop(&canvas, "captureStream"))
            .ok_or_else(|| js_sys::Error::new("canvas.captureStream is not supported in this browser"))?
            .dyn_into()?;
        match fps.filter(|n| n.is_finite() && *n >= 0.0) {
            Some(rate) => f.call1(&canvas, &JsValue::from_f64(rate)),
            None => f.call0(&canvas),
        }
    }

    fn reply_to_promise_outcome(reply: &Value) -> Result<JsValue, JsValue> {
        if reply.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(json_to_js(reply.get("result").unwrap_or(&Value::Null)))
        } else {
            let msg = match reply.get("error") {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => "error".to_string(),
            };
            Err(js_sys::Error::new(&msg).into())
        }
    }

    pub fn json_to_js(v: &Value) -> JsValue {
        js_sys::JSON::parse(&v.to_string()).unwrap_or(JsValue::NULL)
    }

    pub fn js_to_json(v: &JsValue) -> Value {
        if v.is_undefined() || v.is_null() {
            return json!({});
        }
        js_sys::JSON::stringify(v)
            .ok()
            .and_then(|s| s.as_string())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_16_apps_have_distinct_profiles_and_valid_methods() {
        assert_eq!(ALL_APPS.len(), 16);
        let expected = [
            "photocraft",
            "vectorcraft",
            "lightcraft",
            "pdfcraft",
            "effectcraft",
            "designcraft",
            "figcraft",
            "filmcraft",
            "texturecraft",
            "materialcraft",
            "wordcraft",
            "soundcraft",
            "deckcraft",
            "cadcraft",
            "gridcraft",
            "craft-anything",
        ];
        for slug in expected {
            let profile = AppProfile::for_app(slug);
            assert!(profile.is_some(), "missing profile for {slug}");
            let p = profile.unwrap_or(&ALL_APPS[0]);
            assert!(p.methods.commands.is_some(), "{slug} commands method");
            assert!(p.methods.execute.is_some(), "{slug} execute method");
            assert!(p.methods.screenshot.is_some(), "{slug} screenshot method");
        }
    }

    #[test]
    fn execute_payload_adapts_between_engine_execute_and_command_run() {
        let std_profile = AppProfile::resolve("photocraft");
        let (m1, p1) = std_profile
            .methods
            .format_execute_params("layer.add", json!({"name": "BG"}));
        assert_eq!(m1, "engine.execute");
        assert_eq!(p1, json!({"command": "layer.add", "params": {"name": "BG"}}));

        let tc_profile = AppProfile::resolve("texturecraft");
        let (m2, p2) = tc_profile
            .methods
            .format_execute_params("layer.addFill", json!({"roughness": 0.4}));
        assert_eq!(m2, "command.run");
        assert_eq!(p2, json!({"id": "layer.addFill", "params": {"roughness": 0.4}}));

        let pdf_profile = AppProfile::resolve("pdfcraft");
        let (m3, p3) = pdf_profile
            .methods
            .format_execute_params("file.save", json!({}));
        assert_eq!(m3, "ui.command");
        assert_eq!(p3, json!({"id": "file.save", "params": {}}));
    }

    #[test]
    fn host_mode_detects_all_four_embedding_conventions() {
        assert_eq!(HostMode::detect("?host=parent", None), Some(HostMode::Parent));
        assert_eq!(HostMode::detect("?webgl&embed", None), Some(HostMode::Embed));
        assert_eq!(HostMode::detect("?embed=1", None), Some(HostMode::Embed));
        assert_eq!(
            HostMode::detect("?host=https://studio.example.com:8443/", None),
            Some(HostMode::ExplicitOrigin("https://studio.example.com:8443".to_string()))
        );
        assert_eq!(HostMode::detect("", Some("page")), Some(HostMode::Page));
        assert_eq!(HostMode::detect("?host=*", None), None);
        assert_eq!(HostMode::detect("?host=javascript:alert(1)", None), None);
    }

    #[test]
    fn revision_gate_holds_dirty_until_host_confirms_matching_revision() {
        let mut gate = RevisionGate::new(true);
        assert!(!gate.is_dirty());
        let r1 = gate.record_edit();
        assert_eq!(r1, 1);
        assert!(gate.is_dirty());

        let sent = gate.begin_save(Some("s1".to_string()));
        assert_eq!(sent, 1);
        // Still dirty until host confirms!
        assert!(gate.is_dirty());

        // User edits again while save s1 is uploading:
        let r2 = gate.record_edit();
        assert_eq!(r2, 2);

        // Host confirms revision 1: document must stay dirty because revision 2 is newer!
        assert!(!gate.confirm_saved(Some(1), None));
        assert!(gate.is_dirty());

        // Host confirms revision 2: now clean!
        assert!(gate.confirm_saved(Some(2), None));
        assert!(!gate.is_dirty());
    }

    #[test]
    fn incoming_messages_parse_across_all_dialects() {
        assert_eq!(
            IncomingKind::parse("photocraft", "photocraft:open", false),
            Some(IncomingKind::Open)
        );
        assert_eq!(
            IncomingKind::parse("photocraft", "photocraft:request", false),
            Some(IncomingKind::Request)
        );
        assert_eq!(
            IncomingKind::parse("photocraft", "photocraft:execute", false),
            Some(IncomingKind::Execute)
        );
        assert_eq!(
            IncomingKind::parse("photocraft", "photocraft:snapshot-request", false),
            Some(IncomingKind::SnapshotRequest)
        );
        assert_eq!(
            IncomingKind::parse("photocraft", "open", true),
            Some(IncomingKind::Open)
        );
        assert_eq!(
            IncomingKind::parse("photocraft", "open", false),
            None,
            "unprefixed message without bridge envelope must be ignored"
        );
        assert_eq!(
            IncomingKind::parse("photocraft", "vectorcraft:open", false),
            None,
            "another app's message must be ignored"
        );
    }

    #[test]
    fn cursor_compositing_marks_rgba_framebuffer_and_base64_roundtrips() {
        let mut cur = CursorState::default();
        cur.update_from_json(&json!({"x": 0.5, "y": 0.5, "normalized": true, "color": "#ff0000"}));
        assert!(cur.visible);
        assert_eq!(cur.pixel_coords(100, 100), (50, 50));

        let mut rgba = vec![0u8; 32 * 32 * 4];
        cur.composite_rgba(&mut rgba, 32, 32);
        let center_idx = (16 * 32 + 16) * 4;
        assert_eq!(&rgba[center_idx..center_idx + 4], &[0xff, 0x00, 0x00, 0xff]);

        let sample: &[u8] = &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', b'h', b'e', b'l', b'l', b'o'];
        let encoded = base64_encode(sample);
        let decoded = base64_decode(&encoded).unwrap_or_default();
        assert_eq!(decoded, sample);
    }

    #[test]
    fn import_message_and_pointer_scaling_and_table_parsing_work() {
        assert_eq!(
            IncomingKind::parse("gridcraft", "gridcraft:import", false),
            Some(IncomingKind::Import)
        );
        assert_eq!(
            IncomingKind::parse("photocraft", "photocraft:insert", false),
            Some(IncomingKind::Import)
        );
        assert_eq!(
            IncomingKind::parse("gridcraft", "gridcraft:ready-request", false),
            Some(IncomingKind::Hello)
        );
        assert_eq!(infer_mime_from_name("slip.csv"), "text/csv");
        assert_eq!(infer_mime_from_name("hero.png"), "image/png");

        let rows = parse_tabular_text_rows("Market,Side,Stake\nBTC > 100k,YES,$50");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["Market", "Side", "Stake"]);
        assert_eq!(rows[1], vec!["BTC > 100k", "YES", "$50"]);

        let ptr_in = json!({
            "x": 0.5,
            "y": 0.25,
            "normalized": true,
            "events": [
                { "type": "move", "x": 0.5, "y": 0.25 },
                { "type": "down", "x": 0.5, "y": 0.25, "button": 0 },
                { "type": "up", "x": 0.5, "y": 0.25, "button": 0 }
            ]
        });
        let (scaled, dom_evts) = scale_pointer_payload_for_canvas(&ptr_in, 10.0, 20.0, 1000.0, 800.0);
        assert_eq!(scaled["x"], json!(500.0));
        assert_eq!(scaled["y"], json!(200.0));
        assert_eq!(scaled["clientX"], json!(510.0));
        assert_eq!(scaled["clientY"], json!(220.0));
        assert_eq!(dom_evts.len(), 4); // move, down, up, click
        assert_eq!(dom_evts[3].0, "click");
    }
}
