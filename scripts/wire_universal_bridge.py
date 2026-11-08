#!/usr/bin/env python3
import os
import re
import sys

app_id, app_root, web_dir = sys.argv[1], sys.argv[2], sys.argv[3]
cargo_path = os.path.join(web_dir, "Cargo.toml")
root_cargo_path = os.path.join(app_root, "Cargo.toml")

with open(cargo_path, "r", encoding="utf-8") as f:
    text = f.read()
with open(root_cargo_path, "r", encoding="utf-8") as f:
    root_text = f.read()

has_ws_serde = bool(re.search(r"^serde\s*=", root_text, re.M))
has_ws_serde_json = bool(re.search(r"^serde_json\s*=", root_text, re.M))

def ensure_dep(toml_text: str, section: str, crate: str, spec: str) -> str:
    if re.search(rf"^{re.escape(crate)}\s*=", toml_text, re.M):
        return toml_text
    header = f"[{section}]"
    if header in toml_text:
        return toml_text.replace(header, f"{header}\n{crate} = {spec}", 1)
    return toml_text + f"\n{header}\n{crate} = {spec}\n"

serde_spec = '{ workspace = true }' if has_ws_serde else '{ version = "1", features = ["derive"] }'
serde_json_spec = '{ workspace = true }' if has_ws_serde_json else '"1"'
target_sec = 'target.\'cfg(target_arch = "wasm32")\'.dependencies'

if app_id in ("filmcraft", "soundcraft"):
    text = ensure_dep(text, target_sec, "serde", serde_spec)
    text = ensure_dep(text, target_sec, "serde_json", serde_json_spec)
    text = ensure_dep(text, target_sec, "js-sys", '"0.3"')
else:
    text = ensure_dep(text, "dependencies", "serde", serde_spec)
    text = ensure_dep(text, "dependencies", "serde_json", serde_json_spec)
    text = ensure_dep(text, target_sec, "js-sys", '"0.3"')

with open(cargo_path, "w", encoding="utf-8") as f:
    f.write(text)

entry_rs = os.path.join(
    web_dir,
    "src",
    "lib.rs" if app_id in ("lightcraft", "effectcraft", "filmcraft") else "main.rs",
)
with open(entry_rs, "r", encoding="utf-8") as f:
    entry_src = f.read()

if "mod universal_bridge;" not in entry_src:
    if app_id == "filmcraft":
        entry_src = entry_src.replace("pub mod api;", "pub mod universal_bridge;\npub mod api;", 1)
    else:
        lines = entry_src.splitlines(True)
        insert_idx = 0
        for idx, line in enumerate(lines):
            if line.startswith("#![") or line.startswith("//!") or line.strip() == "":
                insert_idx = idx + 1
            else:
                break
        lines.insert(insert_idx, '#[cfg(any(target_arch = "wasm32", test))]\npub mod universal_bridge;\n')
        entry_src = "".join(lines)

if app_id == "pdfcraft":
    entry_src = entry_src.replace(
        'if query_param("control").is_some() {\n                        api::install(app.attach_control(&cc.egui_ctx));\n                    }',
        'api::install(app.attach_control(&cc.egui_ctx));\n                    let _ = crate::universal_bridge::web::UniversalBridge::install_auto("pdfcraft");',
    )

with open(entry_rs, "w", encoding="utf-8") as f:
    f.write(entry_src)

