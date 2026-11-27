#!/usr/bin/env bash
# Remove the user-local Veil launcher and binaries.
# Profile data, settings, and logs are intentionally left in place.
set -euo pipefail

: "${HOME:?HOME must be set}"
PREFIX="$HOME/.local/opt/veil"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
ICON_DIR="$DATA_HOME/icons/hicolor/512x512/apps"
APPS_DIR="$DATA_HOME/applications"
DESKTOP_FILE="$APPS_DIR/veil.desktop"

rm -f "$PREFIX/veil" "$PREFIX/sing-box"
rmdir "$PREFIX" 2>/dev/null || true
rm -f "$ICON_DIR/veil.png" "$DESKTOP_FILE"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPS_DIR"
fi

echo "Removed:"
echo "  $PREFIX/veil"
echo "  $PREFIX/sing-box"
echo "  $ICON_DIR/veil.png"
echo "  $DESKTOP_FILE"
echo
echo "Left in place: $HOME/.local/share/veil (profiles/logs) and $HOME/.config/veil (settings)."
