#!/usr/bin/env bash
# Install Veil and its core for the current user, without sudo or root.
# This does not launch Veil and does not modify any profile database.
# Invariant: this installer never moves, deletes, copies, or rewrites
# ~/.local/share/veil/veil.db (or ~/.config/veil/settings.json). A future data
# migration must be explicit opt-in, never a default install action.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
APP_SRC="${VEIL_APP_BIN:-$ROOT/src-tauri/target/release/veil}"
CORE_SRC="${VEIL_CORE_BIN:-$ROOT/vendor/sing-box-1.14.5-linux-amd64/sing-box}"
ICON_SRC="${VEIL_ICON:-$ROOT/src-tauri/icons/icon.png}"

: "${HOME:?HOME must be set}"
PREFIX="$HOME/.local/opt/veil"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
DATA_DIR="$HOME/.local/share/veil"
ICON_DIR="$DATA_HOME/icons/hicolor/512x512/apps"
APPS_DIR="$DATA_HOME/applications"
DESKTOP_FILE="$APPS_DIR/veil.desktop"

for source in "$APP_SRC" "$CORE_SRC" "$ICON_SRC"; do
    if [ ! -f "$source" ]; then
        echo "missing required file: $source" >&2
        exit 1
    fi
done
if [ ! -x "$APP_SRC" ] || [ ! -x "$CORE_SRC" ]; then
    echo "release app and core must be executable: $APP_SRC, $CORE_SRC" >&2
    exit 1
fi

if [ -e "$DATA_DIR/veil.db" ]; then
    echo "Preserving existing profile database: $DATA_DIR/veil.db"
fi

install -d -m 0755 "$PREFIX" "$ICON_DIR" "$APPS_DIR"
install -m 0755 "$APP_SRC" "$PREFIX/veil"
install -m 0755 "$CORE_SRC" "$PREFIX/sing-box"
install -m 0644 "$ICON_SRC" "$ICON_DIR/veil.png"

umask 022
TMP_DESKTOP="$DESKTOP_FILE.tmp.$$"
trap 'rm -f "$TMP_DESKTOP"' EXIT
cat > "$TMP_DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=Veil
Comment=Linux desktop proxy client powered by sing-box
Exec=$PREFIX/veil
TryExec=$PREFIX/veil
Icon=veil
Terminal=false
Categories=Network;Utility;Security;
StartupNotify=true
StartupWMClass=veil
EOF
chmod 0644 "$TMP_DESKTOP"
mv -f "$TMP_DESKTOP" "$DESKTOP_FILE"
trap - EXIT

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPS_DIR"
fi
if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$DESKTOP_FILE"
fi

echo "Installed:"
echo "  $PREFIX/veil"
echo "  $PREFIX/sing-box"
echo "  $ICON_DIR/veil.png"
echo "  $DESKTOP_FILE"
echo
echo "Launch 'Veil' from the desktop application launcher, or:"
echo "  $PREFIX/veil"
echo "Normal launches use $HOME/.local/share/veil and $HOME/.config/veil."
echo "Existing profile data is preserved; no database was moved, deleted, or copied."
echo "No subscription was copied from the isolated test data."