web_rs = os.path.join(web_dir, "src", "web.rs")
if app_id == "photocraft" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "let mut app = PhotocraftApp::new(session, services(inbox.clone(), place_inbox.clone(), drag_pos_service(pen_target.clone(), drag)));"
    new = """let mut svc = services(inbox.clone(), place_inbox.clone(), drag_pos_service(pen_target.clone(), drag));
                    let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel();
                    let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("photocraft", move |m, p| {
                        let (req, rx) = photocraft_ui_egui::ControlRequest::new(m, p);
                        let _ = ctrl_tx.send(req);
                        rx
                    });
                    let open_inbox = inbox.clone();
                    let open_ctx = cc.egui_ctx.clone();
                    ubridge.on_open(move |name, bytes, _fonts| {
                        open_inbox.lock().unwrap_or_else(|e| e.into_inner()).push((name, bytes));
                        open_ctx.request_repaint();
                    });
                    if ubridge.mode.is_some() {
                        let save_bridge = ubridge.clone();
                        svc.write = Some(Box::new(move |path: &str, bytes: &[u8]| {
                            save_bridge.post_save(path, bytes, false, None).map(|_| ())
                        }));
                        svc.write_unconfirmed = false;
                        let _ = ubridge.post_ready(env!("CARGO_PKG_VERSION"), None);
                    }
                    let mut app = PhotocraftApp::new(session, svc).with_control(ctrl_rx);"""
    if old in w and 'install_with_channel("photocraft"' not in w:
        w = w.replace(old, new, 1)
        with open(web_rs, "w", encoding="utf-8") as f:
            f.write(w)

elif app_id == "designcraft" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "let mut app = DesignApp::new(Session::new(), services(inbox.clone(), cc.egui_ctx.clone()));"
    new = """let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel();
                    let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("designcraft", move |m, p| {
                        let (req, rx) = designcraft_ui_egui::ControlRequest::new(m, p);
                        let _ = ctrl_tx.send(req);
                        rx
                    });
                    let mut svc = services(inbox.clone(), cc.egui_ctx.clone());
                    if ubridge.mode.is_some() {
                        let sb1 = ubridge.clone();
                        let sb2 = ubridge.clone();
                        svc.write = Some(Box::new(move |name: &str, bytes: &[u8]| sb1.post_save(name, bytes, false, None).map(|_| ())));
                        svc.download = Some(Box::new(move |name: &str, bytes: &[u8]| sb2.post_save(name, bytes, false, None).map(|_| ())));
                        let _ = ubridge.post_ready(env!("CARGO_PKG_VERSION"), None);
                    }
                    let mut app = DesignApp::new(Session::new(), svc).with_control(ctrl_rx);"""
    if old in w and 'install_with_channel("designcraft"' not in w:
        w = w.replace(old, new, 1)
        with open(web_rs, "w", encoding="utf-8") as f:
            f.write(w)

elif app_id == "texturecraft" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "let mut app = TextureCraftApp::new(cc, session);"
    new = """let mut app = TextureCraftApp::new(cc, session);
                let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel();
                app.control = Some(ctrl_rx);
                let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("texturecraft", move |m, p| {
                    let (req, rx) = texturecraft_ui_egui::ControlRequest::new(m, p);
                    let _ = ctrl_tx.send(req);
                    rx
                });
                if ubridge.mode.is_some() {
                    let _ = ubridge.post_ready(env!("CARGO_PKG_VERSION"), None);
                }"""
    if old in w and 'install_with_channel("texturecraft"' not in w:
        w = w.replace(old, new, 1)
        with open(web_rs, "w", encoding="utf-8") as f:
            f.write(w)

elif app_id == "materialcraft" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "let mut app = MaterialCraftApp::new(cc, session);"
    new = """let mut app = MaterialCraftApp::new(cc, session);
                let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel();
                app.control = Some(ctrl_rx);
                let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("materialcraft", move |m, p| {
                    let (req, rx) = materialcraft_ui_egui::ControlRequest::new(m, p);
                    let _ = ctrl_tx.send(req);
                    rx
                });
                if ubridge.mode.is_some() {
                    let _ = ubridge.post_ready(env!("CARGO_PKG_VERSION"), None);
                }"""
    if old in w and 'install_with_channel("materialcraft"' not in w:
        w = w.replace(old, new, 1)
        with open(web_rs, "w", encoding="utf-8") as f:
            f.write(w)

elif app_id == "soundcraft" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "let app = SoundApp::new(engine, Some(player), Services::default());"
    new = """let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel();
                    let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("soundcraft", move |m, p| {
                        let (req, rx) = soundcraft_ui_egui::ControlRequest::new(m, p);
                        let _ = ctrl_tx.send(req);
                        rx
                    });
                    if ubridge.mode.is_some() {
                        let _ = ubridge.post_ready(env!("CARGO_PKG_VERSION"), None);
                    }
                    let app = SoundApp::new(engine, Some(player), Services::default()).with_control(ctrl_rx);"""
    if old in w and 'install_with_channel("soundcraft"' not in w:
        w = w.replace(old, new, 1)
        with open(web_rs, "w", encoding="utf-8") as f:
            f.write(w)

