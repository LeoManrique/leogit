app := "tauri-app"

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
dev:
    cd {{app}} && [ -z "${WEBKIT_DISABLE_DMABUF_RENDERER:-}" ] && [ -e /dev/nvidia0 ] && export WEBKIT_DISABLE_DMABUF_RENDERER=1; pnpm tauri dev

# Build development binary
build:
    cd {{app}} && pnpm tauri build

# Build optimized release binary
build-release:
    cd {{app}} && RUST_BACKTRACE=1 pnpm tauri build --release

# Clean build artifacts
clean:
    cd {{app}} && rm -rf dist target src-tauri/target node_modules pnpm-lock.yaml
    cd {{app}} && cargo clean --manifest-path src-tauri/Cargo.toml

# Run type checking
check:
    cd {{app}} && pnpm check
    cd {{app}} && cargo check --manifest-path src-tauri/Cargo.toml

# Format code with prettier and rustfmt
format:
    cd {{app}} && pnpm format
    cd {{app}} && cargo fmt --manifest-path src-tauri/Cargo.toml
