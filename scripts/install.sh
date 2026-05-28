#!/usr/bin/env bash
set -euo pipefail

# Installs LeoGit from the latest GitHub release into /Applications.
# Intended to be curlable:
#   curl -fsSL https://raw.githubusercontent.com/LeoManrique/leogit/main/scripts/install.sh | bash

# ── Colors ──
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; NC='\033[0m'

TOTAL_STEPS=5
step()    { echo -e "\n${BLUE}[$1/$TOTAL_STEPS]${NC} ${CYAN}$2${NC}"; }
success() { echo -e "  ${GREEN}✓ $1${NC}"; }
warn()    { echo -e "  ${YELLOW}⚠ $1${NC}"; }
error()   { echo -e "  ${RED}✗ $1${NC}"; exit 1; }

REPO="LeoManrique/leogit"
API_URL="https://api.github.com/repos/$REPO/releases/latest"
TMP_DIR="/tmp/leogit-install"
APP_NAME="leogit.app"
DEST="/Applications/$APP_NAME"

# ── Step 1: Detect platform ──
step 1 "Detecting platform"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
  arm64|aarch64) ARCH="arm64" ;;
  x86_64)        ARCH="amd64" ;;
  *) error "Unsupported architecture: $ARCH" ;;
esac
[ "$OS" = "darwin" ] || error "Unsupported OS: $OS (LeoGit is macOS-only)"
PLATFORM="macOS-$ARCH"
success "Platform: $PLATFORM"

# ── Step 2: Stop any running instance ──
# A running app holds an open file lock on its binary; replacing the bundle
# on disk while the old copy is running leaves you with a half-old/half-new
# install until the next launch. Kill it first.
step 2 "Stopping running instance"

if pgrep -x leogit >/dev/null 2>&1; then
  pkill -TERM -x leogit 2>/dev/null || true
  for _ in $(seq 1 16); do
    pgrep -x leogit >/dev/null 2>&1 || break
    sleep 0.5
  done
  if pgrep -x leogit >/dev/null 2>&1; then
    warn "Force-killing leogit (graceful stop timed out)"
    pkill -KILL -x leogit 2>/dev/null || true
  fi
  success "Stopped running instance"
else
  success "No running instances"
fi

# ── Step 3: Fetch latest release metadata ──
step 3 "Fetching latest release from GitHub"

RELEASE_JSON=$(curl -fsSL -H "Accept: application/vnd.github.v3+json" "$API_URL" 2>/dev/null) \
  || error "Failed to fetch release info from GitHub. Check your internet connection."

TAG=$(echo "$RELEASE_JSON" | { grep -o '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"' || true; } | head -1 | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')
[ -z "$TAG" ] && error "Could not parse release tag from GitHub API"

VERSION="${TAG#v}"
success "Latest version: $VERSION (tag: $TAG)"

# ── Step 4: Download artifact ──
step 4 "Downloading $APP_NAME"

ARTIFACT="LeoGit-$VERSION-$PLATFORM.zip"
# `|| true` keeps a no-match from tripping `set -o pipefail` and aborting the
# script silently before the explicit "not found" check below can run.
DOWNLOAD_URL=$(echo "$RELEASE_JSON" | { grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*'"$ARTIFACT"'"' || true; } | head -1 | sed 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')
[ -z "$DOWNLOAD_URL" ] && error "Could not find artifact $ARTIFACT in release $TAG"

rm -rf "$TMP_DIR"
mkdir -p "$TMP_DIR"

curl -fSL --progress-bar -o "$TMP_DIR/$ARTIFACT" "$DOWNLOAD_URL" \
  || error "Failed to download $ARTIFACT"
success "Downloaded $ARTIFACT"

# ── Step 5: Install ──
step 5 "Installing to /Applications"

# `ditto -x -k` unpacks the zip preserving the bundle structure (xattrs,
# resource forks, symlinks) — the inverse of how deploy_releases.sh packs it.
ditto -x -k "$TMP_DIR/$ARTIFACT" "$TMP_DIR"
[ -d "$TMP_DIR/$APP_NAME" ] || error "Expected $APP_NAME inside $ARTIFACT, but didn't find it"

if [ -d "$DEST" ]; then
  rm -rf "$DEST"
  warn "Replaced existing $DEST"
fi
mv "$TMP_DIR/$APP_NAME" "$DEST"

# Strip the quarantine xattr Gatekeeper adds to anything downloaded via curl.
# Without this, ad-hoc-signed bundles trigger a "developer cannot be verified"
# dialog and won't open with a double-click. Stripping is the standard escape
# hatch for open-source / unsigned tools.
xattr -cr "$DEST" 2>/dev/null || true

# Register with Launch Services so Spotlight, Launchpad, and `open -a leogit`
# resolve the freshly-unpacked bundle. Without this, LS only learns about the
# app the first time Finder touches it.
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
[ -x "$LSREGISTER" ] && "$LSREGISTER" -f "$DEST" >/dev/null 2>&1 || true

success "Installed: $DEST"

rm -rf "$TMP_DIR"

echo -e "\n${GREEN}═══ LeoGit $VERSION installed ═══${NC}"
echo -e "  ${CYAN}Open from /Applications, Spotlight, or:  open $DEST${NC}"
