#!/usr/bin/env bash
set -euo pipefail

# Releases a new version of LeoGit:
#   1. Validates prerequisites (gh, pnpm, cargo, git + platform-specific tools)
#   2. Resolves / bumps version in tauri.conf.json, Cargo.toml, package.json
#   3. Tags the release
#   4. Bundles the app via scripts/bundle.sh (drives pnpm tauri build)
#   5. Packages and uploads to GitHub Releases
#
# Runs on macOS, Linux, and Windows (via Git-Bash/MSYS2). Each platform builds
# and uploads its own artifact to the shared GitHub Release: macOS ships a
# zipped .app, Linux ships an AppImage, Windows ships an NSIS installer
# (-setup.exe). Run it once per platform to publish a complete release.
#
# Usage: scripts/deploy_releases.sh [x.y.z]
#   If a version is passed and it's higher than the current one, the script
#   bumps and commits the version files first so the tag points at the bump
#   commit. Without an arg, uses whatever version is already set.

# ── Colors ──
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; NC='\033[0m'

TOTAL_STEPS=5
REPO="LeoManrique/leogit"
step()    { echo -e "\n${BLUE}[$1/$TOTAL_STEPS]${NC} ${CYAN}$2${NC}"; }
success() { echo -e "  ${GREEN}✓ $1${NC}"; }
warn()    { echo -e "  ${YELLOW}⚠ $1${NC}"; }
error()   { echo -e "  ${RED}✗ $1${NC}"; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
APP_DIR="$PROJECT_ROOT/tauri-app"
TAURI_CONF="$APP_DIR/src-tauri/tauri.conf.json"
CARGO_TOML="$APP_DIR/src-tauri/Cargo.toml"
CARGO_LOCK="$APP_DIR/src-tauri/Cargo.lock"
PKG_JSON="$APP_DIR/package.json"
cd "$PROJECT_ROOT"

VERSION="${1:-}"

read_version() {
  grep -o '"version"[[:space:]]*:[[:space:]]*"[^"]*"' "$TAURI_CONF" | head -1 | sed 's/.*"\([^"]*\)"$/\1/'
}

# ── Platform detection ──
# The build/package/upload steps fork on the host OS; everything else (version
# bump, tagging, gh upload) is shared.
OS_KERNEL=$(uname -s)
case "$OS_KERNEL" in
  Darwin)               TARGET_OS="macOS";   ARTIFACT_EXT="zip" ;;
  Linux)                TARGET_OS="linux";   ARTIFACT_EXT="AppImage" ;;
  MINGW*|MSYS*|CYGWIN*) TARGET_OS="windows"; ARTIFACT_EXT="exe" ;;
  *)                    error "Unsupported OS: $OS_KERNEL (LeoGit builds on macOS, Linux, and Windows)" ;;
esac

# ── Step 1: Validate prerequisites ──
step 1 "Validating prerequisites"

# codesign/ditto are macOS-only; Linux relies on Tauri's bundled AppImage tooling.
PLATFORM_TOOLS=""
[ "$TARGET_OS" = "macOS" ] && PLATFORM_TOOLS="codesign ditto"
for cmd in gh pnpm cargo git $PLATFORM_TOOLS; do
  command -v "$cmd" &>/dev/null || error "$cmd is not installed"
  success "$cmd found"
done

# Linux AppImage builds link against the WebKitGTK dev headers Tauri uses.
if [ "$TARGET_OS" = "linux" ]; then
  if command -v pkg-config &>/dev/null && pkg-config --exists webkit2gtk-4.1; then
    success "webkit2gtk-4.1 found"
  else
    warn "webkit2gtk-4.1 not detected — install it if 'tauri build' fails (Arch: sudo pacman -S webkit2gtk-4.1)"
  fi
fi

gh auth status &>/dev/null || error "gh CLI not authenticated. Run: gh auth login"
success "gh authenticated"

# Working tree must be clean — otherwise we can't be sure what's in the tag.
if ! git diff --quiet || ! git diff --cached --quiet; then
  error "Working tree is dirty — commit or stash changes before releasing"
fi
success "Working tree clean"

# ── Step 2: Determine + bump version ──
step 2 "Determining version"

CURRENT_VERSION=$(read_version)
[ -z "$CURRENT_VERSION" ] && error "Could not read version from tauri.conf.json"

