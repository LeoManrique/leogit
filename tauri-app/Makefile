.PHONY: help install dev build build-release clean check format

help:
	@echo "leogit - Visual Git client"
	@echo ""
	@echo "Available targets:"
	@echo "  install          Install dependencies"
	@echo "  dev              Run development server"
	@echo "  build            Build development binary"
	@echo "  build-release    Build optimized release binary"
	@echo "  clean            Clean build artifacts"
	@echo "  check            Run type checking"
	@echo "  format           Format code with prettier"

install:
	pnpm install

dev:
	pnpm tauri dev

build:
	pnpm tauri build

build-release:
	RUST_BACKTRACE=1 pnpm tauri build --release

clean:
	rm -rf dist target src-tauri/target node_modules pnpm-lock.yaml
	cargo clean --manifest-path src-tauri/Cargo.toml

check:
	pnpm check
	cargo check --manifest-path src-tauri/Cargo.toml

format:
	pnpm format
	cargo fmt --manifest-path src-tauri/Cargo.toml
