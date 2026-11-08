# `yubi-OS/craft-web-builds`

Aggregator repository housing the CI matrix workflow, unified library patch series, and universal Rust bridge insert (`craft-universal-bridge`) for building all 16 [ArtCraft](https://getartcraft.com/apps) WebAssembly applications into static-host-ready packages (Cloudflare Pages / Workers Static Assets compatible, `<= 24 MiB` per file).

## Repository Layout

- `.github/workflows/craft-web-builds.yml` — 16-app matrix build workflow + `verify-universal-bridge` native/wasm check + `combine` suite artifact (`wasm-suite-<sha>`).
- `crates/craft-universal-bridge/` — Standalone crate and single-file universal Rust insert (`src/universal_bridge.rs`) rebasing and widening `storytold/photocraft#1614`, `#9593`, `#3101`, `#6450`, and `storytold/craft-libs` (`craft-web-host` + `craft-web-api`).
- `patches/` — Unified patch series:
  - `patches/SERIES.md` — Patch series manifest and base commit table.
  - `patches/0001-craft-libs-universal-web-bridge.patch` — Shared library patch against `storytold/craft-libs`.
  - `patches/0002-universal-rust-insert-all-16-apps.patch` — Combined 16-app universal insert patch.
  - `patches/apps/<app>.patch` — Individual per-app patches for all 16 apps.
- `scripts/apply-universal-bridge.sh` — Deterministic CI patcher and universal Rust inserter.
- `docs/PROTOCOL.md` — Complete specification of the same-origin host-frame JSON control, file handoff, revision-acked save, shared cursor overlay, and frame capture protocol.
- `PINNED.md` — Pinned upstream commit SHAs and per-app font/i18n build settings.
