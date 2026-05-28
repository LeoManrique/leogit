#!/usr/bin/env bash
set -euo pipefail

# Builds leogit.app from the Tauri project.
# Output: tauri-app/src-tauri/target/release/bundle/macos/leogit.app
# Usage:  scripts/bundle.sh
#   Version is read from tauri-app/src-tauri/tauri.conf.json and baked into
#   the bundle at build time. To release a new version, use
#   deploy_releases.sh, which bumps the version (and commits it) before
#   calling this script.

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; NC='\033[0m'
success() { echo -e "  ${GREEN}✓ $1${NC}"; }
warn()    { echo -e "  ${YELLOW}⚠ $1${NC}"; }
error()   { echo -e "  ${RED}✗ $1${NC}"; exit 1; }
info()    { echo -e "${CYAN}$1${NC}"; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
APP_DIR="$PROJECT_ROOT/tauri-app"
TAURI_CONF="$APP_DIR/src-tauri/tauri.conf.json"
cd "$APP_DIR"

command -v pnpm >/dev/null 2>&1 || error "pnpm is not installed"

# Pull the version from tauri.conf.json so the bundle name in logs matches
# what Tauri actually bakes into Info.plist.
VERSION=$(grep -o '"version"[[:space:]]*:[[:space:]]*"[^"]*"' "$TAURI_CONF" | head -1 | sed 's/.*"\([^"]*\)"$/\1/')
[ -z "$VERSION" ] && error "Could not read version from tauri.conf.json"
info "Bundling LeoGit $VERSION"

echo "==> Installing dependencies"
pnpm install --frozen-lockfile >/dev/null
success "Dependencies installed"

# beforeBuildCommand runs the Vite frontend build; tauri then bundles just the
# .app. We pass --bundles app to skip the .dmg — the release pipeline ships a
# zipped .app, so the disk image would only be wasted build time.
echo "==> Building Release configuration (this takes a while)"
pnpm tauri build --bundles app >/dev/null
success "Built Release configuration"

APP_PATH="$APP_DIR/src-tauri/target/release/bundle/macos/leogit.app"
[ -d "$APP_PATH" ] || error "tauri build did not produce $APP_PATH"

# Ad-hoc sign. No Developer ID — users bypass Gatekeeper via xattr -cr
# (install.sh does this automatically) or right-click → Open on first launch.
# --force re-signs over Tauri's default signature.
if codesign --force --deep --sign - "$APP_PATH" 2>/dev/null; then
  success "Ad-hoc signed"
else
  warn "codesign failed (continuing — bundle still usable with xattr -cr)"
fi

codesign --verify --verbose=2 "$APP_PATH" >/dev/null 2>&1 || warn "codesign --verify failed"

success "Built $APP_PATH"
