#!/usr/bin/env bash
# Apply the Universal WebAssembly Bridge insert and patch series to any of the 16 ArtCraft app checkouts.
#
# Usage: scripts/apply-universal-bridge.sh <app-id> [app-checkout-dir]
set -euo pipefail

APP_ID="${1:?usage: apply-universal-bridge.sh <app-id> [app-checkout-dir]}"
APP_ROOT="${2:-$PWD}"
OVERLAY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE_SRC="$OVERLAY_ROOT/crates/craft-universal-bridge/src/universal_bridge.rs"
WEB_DIR="$APP_ROOT/apps/${APP_ID}-web"

[[ -f "$BRIDGE_SRC" ]] || { echo "error: missing $BRIDGE_SRC" >&2; exit 1; }
[[ -d "$WEB_DIR/src" ]] || { echo "error: missing $WEB_DIR/src" >&2; exit 1; }

cp "$BRIDGE_SRC" "$WEB_DIR/src/universal_bridge.rs"
echo "Inserted $WEB_DIR/src/universal_bridge.rs"

python3 "$OVERLAY_ROOT/scripts/wire_universal_bridge.py" "$APP_ID" "$APP_ROOT" "$WEB_DIR"
