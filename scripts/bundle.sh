#!/usr/bin/env bash
set -euo pipefail

# Builds the LeoGit app bundle from the Tauri project for the host platform.
#   macOS   → target/release/bundle/macos/leogit.app
#   Linux   → target/release/bundle/appimage/*.AppImage
#   Windows → target/release/bundle/nsis/*-setup.exe
# Usage:  scripts/bundle.sh
#   Version is read from apps/tauri-app/src-tauri/tauri.conf.json and baked into
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
APP_DIR="$PROJECT_ROOT/apps/tauri-app"
TAURI_CONF="$APP_DIR/src-tauri/tauri.conf.json"
# The Cargo workspace relocates target/ to the repo root, so tauri drops bundle
# artifacts here rather than under the app's src-tauri.
TARGET_DIR="$PROJECT_ROOT/target"
cd "$APP_DIR"

# Host kernel drives the per-OS bundle target below. MSYS2/Git-Bash report
# MINGW*/MSYS*, Cygwin reports CYGWIN* — all mean Windows.
OS_KERNEL=$(uname -s)

command -v pnpm >/dev/null 2>&1 || error "pnpm is not installed"
command -v cargo >/dev/null 2>&1 \
  || error "cargo is not installed — install the Rust toolchain (Arch: sudo pacman -S rustup && rustup default stable)"

# Linux AppImage builds link against WebKitGTK and need patchelf. Catch missing
# system deps here with an actionable message instead of a cryptic build error.
if [ "$OS_KERNEL" = "Linux" ]; then
  command -v patchelf >/dev/null 2>&1 \
    || warn "patchelf not found — AppImage bundling may fail (Arch: sudo pacman -S patchelf)"
  if command -v pkg-config >/dev/null 2>&1 && ! pkg-config --exists webkit2gtk-4.1; then
    warn "webkit2gtk-4.1 not found — the build will fail (Arch: sudo pacman -S webkit2gtk-4.1)"
  fi
fi

# Pull the version from tauri.conf.json so the bundle name in logs matches
# what Tauri actually bakes into Info.plist.
VERSION=$(grep -o '"version"[[:space:]]*:[[:space:]]*"[^"]*"' "$TAURI_CONF" | head -1 | sed 's/.*"\([^"]*\)"$/\1/')
[ -z "$VERSION" ] && error "Could not read version from tauri.conf.json"
info "Bundling LeoGit $VERSION"

echo "==> Installing dependencies"
pnpm install --frozen-lockfile >/dev/null
success "Dependencies installed"

# beforeBuildCommand runs the Vite frontend build; tauri then bundles only the
# artifact the release pipeline ships, skipping the .dmg/.deb/.msi that would
# otherwise be wasted build time: macOS → app, Linux → appimage, Windows → nsis.
case "$OS_KERNEL" in
  Darwin)
    BUNDLE_TARGET=app
    ;;
  MINGW*|MSYS*|CYGWIN*)
    # NSIS: a per-user installer (no admin) with a WebView2 bootstrapper. Tauri
    # downloads the NSIS toolchain on first use; nothing else to install here.
    BUNDLE_TARGET=nsis
    # Tauri drops one <product>_<version>_<arch>-setup.exe into bundle/nsis per
    # build and never prunes old ones. Wipe the dir first so the only installer
    # left afterward is this build's — deploy_releases.sh globs it by version.
    rm -rf "$TARGET_DIR/release/bundle/nsis"
    ;;
  Linux)
    BUNDLE_TARGET=appimage

    # linuxdeploy (the AppImage tooling Tauri downloads) needs two workarounds on
    # current Linux. Set them here so release builds are reproducible:
    #   NO_STRIP — linuxdeploy bundles an old `strip` that aborts on the
    #     `.relr.dyn` section modern binutils emits ("unknown type [0x13]").
    #   APPIMAGE_EXTRACT_AND_RUN — run linuxdeploy's nested plugin AppImages by
    #     extracting rather than FUSE-mounting, which is flaky when nested.
    export NO_STRIP=true
    export APPIMAGE_EXTRACT_AND_RUN=1

    # The gtk plugin unconditionally copies gdk-pixbuf's loader dir (path from
    # pkg-config). On current Arch that dir is gone — gdk-pixbuf has its loaders
    # built in and librsvg dropped its pixbuf loader — so the copy aborts the
    # plugin. An empty dir satisfies it; LeoGit only renders PNG icons.
    PIXBUF_DIR=$(pkg-config --variable=gdk_pixbuf_binarydir gdk-pixbuf-2.0 2>/dev/null || true)
    if [ -n "$PIXBUF_DIR" ] && [ ! -d "$PIXBUF_DIR" ]; then
      error "gdk-pixbuf loader dir missing — linuxdeploy's gtk plugin needs it. Create it once (empty is fine):
    sudo mkdir -p $PIXBUF_DIR/loaders
    gdk-pixbuf-query-loaders | sudo tee $PIXBUF_DIR/loaders.cache >/dev/null"
    fi
    ;;
  *)
    error "Unsupported OS: $OS_KERNEL (LeoGit bundles on macOS, Linux, and Windows)"
    ;;
esac
echo "==> Building Release configuration (this takes a while)"
pnpm tauri build --bundles "$BUNDLE_TARGET" >/dev/null
success "Built Release configuration"

case "$OS_KERNEL" in
  Darwin)
    APP_PATH="$TARGET_DIR/release/bundle/macos/leogit.app"
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
    ;;
  MINGW*|MSYS*|CYGWIN*)
    # NSIS produces a self-contained, unsigned setup.exe. The bundle/nsis dir was
    # wiped before the build, so the single *-setup.exe here is this version's.
    SETUP=$(ls "$TARGET_DIR/release/bundle/nsis/"*-setup.exe 2>/dev/null | head -1 || true)
    [ -n "$SETUP" ] && [ -f "$SETUP" ] || error "tauri build did not produce an NSIS installer"
    success "Built $SETUP"
    ;;
  *)
    # Linux AppImages are self-contained and need no signing.
    APPIMAGE=$(ls "$TARGET_DIR/release/bundle/appimage/"*.AppImage 2>/dev/null | head -1 || true)
    [ -n "$APPIMAGE" ] && [ -f "$APPIMAGE" ] || error "tauri build did not produce an AppImage"
    success "Built $APPIMAGE"
    ;;
esac
