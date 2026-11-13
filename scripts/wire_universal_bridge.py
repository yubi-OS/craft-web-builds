#!/usr/bin/env python3
import os
import re
import sys
import tomllib

app_id, app_root, web_dir = sys.argv[1], sys.argv[2], sys.argv[3]
cargo_path = os.path.join(web_dir, "Cargo.toml")
root_cargo_path = os.path.join(app_root, "Cargo.toml")

with open(cargo_path, "r", encoding="utf-8") as f:
    text = f.read()
with open(root_cargo_path, "r", encoding="utf-8") as f:
    root_text = f.read()

root_toml = tomllib.loads(root_text)
ws_deps = root_toml.get("workspace", {}).get("dependencies", {})

def spec_for(crate: str, fallback: str) -> str:
    if crate in ws_deps:
        if crate == "serde":
            return '{ workspace = true, features = ["derive"] }'
        return "{ workspace = true }"
    return fallback

def insert_into_section(toml_text: str, section_header: str, crate: str, spec: str) -> str:
    lines = toml_text.splitlines(True)
    header_line = f"[{section_header}]"
    for i, line in enumerate(lines):
        if line.strip() == header_line:
            lines.insert(i + 1, f"{crate} = {spec}\n")
            return "".join(lines)
    return toml_text.rstrip() + f"\n\n[{section_header}]\n{crate} = {spec}\n"

def has_runtime_dep(parsed: dict, crate: str) -> bool:
    if crate in parsed.get("dependencies", {}):
        return True
    for t_cfg, t_tbl in parsed.get("target", {}).items():
        if "wasm32" in t_cfg and crate in t_tbl.get("dependencies", {}):
            return True
    return False

parsed = tomllib.loads(text)
target_sec = 'target.\'cfg(target_arch = "wasm32")\'.dependencies'
base_sec = target_sec if app_id in ("filmcraft", "soundcraft") else "dependencies"

for crate, fallback, sec in [
    ("serde", '{ version = "1", features = ["derive"] }', base_sec),
    ("serde_json", '"1"', base_sec),
    ("wasm-bindgen", '"0.2"', target_sec),
    ("wasm-bindgen-futures", '"0.4"', target_sec),
    ("js-sys", '"0.3"', target_sec),
]:
    if not has_runtime_dep(parsed, crate):
        text = insert_into_section(text, sec, crate, spec_for(crate, fallback))
        parsed = tomllib.loads(text)
    elif crate == "serde":
        serde_entry = parsed.get("dependencies", {}).get("serde") or parsed.get("target", {}).get('cfg(target_arch = "wasm32")', {}).get("dependencies", {}).get("serde")
        if isinstance(serde_entry, dict) and "features" not in serde_entry:
            text = re.sub(
                r"^serde\s*=\s*\{\s*workspace\s*=\s*true\s*\}",
                'serde = { workspace = true, features = ["derive"] }',
                text,
                count=1,
                flags=re.M,
            )
            parsed = tomllib.loads(text)

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
        # Always insert AFTER all inner attributes (#![...]) and leading comments (//! or //)
        for idx, line in enumerate(lines):
            stripped = line.strip()
            if stripped.startswith("#![") or stripped.startswith("//") or stripped == "":
                insert_idx = idx + 1
            else:
                break
        lines.insert(insert_idx, '#[cfg(any(target_arch = "wasm32", test))]\npub mod universal_bridge;\n')
        entry_src = "".join(lines)

if app_id == "pdfcraft":
    entry_src = entry_src.replace(
        'if query_param("control").is_some() {\n                        api::install(app.attach_control(&cc.egui_ctx));\n                    }',
        """api::install(app.attach_control(&cc.egui_ctx));
                    let ubridge = crate::universal_bridge::web::UniversalBridge::install_auto("pdfcraft");
                    ubridge.set_waker({ let c = cc.egui_ctx.clone(); move || c.request_repaint() });
                    if ubridge.mode.is_some() {
                        let _ = ubridge.post_ready(env!("CARGO_PKG_VERSION"), None);
                    }""",
    )

with open(entry_rs, "w", encoding="utf-8") as f:
    f.write(entry_src)

web_rs = os.path.join(web_dir, "src", "web.rs")
app_rs = os.path.join(web_dir, "src", "app.rs")

