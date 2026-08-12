#!/bin/bash
set -e

echo "🔍 leogit Build Verification Script"
echo "===================================="
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

check() {
  if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓${NC} $1"
  else
    echo -e "${RED}✗${NC} $1"
    exit 1
  fi
}

# Prerequisites
echo "📦 Checking prerequisites..."
which git > /dev/null
check "Git installed"

which node > /dev/null
check "Node.js installed"

which pnpm > /dev/null
check "pnpm installed"

which cargo > /dev/null
check "Rust/cargo installed"

echo ""
echo "🧹 Dependencies..."
pnpm install > /dev/null 2>&1
check "npm dependencies installed"

echo ""
echo "🔨 Type checking..."
pnpm check > /dev/null 2>&1
check "TypeScript type checking"

cargo check --manifest-path src-tauri/Cargo.toml > /dev/null 2>&1
check "Rust compilation"

echo ""
echo "📦 Building frontend..."
pnpm run build:frontend > /dev/null 2>&1
check "Frontend build (Vite)"

[ -d dist ] && [ "$(ls -A dist)" ]
check "Frontend dist created"

echo ""
echo "🎯 Build targets..."
# Check if we can build (this takes longer, so optional)
if [ "$1" = "--full" ]; then
  echo "Building release binary (this may take a few minutes)..."
  pnpm tauri build --release > /dev/null 2>&1
  check "Release binary build"
fi

echo ""
echo "✨ All checks passed!"
echo ""
echo "Next steps:"
echo "  • Run: pnpm tauri dev        # Development server"
echo "  • Or:  pnpm tauri build      # Build app"
echo "  • Or:  ./verify-build.sh --full  # Full release build"