# Validate an explicit version arg up front; otherwise ship the current version.
if [ -n "$VERSION" ]; then
  [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || error "Version must be x.y.z (got: $VERSION)"
else
  VERSION="$CURRENT_VERSION"
fi

# Guard: never publish behind GitHub's "latest" release. install.sh installs
# from /releases/latest, so uploading artifacts to an older tag leaves them
# invisible to installers and can strand the real latest release without this
# platform's build. A stale version here almost always means the local tree is
# behind origin — the fix is `git pull`. Skipped when no release exists yet.
LATEST_TAG=$(gh api "repos/$REPO/releases/latest" --jq '.tag_name' 2>/dev/null || true)
if [ -n "$LATEST_TAG" ]; then
  LATEST_VERSION="${LATEST_TAG#v}"
  OLDER=$(printf '%s\n%s\n' "$VERSION" "$LATEST_VERSION" | sort -V | head -1)
  if [ "$VERSION" != "$LATEST_VERSION" ] && [ "$OLDER" = "$VERSION" ]; then
    error "Refusing to release v$VERSION: GitHub's latest release is already $LATEST_TAG. Your local tree is likely behind origin — run 'git pull' (then bump if you intend a new version) and retry."
  fi
  success "Latest GitHub release: $LATEST_TAG"
else
  success "No published release yet — this will be the first"
fi

if [ "$VERSION" = "$CURRENT_VERSION" ]; then
  success "Using current version: $VERSION"
else
  HIGHER=$(printf '%s\n%s\n' "$CURRENT_VERSION" "$VERSION" | sort -V | tail -1)
  [ "$HIGHER" = "$VERSION" ] || error "New version $VERSION is not greater than current $CURRENT_VERSION"
  # tauri.conf.json + package.json: "version": "X"
  sed -i.bak "s/\"version\"[[:space:]]*:[[:space:]]*\"$CURRENT_VERSION\"/\"version\": \"$VERSION\"/" "$TAURI_CONF"
  sed -i.bak "s/\"version\"[[:space:]]*:[[:space:]]*\"$CURRENT_VERSION\"/\"version\": \"$VERSION\"/" "$PKG_JSON"
  # Cargo.toml: the package version is the only line starting with `version =`
  # (dependency versions are `name = { version = ... }` or `name = "x"`).
  sed -i.bak "s/^version = \"$CURRENT_VERSION\"/version = \"$VERSION\"/" "$CARGO_TOML"
  rm -f "$TAURI_CONF.bak" "$PKG_JSON.bak" "$CARGO_TOML.bak"
  # Cargo.lock records the workspace package's own version too. Sync it now
  # (-w touches only workspace members, not dependency pins) so the build in
  # step 4 doesn't resync it and leave the tree dirty after the release.
  (cd "$APP_DIR/src-tauri" && cargo update -w >/dev/null 2>&1) \
    || warn "cargo update -w failed; Cargo.lock may be left stale"
  git add "$TAURI_CONF" "$PKG_JSON" "$CARGO_TOML" "$CARGO_LOCK"
  git commit -m "Bump version to $VERSION"
  git push
  success "Bumped $CURRENT_VERSION → $VERSION and pushed"
fi

TAG="v$VERSION"
if [ "$TARGET_OS" = "windows" ]; then
  # On Windows the Tauri build targets rustc's host triple, which need not match
  # the shell's reported arch — an x86_64 MSYS2/Git-Bash shell emulated on an
  # ARM64 host still builds (and NSIS-packages) aarch64-pc-windows-msvc. Label
  # the artifact by what actually gets built, not by `uname -m`.
  RUST_HOST=$(rustc -vV | sed -n 's/^host: //p')
  case "$RUST_HOST" in
    aarch64-*) ARCH="arm64" ;;
    x86_64-*)  ARCH="amd64" ;;
    *)         error "Unsupported Windows build target: ${RUST_HOST:-unknown}" ;;
  esac
else
  ARCH=$(uname -m)
  case "$ARCH" in
    arm64|aarch64) ARCH="arm64" ;;
    x86_64)        ARCH="amd64" ;;
  esac
fi
PLATFORM="$TARGET_OS-$ARCH"
success "Version: $VERSION (tag: $TAG), platform: $PLATFORM"

# ── Step 3: Create git tag ──
step 3 "Tagging release"

if git rev-parse "$TAG" &>/dev/null; then
  warn "Tag $TAG already exists, skipping"
else
  git tag -a "$TAG" -m "Release $TAG"
  git push origin "$TAG"
  success "Created and pushed tag $TAG"
fi

# ── Step 4: Bundle + package ──
step 4 "Bundling LeoGit"

"$SCRIPT_DIR/bundle.sh"