elif app_id == "craft-anything" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "let inbox = Inbox::default();"
    new = """let inbox = Inbox::default();
                    let open_inbox = inbox.clone();
                    let open_ctx = cc.egui_ctx.clone();
                    let triage_inbox = inbox.clone();
                    let triage_ctx = cc.egui_ctx.clone();
                    let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("craft-anything", move |method, params| {
                        let (tx, rx) = std::sync::mpsc::channel();
                        let reply = match method.as_str() {
                            "engine.commands" => serde_json::json!({
                                "ok": true,
                                "result": [
                                    {"id": "file.triage", "label": "Triage File Bytes", "params": [{"name": "name", "type": "string", "required": true}, {"name": "data", "type": "string", "required": true}]},
                                    {"id": "file.open", "label": "Open File Bytes", "params": [{"name": "name", "type": "string", "required": true}, {"name": "data", "type": "string", "required": true}]}
                                ]
                            }),
                            "engine.execute" => {
                                let cmd = params.get("command").and_then(|v| v.as_str()).unwrap_or("");
                                let p = params.get("params").cloned().unwrap_or_else(|| serde_json::json!({}));
                                let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("sample.bin").to_string();
                                let b64 = p.get("data").and_then(|v| v.as_str()).unwrap_or("");
                                let bytes = crate::universal_bridge::base64_decode(b64).unwrap_or_default();
                                let look = crate::triage::look(&name, &bytes);
                                if cmd == "file.open" || cmd == "file.triage" {
                                    if let Ok(mut slot) = triage_inbox.lock() {
                                        *slot = Some(Ok((name.clone(), bytes)));
                                    }
                                    triage_ctx.request_repaint();
                                }
                                serde_json::json!({
                                    "ok": true,
                                    "result": {
                                        "fileName": look.file_name,
                                        "summary": look.summary,
                                        "facts": look.facts,
                                        "limitations": look.limitations,
                                        "details": look.details
                                    }
                                })
                            }
                            "document.inspect" | "ui.inspect" => serde_json::json!({
                                "ok": true,
                                "result": {"app": "craft-anything", "ready": true}
                            }),
                            _ => serde_json::json!({"ok": false, "error": format!("unsupported method: {method}")}),
                        };
                        let _ = tx.send(reply);
                        rx
                    });
                    ubridge.on_open(move |name, bytes, _fonts| {
                        if let Ok(mut slot) = open_inbox.lock() {
                            *slot = Some(Ok((name, bytes)));
                        }
                        open_ctx.request_repaint();
                    });
                    if ubridge.mode.is_some() {
                        let _ = ubridge.post_ready(env!("CARGO_PKG_VERSION"), None);
                    }"""
    if old in w and 'install_with_channel("craft-anything"' not in w:
        w = w.replace(old, new, 1)
        with open(web_rs, "w", encoding="utf-8") as f:
            f.write(w)

else:
    target_file = web_rs if os.path.exists(web_rs) else entry_rs
    with open(target_file, "r", encoding="utf-8") as f:
        w = f.read()
    hook = f'let _ = crate::universal_bridge::web::UniversalBridge::install_auto("{app_id}");'
    if hook not in w:
        for anchor in [
            "crate::api::install(",
            "api::install(",
            "let mut app = LightcraftApp::new(",
            "let mut app = EffectcraftApp::new(",
        ]:
            if anchor in w:
                idx = w.find(anchor)
                line_end = w.find("\n", idx)
                if line_end != -1:
                    w = w[: line_end + 1] + f"                    {hook}\n" + w[line_end + 1 :]
                    break
        with open(target_file, "w", encoding="utf-8") as f:
            f.write(w)

print(f"Wired universal_bridge into {app_id}-web")
