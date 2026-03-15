#!/usr/bin/env bash
set -euo pipefail

APP_NAME="ButterVoice"
BUNDLE_ID="com.lpshanley.buttervoice"
INSTALL_DIR="/Applications"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_OUTPUT="$PROJECT_DIR/src-tauri/target/release/bundle/macos/${APP_NAME}.app"
DMG_OUTPUT="$PROJECT_DIR/src-tauri/target/release/bundle/dmg/${APP_NAME}_0.1.0_aarch64.dmg"

BUILD_MODE="${1:-app}"
case "$BUILD_MODE" in
  app|local)
    BUNDLE_ARGS="--bundles app"
    ;;
  release|dmg|all)
    BUNDLE_ARGS="--bundles app,dmg"
    ;;
  *)
    fail "Unknown build mode '$BUILD_MODE'. Use one of: app (default), local, release, dmg, all."
    ;;
esac

# --- Helpers ---

info()  { printf "\033[1;34m==>\033[0m %s\n" "$1"; }
ok()    { printf "\033[1;32m==>\033[0m %s\n" "$1"; }
warn()  { printf "\033[1;33m==>\033[0m %s\n" "$1"; }
fail()  { printf "\033[1;31m==>\033[0m %s\n" "$1" >&2; exit 1; }

require() {
    if ! command -v "$1" &>/dev/null; then
        fail "$1 is required but not found. $2"
    fi
    info "$1 $(command $1 --version 2>/dev/null | head -1)"
}

# --- Preflight ---

info "Checking prerequisites..."

[[ "$(uname)" == "Darwin" ]] || fail "This script only runs on macOS."

require rustc  "Install via: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
require cargo  "Install via: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
require node   "Install via: https://nodejs.org"
require pnpm   "Install via: npm install -g pnpm"

echo ""

# --- Install frontend dependencies ---

info "Installing frontend dependencies..."
(cd "$PROJECT_DIR" && pnpm install)
echo ""

# --- Build ---

info "Building ${APP_NAME} (this may take a few minutes on first run)..."
(cd "$PROJECT_DIR" && pnpm tauri build ${BUNDLE_ARGS})

if [[ ! -d "$BUILD_OUTPUT" ]]; then
    fail "Build output not found at $BUILD_OUTPUT"
fi
ok "Build succeeded."
if [[ "$BUILD_MODE" == release || "$BUILD_MODE" == dmg || "$BUILD_MODE" == all ]]; then
    if [[ -f "$DMG_OUTPUT" ]]; then
        ok "DMG output: $DMG_OUTPUT"
    else
        warn "DMG was requested but no DMG artifact was found at expected path: $DMG_OUTPUT"
    fi
fi
echo ""

# --- Kill running instance ---

if pgrep -x "$APP_NAME" &>/dev/null; then
    warn "Killing running ${APP_NAME} instance..."
    pkill -x "$APP_NAME" || true
    sleep 1
fi

# --- Install ---

info "Installing to ${INSTALL_DIR}/${APP_NAME}.app ..."
rm -rf "${INSTALL_DIR}/${APP_NAME}.app"
cp -R "$BUILD_OUTPUT" "${INSTALL_DIR}/${APP_NAME}.app"
ok "Installed."
echo ""

# --- Reset TCC permissions ---

info "Resetting macOS permissions (will re-prompt on next launch)..."
tccutil reset Microphone   "$BUNDLE_ID" 2>/dev/null || true
tccutil reset Accessibility "$BUNDLE_ID" 2>/dev/null || true
tccutil reset ListenEvent   "$BUNDLE_ID" 2>/dev/null || true
ok "Permissions reset."
echo ""

# --- Done ---

ok "${APP_NAME} has been built and installed to ${INSTALL_DIR}/${APP_NAME}.app"
info "Launch it from /Applications or Spotlight. macOS will prompt for Microphone, Accessibility, and Input Monitoring permissions on first use."
