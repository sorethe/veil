#!/usr/bin/env bash
# Build and install the privileged Veil helper + polkit policy for local development.
# Requires interactive sudo. The GUI itself is never run as root.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

echo "==> Building veil-helper (release)"
cargo build --release --manifest-path src-tauri/helper/Cargo.toml

echo "==> Installing /usr/bin/veil-helper"
sudo install -Dm755 src-tauri/helper/target/release/veil-helper /usr/bin/veil-helper

echo "==> Installing polkit action dev.veil.helper"
sudo install -Dm644 src-tauri/resources/dev.veil.helper.policy \
  /usr/share/polkit-1/actions/dev.veil.helper.policy

echo "==> Installing desktop entry and icon"
sudo install -Dm644 src-tauri/resources/veil.desktop /usr/share/applications/veil.desktop
sudo install -Dm644 src-tauri/icons/icon.png /usr/share/icons/hicolor/512x512/apps/veil.png

echo "==> Checking helper"
/usr/bin/veil-helper check

cat <<'EOF'

Installed. TUN mode now uses:
  pkexec /usr/bin/veil-helper start --config ... --core ... --pid-file --log-file

Notes:
- The polkit action allows active-session users to authenticate as admin (kept
  briefly). No passwordless sudoers rule is used.
- TUN mode needs /dev/net/tun and nftables. The helper never runs a shell.
EOF