DIST_DIR="$PROJECT_ROOT/dist"
mkdir -p "$DIST_DIR"
# macOS/Linux ship a single file with a plain extension; Windows ships an NSIS
# installer, conventionally suffixed "-setup.exe".
if [ "$TARGET_OS" = "windows" ]; then
  ARTIFACT="LeoGit-$VERSION-$PLATFORM-setup.exe"
else
  ARTIFACT="LeoGit-$VERSION-$PLATFORM.$ARTIFACT_EXT"
fi
ARTIFACT_PATH="$DIST_DIR/$ARTIFACT"
rm -f "$ARTIFACT_PATH"

case "$TARGET_OS" in
  macOS)
    APP_PATH="$APP_DIR/src-tauri/target/release/bundle/macos/leogit.app"
    [ -d "$APP_PATH" ] || error "Bundle script did not produce $APP_PATH"
    # `ditto -c -k --keepParent` is the macOS-canonical way to zip an .app:
    # preserves resource forks, xattrs, and symlinks (plain `zip` strips them and
    # can produce a bundle that Finder refuses to open after extraction).
    ditto -c -k --keepParent "$APP_PATH" "$ARTIFACT_PATH"
    ;;
  windows)
    # Tauri names the NSIS installer leogit_<version>_<arch>-setup.exe under
    # bundle/nsis/. bundle.sh wiped stale installers first, but glob by version
    # anyway so we never grab a leftover from an earlier version's build.
    SETUP=$(ls "$APP_DIR/src-tauri/target/release/bundle/nsis/"*_"$VERSION"_*-setup.exe 2>/dev/null | head -1 || true)
    [ -n "$SETUP" ] && [ -f "$SETUP" ] || error "Bundle script did not produce an NSIS installer for $VERSION"
    cp "$SETUP" "$ARTIFACT_PATH"
    ;;
  *)
    # Tauri writes the AppImage under bundle/appimage/ with its own version/arch
    # naming, so glob for it rather than reconstructing the filename. The AppImage
    # is already a single self-contained executable — just copy it into dist/.
    APPIMAGE=$(ls "$APP_DIR/src-tauri/target/release/bundle/appimage/"*.AppImage 2>/dev/null | head -1 || true)
    [ -n "$APPIMAGE" ] && [ -f "$APPIMAGE" ] || error "Bundle script did not produce an AppImage"
    cp "$APPIMAGE" "$ARTIFACT_PATH"
    chmod +x "$ARTIFACT_PATH"
    ;;
esac
success "Packaged: $ARTIFACT ($(du -h "$ARTIFACT_PATH" | cut -f1))"

# ── Step 5: Upload to GitHub Release ──
step 5 "Uploading to GitHub Release"

if gh release view "$TAG" --repo "$REPO" &>/dev/null; then
  warn "Release $TAG already exists, uploading artifacts (clobber)"
  gh release upload "$TAG" "$ARTIFACT_PATH" --clobber --repo "$REPO"
else
  # Notes cover both platforms regardless of which one creates the release —
  # the other platform uploads its artifact with --clobber and leaves notes as-is.
  gh release create "$TAG" "$ARTIFACT_PATH" \
    --repo "$REPO" \
    --title "LeoGit $TAG" \
    --notes "## LeoGit $TAG

A fast, native Git client built with Tauri and Svelte.

### Install

\`\`\`
curl -fsSL https://raw.githubusercontent.com/$REPO/main/scripts/install.sh | bash
\`\`\`

The installer auto-detects your platform (macOS and Linux).

**macOS** — or download \`LeoGit-$VERSION-macOS-*.zip\` below and drag \`leogit.app\` into \`/Applications\`. The bundle is ad-hoc signed, so on first launch right-click → Open (or run \`xattr -cr /Applications/leogit.app\` — the install script does this for you).

**Linux** — or download \`LeoGit-$VERSION-linux-*.AppImage\` below, \`chmod +x\` it, and run. The install script also drops a launcher in your app menu.

**Windows** — download \`LeoGit-$VERSION-windows-*-setup.exe\` below and run it. It installs per-user (no admin), adds a Start Menu shortcut, and pulls in the WebView2 runtime if it's missing.

### Requirements

- macOS 14+ (Intel or Apple Silicon)
- Linux (x86_64) with WebKitGTK 4.1 + FUSE 2 (Arch: \`sudo pacman -S webkit2gtk-4.1 fuse2\`)
- Windows 10/11 (the installer bundles the WebView2 bootstrapper)"
fi
success "Uploaded $ARTIFACT to release $TAG"

echo -e "\n${GREEN}═══ Release $TAG complete ($PLATFORM) ═══${NC}"
echo -e "  ${CYAN}https://github.com/$REPO/releases/tag/$TAG${NC}"
