#!/usr/bin/env bash
set -euo pipefail

# Installs LeoGit from the latest GitHub release for the host platform:
#   macOS → /Applications/leogit.app
#   Linux → ~/.local/bin/leogit (AppImage) + an app-menu launcher
# Intended to be curlable:
#   curl -fsSL https://raw.githubusercontent.com/LeoManrique/leogit/main/scripts/install.sh | bash

# ── Colors ──
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; NC='\033[0m'

TOTAL_STEPS=6
step()    { echo -e "\n${BLUE}[$1/$TOTAL_STEPS]${NC} ${CYAN}$2${NC}"; }
success() { echo -e "  ${GREEN}✓ $1${NC}"; }
warn()    { echo -e "  ${YELLOW}⚠ $1${NC}"; }
error()   { echo -e "  ${RED}✗ $1${NC}"; exit 1; }

REPO="LeoManrique/leogit"
API_URL="https://api.github.com/repos/$REPO/releases/latest"
TMP_DIR="/tmp/leogit-install"

# ── leogit() shell command ────────────────────────────────────────────────────
# A `leogit` shell function lets you open a repo straight from a terminal:
# `leogit` (current dir) or `leogit <path>`. It resolves the directory and hands
# it to the app — on macOS via `open -n --args` (a fresh process whose argv the
# single-instance plugin forwards to the running window), on Linux via the PATH
# wrapper, backgrounded so the terminal returns. Installed into the rc file of
# the user's login shell (detected from $SHELL in Step 6 below).

# Emit the marker-delimited zsh/bash block for the current $OS. Quoted heredocs
# keep $dir / $target / $1 literal in the generated function body.
leogit_shell_block() {
  cat <<'HEADER'
# >>> leogit >>>
# LeoGit shell command — open a repository from the terminal: `leogit [dir]`.
# Managed by the LeoGit installer; re-running the installer replaces this block.
HEADER
  if [ "$OS" = "darwin" ]; then
    cat <<'MAC'
leogit() {
  local target="${1:-.}" dir
  if ! dir=$(cd "$target" 2>/dev/null && pwd); then
    echo "leogit: no such directory: $target" >&2
    return 1
  fi
  open -na "/Applications/leogit.app" --args "$dir"
}
MAC
  else
    cat <<'LINUX'
leogit() {
  local target="${1:-.}" dir
  if ! dir=$(cd "$target" 2>/dev/null && pwd); then
    echo "leogit: no such directory: $target" >&2
    return 1
  fi
  # `command` bypasses this function to reach the PATH wrapper; the subshell
  # backgrounds and detaches it so the prompt returns immediately.
  ( command leogit "$dir" >/dev/null 2>&1 & )
}
LINUX
  fi
  echo "# <<< leogit <<<"
}

# Replace any prior managed block in the given rc file, then append a fresh one.
# Idempotent: re-running the installer never stacks duplicate functions.
install_block_into_rc() {
  local rc="$1"
  touch "$rc" 2>/dev/null || return 1
  if grep -q '# >>> leogit >>>' "$rc" 2>/dev/null; then
    sed -i.bak '/# >>> leogit >>>/,/# <<< leogit <<</d' "$rc" && rm -f "$rc.bak"
    # Collapse the trailing blank line(s) the deletion leaves behind so repeated
    # installs don't accumulate empty lines. $(…) strips all trailing newlines;
    # printf restores exactly one.
    local body; body=$(cat "$rc"); printf '%s\n' "$body" > "$rc"
  fi
  { printf '\n'; leogit_shell_block; } >> "$rc"
}