if app_id == "photocraft" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "let mut app = PhotocraftApp::new(session, services(inbox.clone(), place_inbox.clone(), drag_pos_service(pen_target.clone(), drag)));"
    new = """let mut svc = services(inbox.clone(), place_inbox.clone(), drag_pos_service(pen_target.clone(), drag));
                    let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel();
                    let ctrl_ctx = cc.egui_ctx.clone();
                    let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("photocraft", move |m, p| {
                        let (req, rx) = photocraft_ui_egui::ControlRequest::new(m, p);
                        let _ = ctrl_tx.send(req);
                        ctrl_ctx.request_repaint();
                        rx
                    });
                    ubridge.set_waker({ let c = cc.egui_ctx.clone(); move || c.request_repaint() });
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

elif app_id == "vectorcraft" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "VectorcraftApp::new(Session::new(), services).with_control(control_rx)"
    new = """let ubridge = crate::universal_bridge::web::UniversalBridge::install_auto("vectorcraft");
                        ubridge.set_waker({ let c = ctx.clone(); move || c.request_repaint() });
                        let open_inbox = __VC_OPEN_INBOX__;
                        let open_ctx = ctx.clone();
                        ubridge.on_open(move |name, bytes, _fonts| {
                            open_inbox.lock().unwrap_or_else(|e| e.into_inner()).push((name, bytes));
                            open_ctx.request_repaint();
                        });
                        if ubridge.mode.is_some() {
                            let _ = ubridge.post_ready(env!("CARGO_PKG_VERSION"), None);
                        }
                        VectorcraftApp::new(Session::new(), services).with_control(control_rx)"""
    # Upstream storytold/vectorcraft (>= 4306a1d) split `Host::inbox` into `OpenInboxes { dropped, picked, page }`;
    # bridge `open` requests are page-script opens, so route them to `inbox.page` (the `window.vectorcraft.openFile` inbox).
    new = new.replace("__VC_OPEN_INBOX__", "host.inbox.page.clone()" if "struct OpenInboxes" in w else "host.inbox.clone()")
    if old in w and 'install_auto("vectorcraft")' not in w:
        w = w.replace(old, new, 1)
        with open(web_rs, "w", encoding="utf-8") as f:
            f.write(w)

elif app_id == "lightcraft" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "let mut app = LightcraftApp::new(session, services).with_chinese_font(chinese_font);"
    new = """let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel();
        let ctrl_ctx = cc.egui_ctx.clone();
        let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("lightcraft", move |m, p| {
            let (req, rx) = lightcraft_ui_egui::ControlRequest::new(m, p);
            let _ = ctrl_tx.send(req);
            ctrl_ctx.request_repaint();
            rx
        });
        ubridge.set_waker({ let c = cc.egui_ctx.clone(); move || c.request_repaint() });
        if ubridge.mode.is_some() {
            let _ = ubridge.post_ready(env!("CARGO_PKG_VERSION"), None);
        }
        let mut app = LightcraftApp::new(session, services).with_chinese_font(chinese_font).with_control(ctrl_rx);"""
    if old in w and 'install_with_channel("lightcraft"' not in w:
        w = w.replace(old, new, 1)
        with open(web_rs, "w", encoding="utf-8") as f:
            f.write(w)

elif app_id == "effectcraft" and os.path.exists(app_rs):
    with open(app_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old_opts = "eframe::WebOptions::default(),"
    new_opts = """{
                let mut opts = eframe::WebOptions::default();
                if query_flag("webgl")
                    && let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut opts.wgpu_options.wgpu_setup
                {
                    create.instance_descriptor.backends = eframe::wgpu::Backends::GL;
                }
                opts
            },"""
    if old_opts in w:
        w = w.replace(old_opts, new_opts, 1)
    old_workers = 'let n = if query_flag("noworkers") { 0 } else { query_value("frameworkers").and_then(|v| v.parse().ok()).unwrap_or(2usize).min(8) };'
    new_workers = 'let n = if query_flag("noworkers") || query_flag("webgl") { 0 } else { query_value("frameworkers").and_then(|v| v.parse().ok()).unwrap_or(2usize).min(8) };'
    if old_workers in w:
        w = w.replace(old_workers, new_workers, 1)
    old = "api::set_sender(tx);\n                app = app.with_control(rx);"
    new = """api::set_sender(tx);
                app = app.with_control(rx);
                let ubridge = crate::universal_bridge::web::UniversalBridge::install_auto("effectcraft");
                ubridge.set_waker(|| crate::repaint());
                if ubridge.mode.is_some() {
                    let _ = ubridge.post_ready(env!("CARGO_PKG_VERSION"), None);
                }"""
    if old in w and 'install_auto("effectcraft")' not in w:
        w = w.replace(old, new, 1)
    with open(app_rs, "w", encoding="utf-8") as f:
        f.write(w)

elif app_id == "designcraft" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "let mut app = DesignApp::new(Session::new(), services(inbox.clone(), cc.egui_ctx.clone()));"
    new = """let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel();
                    let ctrl_ctx = cc.egui_ctx.clone();
                    let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("designcraft", move |m, p| {
                        let (req, rx) = designcraft_ui_egui::ControlRequest::new(m, p);
                        let _ = ctrl_tx.send(req);
                        ctrl_ctx.request_repaint();
                        rx
                    });
                    ubridge.set_waker({ let c = cc.egui_ctx.clone(); move || c.request_repaint() });
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

