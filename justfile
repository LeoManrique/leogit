app := "apps/tauri-app"
mac := "apps/swift-ui-app"

# `just` runs every recipe with `sh`, including on Windows. Git for Windows ships
# it at C:\Program Files\Git\usr\bin; keep that directory on PATH so the POSIX
# recipes below (cd && …, rm -rf, inline env vars, the /dev/nvidia0 guard) run
# unchanged on every platform. Pinned explicitly so it survives a future just
# release that might default Windows to PowerShell.
set windows-shell := ["sh", "-cu"]

# List available recipes
default:
    @just --list

# Install dependencies
install:
    cd {{app}} && pnpm install

# Run development server
#
# `just dev` runs `cargo run` directly, so it bypasses the install.sh wrapper
# that exports WEBKIT_DISABLE_DMABUF_RENDERER on NVIDIA. Apply the same guard
# here: WebKitGTK's DMABUF/GBM renderer crashes on the proprietary NVIDIA driver
# (blank window / "Gdk Error 71 Protocol error" on Wayland). /dev/nvidia0 exists
# only with the proprietary driver, so this stays inert on AMD/Intel/nouveau and
# honors a pre-set value.
# Run the Tauri app in development mode
dev:
    cd {{app}} && [ -z "${WEBKIT_DISABLE_DMABUF_RENDERER:-}" ] && [ -e /dev/nvidia0 ] && export WEBKIT_DISABLE_DMABUF_RENDERER=1; pnpm tauri dev

# Build development binary
build:
    cd {{app}} && pnpm tauri build

# Build optimized release binary
build-release:
    cd {{app}} && RUST_BACKTRACE=1 pnpm tauri build --release

# ---------------------------------------------------------------------------
# Native macOS app (SwiftUI). Requires Xcode and XcodeGen (`brew install xcodegen`).
# ---------------------------------------------------------------------------

# Build leogit-ffi and regenerate its Swift bindings, without touching Xcode
mac-bindings:
    {{mac}}/scripts/build-rust.sh

# The project is generated from project.yml and never committed, so run this
# after editing the spec or adding source files. Bindings are generated first
# because XcodeGen resolves `sources` at generation time — on a clean tree the
# generated Swift would otherwise be missing, and Xcode plans the build around
# an input that its own pre-build phase has not created yet.
# Regenerate LeoGit.xcodeproj from project.yml
mac-generate: mac-bindings
    cd {{mac}} && xcodegen generate

# Xcode's pre-build phase runs build-rust.sh, so the Rust core and its bindings
# are always rebuilt first.
# Build the macOS app
mac-build: mac-generate
    cd {{mac}} && xcodebuild -project LeoGit.xcodeproj -scheme LeoGit -configuration Debug -derivedDataPath build build

# Build and launch the macOS app
mac-run: mac-build
    open {{mac}}/build/Build/Products/Debug/LeoGit.app

# The Cargo workspace target/ lives at the repo root, so `cargo clean` wipes it
# for every crate at once; the macOS app's generated project and bindings go too.
# Clean all build artifacts
clean:
    cd {{app}} && rm -rf dist node_modules pnpm-lock.yaml
    cd {{mac}} && rm -rf build ffi/generated Generated LeoGit.xcodeproj
    cargo clean

# Run type checking (frontend + the whole Rust workspace: core + host)
check:
    cd {{app}} && pnpm check
    cargo check --workspace

# Format code with prettier and rustfmt (whole workspace)
format:
    cd {{app}} && pnpm format
    cargo fmt --all