# fish autoloads functions from their own files — write one (idempotent by
# overwrite) instead of editing config.fish. fish has no subshell operator, so
# we resolve the path with realpath rather than `cd && pwd`.
install_fish_function() {
  local dir="${XDG_CONFIG_HOME:-$HOME/.config}/fish/functions"
  mkdir -p "$dir" 2>/dev/null || return 1
  local f="$dir/leogit.fish"
  if [ "$OS" = "darwin" ]; then
    cat > "$f" <<'FISH'
function leogit --description 'Open a repository in LeoGit'
    set -l target $argv[1]
    test -z "$target"; and set target "."
    if not test -d "$target"
        echo "leogit: no such directory: $target" >&2
        return 1
    end
    open -na "/Applications/leogit.app" --args (realpath "$target")
end
FISH
  else
    cat > "$f" <<'FISH'
function leogit --description 'Open a repository in LeoGit'
    set -l target $argv[1]
    test -z "$target"; and set target "."
    if not test -d "$target"
        echo "leogit: no such directory: $target" >&2
        return 1
    end
    command leogit (realpath "$target") >/dev/null 2>&1 &
    disown
end
FISH
  fi
}

# Detect the login shell ($SHELL survives `curl … | bash` — it's inherited from
# the parent) and install the function into the matching rc file. Sets
# LEOGIT_SHELL_RC / LEOGIT_SHELL_NAME for the post-install hint; returns 1 for an
# unknown shell so the caller can print a manual snippet instead.
install_leogit_shell_command() {
  LEOGIT_SHELL_NAME=$(basename "${SHELL:-}")
  case "$LEOGIT_SHELL_NAME" in
    zsh)
      LEOGIT_SHELL_RC="${ZDOTDIR:-$HOME}/.zshrc"
      install_block_into_rc "$LEOGIT_SHELL_RC" ;;
    bash)
      # Linux interactive shells read ~/.bashrc; macOS Terminal starts login
      # shells that read ~/.bash_profile.
      [ "$OS" = "darwin" ] && LEOGIT_SHELL_RC="$HOME/.bash_profile" || LEOGIT_SHELL_RC="$HOME/.bashrc"
      install_block_into_rc "$LEOGIT_SHELL_RC" ;;
    fish)
      LEOGIT_SHELL_RC="${XDG_CONFIG_HOME:-$HOME/.config}/fish/functions/leogit.fish"
      install_fish_function ;;
    *)
      return 1 ;;
  esac
}

# ── Step 1: Detect platform ──
step 1 "Detecting platform"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
  arm64|aarch64) ARCH="arm64" ;;
  x86_64)        ARCH="amd64" ;;
  *) error "Unsupported architecture: $ARCH" ;;
esac
case "$OS" in
  darwin) PLATFORM="macOS-$ARCH"; ARTIFACT_EXT="zip" ;;
  linux)  PLATFORM="linux-$ARCH"; ARTIFACT_EXT="AppImage" ;;
  *) error "Unsupported OS: $OS (LeoGit supports macOS and Linux)" ;;
esac
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
ARTIFACT="LeoGit-$VERSION-$PLATFORM.$ARTIFACT_EXT"
step 4 "Downloading $ARTIFACT"

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
step 5 "Installing LeoGit"

if [ "$OS" = "darwin" ]; then
  APP_NAME="leogit.app"
  DEST="/Applications/$APP_NAME"

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
else
  # AppImage is a single self-contained executable. Install it as
  # `leogit.AppImage` and put a small `leogit` wrapper onto PATH that execs it;
  # register a .desktop launcher so GNOME/COSMIC app menus list it.
  BIN_DIR="$HOME/.local/bin"
  APPIMAGE_DEST="$BIN_DIR/leogit.AppImage"
  DEST="$BIN_DIR/leogit"
  mkdir -p "$BIN_DIR"
  [ -e "$APPIMAGE_DEST" ] && warn "Replaced existing $APPIMAGE_DEST"
  mv -f "$TMP_DIR/$ARTIFACT" "$APPIMAGE_DEST"
  chmod +x "$APPIMAGE_DEST"

  # WebKitGTK's DMABUF/GBM renderer is broken on the NVIDIA proprietary driver:
  # it spams "Failed to create GBM buffer of size WxH: Invalid argument" and can
  # leave a blank or garbled window. Disabling that renderer falls back to a
  # working path. We detect at launch (not install) because the active GPU and
  # session type are runtime properties. `/dev/nvidia0` exists only when the
  # proprietary driver is loaded — nouveau uses /dev/dri and isn't affected — so
  # this stays inert on AMD/Intel/nouveau. An already-set value is honored, so
  # users can force it on or off.
  cat > "$DEST" <<'WRAPPER'
