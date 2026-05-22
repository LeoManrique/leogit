app := "tauri-app"

# List available recipes
default:
    @just --list

# Install dependencies
install:
    cd {{app}} && pnpm install

# Run development server
dev:
    cd {{app}} && pnpm tauri dev

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
