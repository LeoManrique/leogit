#!/usr/bin/env bash
set -euo pipefail

# Builds leogit-ffi and regenerates its Swift bindings.
#
# Runs standalone (`apps/swift-ui-app/scripts/build-rust.sh [debug|release]`)
# and as an Xcode pre-build phase, where $CONFIGURATION supplies the profile.
#
# Outputs, all consumed by the Xcode target:
#   target/<profile>/libleogit_ffi.a       static lib the app links
#   ffi/generated/LeoGitCore.swift         typed Swift API (compiled into the app)
#   ffi/generated/LeoGitCoreFFI.h          C shim header
#   ffi/generated/module.modulemap         makes the header importable as a Clang module
#
# ffi/generated/ is gitignored: it is derived from the Rust source and must never
# be edited by hand or allowed to drift from the compiled library.

RED='\033[0;31m'; GREEN='\033[0;32m'; CYAN='\033[0;36m'; NC='\033[0m'
success() { echo -e "  ${GREEN}✓ $1${NC}"; }
error()   { echo -e "  ${RED}✗ $1${NC}"; exit 1; }
info()    { echo -e "${CYAN}$1${NC}"; }

# Under Xcode, $SRCROOT is authoritative and points at apps/swift-ui-app. Falling
# back to this file's own location covers standalone runs — and is deliberately
# the *second* choice: a build phase that inlines this script would otherwise
# resolve the repo root to Xcode's intermediates directory.
if [ -n "${SRCROOT:-}" ]; then
  APP_DIR="$SRCROOT"
else
  APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi
PROJECT_ROOT="$(cd "$APP_DIR/../.." && pwd)"
GENERATED_DIR="$APP_DIR/ffi/generated"

[ -f "$PROJECT_ROOT/Cargo.toml" ] \
  || error "expected the Cargo workspace root at $PROJECT_ROOT (app dir resolved to $APP_DIR)"

# Profile: explicit argument wins, else Xcode's $CONFIGURATION, else debug.
PROFILE="${1:-}"
if [ -z "$PROFILE" ]; then
  case "${CONFIGURATION:-Debug}" in
    Release) PROFILE="release" ;;
    *)       PROFILE="debug" ;;
  esac
fi
[ "$PROFILE" = "debug" ] || [ "$PROFILE" = "release" ] \
  || error "unknown profile '$PROFILE' (expected debug or release)"

# Xcode's build phases run with a minimal PATH that omits ~/.cargo/bin, so
# resolve cargo explicitly rather than relying on the ambient shell.
CARGO="$(command -v cargo || true)"
[ -n "$CARGO" ] || CARGO="$HOME/.cargo/bin/cargo"
[ -x "$CARGO" ] || error "cargo not found (looked on PATH and in ~/.cargo/bin)"

cd "$PROJECT_ROOT"

info "Building leogit-ffi ($PROFILE)…"
if [ "$PROFILE" = "release" ]; then
  "$CARGO" build -p leogit-ffi --release
else
  "$CARGO" build -p leogit-ffi
fi

LIB="$PROJECT_ROOT/target/$PROFILE/libleogit_ffi.a"
[ -f "$LIB" ] || error "expected static library at $LIB"
success "$(basename "$LIB") ($(du -h "$LIB" | cut -f1))"

# Bindings are generated in "library mode": uniffi reads type metadata back out
# of the compiled archive, which is what makes proc-macro exports (no .udl file)
# discoverable. The generator therefore must run *after* the cargo build above.
info "Generating Swift bindings…"
mkdir -p "$GENERATED_DIR"

# `module.modulemap` is the filename Clang auto-discovers on an include path;
# naming it anything else would require an extra -fmodule-map-file flag.
#
# --module-name is load-bearing: the generated Swift imports the C shim as
# `#if canImport(LeoGitCoreFFI)`, derived from uniffi.toml's module_name. The
# modulemap otherwise declares the module under the *crate* name (leogit_ffi),
# and because the import sits behind canImport the mismatch does not error — it
# silently skips the import, and every FFI symbol then fails to resolve. Keep
# this in sync with `module_name` in ffi/uniffi.toml.
# Frameworks demanded by core's own dependency graph, not by uniffi: reqwest →
# hyper-util → system-configuration reads the system proxy settings. Declaring
# them in the modulemap makes the requirement travel with the library, so every
# consumer links them automatically instead of each app rediscovering a wall of
# undefined SC* symbols in its own build settings.
FFI_MODULE="LeoGitCoreFFI"
"$CARGO" run --quiet --features bindgen --bin uniffi-bindgen-swift -- \
  "$LIB" "$GENERATED_DIR" --swift-sources
"$CARGO" run --quiet --features bindgen --bin uniffi-bindgen-swift -- \
  "$LIB" "$GENERATED_DIR" --headers --modulemap \
  --modulemap-filename module.modulemap --module-name "$FFI_MODULE" \
  --link-frameworks SystemConfiguration

# Guard the invariant above rather than trusting it: a future uniffi release
# changing either default would otherwise surface as an unresolved-symbol wall.
grep -q "^module $FFI_MODULE " "$GENERATED_DIR/module.modulemap" \
  || error "modulemap declares a module other than $FFI_MODULE — it must match the canImport guard in LeoGitCore.swift"
grep -q "canImport($FFI_MODULE)" "$GENERATED_DIR/LeoGitCore.swift" \
  || error "generated Swift does not import $FFI_MODULE — check module_name in ffi/uniffi.toml"

for f in LeoGitCore.swift LeoGitCoreFFI.h module.modulemap; do
  [ -f "$GENERATED_DIR/$f" ] || error "binding generator did not produce $f"
done
success "bindings → ${GENERATED_DIR#"$PROJECT_ROOT"/}"