#!/usr/bin/env bash
APPIMAGE="$(dirname "$(readlink -f "$0")")/leogit.AppImage"

# Optional per-user launch tweaks (extra env vars, compositor-specific scaling).
# Kept in a separate file the installer never overwrites, so customizations
# survive updates that regenerate this wrapper. Ships nothing by default, so
# this is inert unless the user creates it. Example (Hyprland fractional scale):
#   if [ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]; then
#     export GDK_BACKEND=x11 GDK_DPI_SCALE=1.5
#   fi
LAUNCH_ENV="${XDG_CONFIG_HOME:-$HOME/.config}/leogit/launch-env"
[ -f "$LAUNCH_ENV" ] && . "$LAUNCH_ENV"

if [ -z "${WEBKIT_DISABLE_DMABUF_RENDERER:-}" ] && [ -e /dev/nvidia0 ]; then
  export WEBKIT_DISABLE_DMABUF_RENDERER=1
fi
exec "$APPIMAGE" "$@"
WRAPPER
  chmod +x "$DEST"

  # Pull the bundled icon out for the launcher. --appimage-extract unpacks
  # without needing FUSE; we keep only the icon and discard the squashfs tree.
  ICON_DIR="$HOME/.local/share/icons"
  ICON_DEST="$ICON_DIR/leogit.png"
  mkdir -p "$ICON_DIR"
  (
    cd "$TMP_DIR"
    "$APPIMAGE_DEST" --appimage-extract >/dev/null 2>&1 || true
    ICON_SRC=$(ls squashfs-root/*.png 2>/dev/null | head -1 || true)
    [ -z "$ICON_SRC" ] && [ -f squashfs-root/.DirIcon ] && ICON_SRC="squashfs-root/.DirIcon"
    [ -n "$ICON_SRC" ] && cp "$ICON_SRC" "$ICON_DEST"
  ) || warn "Could not extract app icon (launcher will use a default)"

  APPS_DIR="$HOME/.local/share/applications"
  DESKTOP_FILE="$APPS_DIR/leogit.desktop"
  mkdir -p "$APPS_DIR"
  cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=LeoGit
Comment=A fast, native Git client
Exec=$DEST
Icon=$ICON_DEST
Terminal=false
Categories=Development;RevisionControl;
EOF
  command -v update-desktop-database &>/dev/null \
    && update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true

  # AppImages need FUSE 2 to mount-and-run; Arch ships only FUSE 3 by default.
  if ! ldconfig -p 2>/dev/null | grep -q 'libfuse\.so\.2'; then
    warn "FUSE 2 not found — LeoGit won't launch without it (Arch: sudo pacman -S fuse2)"
  fi

  success "Installed: $DEST"
fi

rm -rf "$TMP_DIR"

# ── Step 6: Install the `leogit` shell command ──
step 6 "Installing the leogit shell command"

if install_leogit_shell_command; then
  success "Added leogit() to $LEOGIT_SHELL_RC ($LEOGIT_SHELL_NAME)"
else
  warn "Unrecognized shell '${LEOGIT_SHELL_NAME:-unknown}' — add this to your shell config manually:"
  leogit_shell_block
fi

echo -e "\n${GREEN}═══ LeoGit $VERSION installed ═══${NC}"
if [ "$OS" = "darwin" ]; then
  echo -e "  ${CYAN}Open from /Applications, Spotlight, or:  open $DEST${NC}"
else
  echo -e "  ${CYAN}Launch from your app menu, or run:  leogit${NC}"
  echo -e "  ${CYAN}(ensure ~/.local/bin is on your PATH)${NC}"
fi
if [ -n "${LEOGIT_SHELL_RC:-}" ]; then
  echo -e "  ${CYAN}Terminal command:  leogit [dir]  — open a repo in LeoGit${NC}"
  echo -e "  ${CYAN}Run it in a new terminal (or 'source $LEOGIT_SHELL_RC' first).${NC}"
fi