elif app_id == "figcraft" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = """                    if crate::query::control(&query()) {
                        crate::api::connect(cc.egui_ctx.clone());
                        if let Err(e) = crate::api::install() {
                            log::error!("couldn't install window.figcraft: {e:?}");
                        }
                    }"""
    new = """                    crate::api::connect(cc.egui_ctx.clone());
                    if let Err(e) = crate::api::install() {
                        log::error!("couldn't install window.figcraft: {e:?}");
                    }
                    let ubridge = crate::universal_bridge::web::UniversalBridge::install_auto("figcraft");
                    ubridge.set_waker({ let c = cc.egui_ctx.clone(); move || c.request_repaint() });"""
    if old in w and 'install_auto("figcraft")' not in w:
        w = w.replace(old, new, 1)
        with open(web_rs, "w", encoding="utf-8") as f:
            f.write(w)

elif app_id == "filmcraft":
    with open(entry_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "api::set_sender(tx);\n                app = app.with_control(rx);"
    new = """api::set_sender(tx);
                app = app.with_control(rx);
                let ubridge = crate::universal_bridge::web::UniversalBridge::install_auto("filmcraft");
                ubridge.set_waker({ let c = cc.egui_ctx.clone(); move || c.request_repaint() });
                if ubridge.mode.is_some() {
                    let _ = ubridge.post_ready(env!("CARGO_PKG_VERSION"), None);
                }"""
    if old in w and 'install_auto("filmcraft")' not in w:
        w = w.replace(old, new, 1)
        with open(entry_rs, "w", encoding="utf-8") as f:
            f.write(w)

elif app_id == "texturecraft" and os.path.exists(web_rs):
    with open(web_rs, "r", encoding="utf-8") as f:
        w = f.read()
    old = "let mut app = TextureCraftApp::new(cc, session);"
    new = """let mut app = TextureCraftApp::new(cc, session);
                let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel();
                app.control = Some(ctrl_rx);
                let ctrl_ctx = cc.egui_ctx.clone();
                let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("texturecraft", move |m, p| {
                    let (req, rx) = texturecraft_ui_egui::ControlRequest::new(m, p);
                    let _ = ctrl_tx.send(req);
                    ctrl_ctx.request_repaint();
                    rx
                });
                ubridge.set_waker({ let c = cc.egui_ctx.clone(); move || c.request_repaint() });
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
                let ctrl_ctx = cc.egui_ctx.clone();
                let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("materialcraft", move |m, p| {
                    let (req, rx) = materialcraft_ui_egui::ControlRequest::new(m, p);
                    let _ = ctrl_tx.send(req);
                    ctrl_ctx.request_repaint();
                    rx
                });
                ubridge.set_waker({ let c = cc.egui_ctx.clone(); move || c.request_repaint() });
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
                    let ctrl_ctx = cc.egui_ctx.clone();
                    let ubridge = crate::universal_bridge::web::UniversalBridge::install_with_channel("soundcraft", move |m, p| {
                        let (req, rx) = soundcraft_ui_egui::ControlRequest::new(m, p);
                        let _ = ctrl_tx.send(req);
                        ctrl_ctx.request_repaint();
                        rx
                    });
                    ubridge.set_waker({ let c = cc.egui_ctx.clone(); move || c.request_repaint() });
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
                    ubridge.set_waker({ let c = cc.egui_ctx.clone(); move || c.request_repaint() });
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
    hook = f"""let ubridge = crate::universal_bridge::web::UniversalBridge::install_auto("{app_id}");
                    ubridge.set_waker({{ let c = cc.egui_ctx.clone(); move || c.request_repaint() }});"""
    if f'install_auto("{app_id}")' not in w:
        for anchor in [
            "crate::api::install(",
            "api::install(",
        ]:
            if anchor in w:
                idx = w.find(anchor)
                line_end = w.find("\n", idx)
                if line_end != -1:
                    w = w[: line_end + 1] + f"                    {hook}\n" + w[line_end + 1 :]
                    break
        with open(target_file, "w", encoding="utf-8") as f:
            f.write(w)

# Ensure packaging/web/package.sh allows >=32 MiB .wasm when universal bridge symbols are linked
pkg_sh = os.path.join(app_root, "packaging", "web", "package.sh")
if os.path.exists(pkg_sh):
    with open(pkg_sh, "r", encoding="utf-8") as f:
        ps = f.read()
    if "25165824" in ps:
        ps = ps.replace("25165824", "33554432")
        with open(pkg_sh, "w", encoding="utf-8") as f:
            f.write(ps)

print(f"Wired universal_bridge into {app_id}-web")
