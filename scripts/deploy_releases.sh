#!/usr/bin/env bash
set -euo pipefail

# Releases a new version of LeoGit:
#   1. Validates prerequisites (gh, pnpm, cargo, codesign, git, ditto)
#   2. Resolves / bumps version in tauri.conf.json, Cargo.toml, package.json
#   3. Tags the release
#   4. Bundles leogit.app via scripts/bundle.sh (drives pnpm tauri build)
#   5. Zips and uploads to GitHub Releases
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

# ── Step 1: Validate prerequisites ──
step 1 "Validating prerequisites"

[ "$(uname -s)" = "Darwin" ] || error "This script only runs on macOS"

for cmd in gh pnpm cargo codesign git ditto; do
  command -v "$cmd" &>/dev/null || error "$cmd is not installed"
  success "$cmd found"
done

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

if [ -z "$VERSION" ]; then
  VERSION="$CURRENT_VERSION"
  success "Using current version: $VERSION"
else
  [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || error "Version must be x.y.z (got: $VERSION)"
  if [ "$VERSION" = "$CURRENT_VERSION" ]; then
    success "Version $VERSION already set, no bump needed"
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
fi

TAG="v$VERSION"
ARCH=$(uname -m)
case "$ARCH" in
  arm64|aarch64) ARCH="arm64" ;;
  x86_64)        ARCH="amd64" ;;
esac
PLATFORM="macOS-$ARCH"
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

# ── Step 4: Bundle + zip ──
step 4 "Bundling leogit.app"

"$SCRIPT_DIR/bundle.sh"
APP_PATH="$APP_DIR/src-tauri/target/release/bundle/macos/leogit.app"
[ -d "$APP_PATH" ] || error "Bundle script did not produce $APP_PATH"

DIST_DIR="$PROJECT_ROOT/dist"
mkdir -p "$DIST_DIR"
ARTIFACT="LeoGit-$VERSION-$PLATFORM.zip"
ARTIFACT_PATH="$DIST_DIR/$ARTIFACT"
rm -f "$ARTIFACT_PATH"
# `ditto -c -k --keepParent` is the macOS-canonical way to zip an .app:
# preserves resource forks, xattrs, and symlinks (plain `zip` strips them and
# can produce a bundle that Finder refuses to open after extraction).
ditto -c -k --keepParent "$APP_PATH" "$ARTIFACT_PATH"
success "Packaged: $ARTIFACT ($(du -h "$ARTIFACT_PATH" | cut -f1))"

# ── Step 5: Upload to GitHub Release ──
step 5 "Uploading to GitHub Release"

if gh release view "$TAG" --repo "$REPO" &>/dev/null; then
  warn "Release $TAG already exists, uploading artifacts (clobber)"
  gh release upload "$TAG" "$ARTIFACT_PATH" --clobber --repo "$REPO"
else
  gh release create "$TAG" "$ARTIFACT_PATH" \
    --repo "$REPO" \
    --title "LeoGit $TAG" \
    --notes "## LeoGit $TAG

A fast, native Git client built with Tauri and Svelte.

### Install

\`\`\`
curl -fsSL https://raw.githubusercontent.com/$REPO/main/scripts/install.sh | bash
\`\`\`

Or download \`$ARTIFACT\` below and drag \`leogit.app\` into \`/Applications\`. The bundle is ad-hoc signed, so on first launch right-click → Open (or run \`xattr -cr /Applications/leogit.app\` — the install script does this for you).

### Requirements

- macOS 14+ (Intel or Apple Silicon)"
fi
success "Uploaded $ARTIFACT to release $TAG"

echo -e "\n${GREEN}═══ Release $TAG complete ($PLATFORM) ═══${NC}"
echo -e "  ${CYAN}https://github.com/$REPO/releases/tag/$TAG${NC}"
