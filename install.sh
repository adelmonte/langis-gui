#!/bin/bash
# Builds langis-gui and installs it for this user: the program in
# ~/.cargo/bin, the launcher entry and icons under ~/.local/share.
# Uninstall with: ./install.sh --uninstall
set -euo pipefail
cd "$(dirname "$0")"
share=${XDG_DATA_HOME:-$HOME/.local/share}
sizes="16 32 48 64 128 256 512"

if [ "${1:-}" = "--uninstall" ]; then
    cargo uninstall langis-gui || true
    rm -f "$share/applications/langis.desktop" "$share/applications/langis-gui.desktop" "$share/icons/hicolor/scalable/apps/langis.svg"
    for size in $sizes; do rm -f "$share/icons/hicolor/${size}x${size}/apps/langis.png"; done
    exit
fi

cargo install --locked --path .
install -Dm644 data/langis.desktop "$share/applications/langis.desktop"
install -Dm644 assets/app/langis.svg "$share/icons/hicolor/scalable/apps/langis.svg"
for size in $sizes; do
    install -Dm644 "assets/app/langis-$size.png" "$share/icons/hicolor/${size}x${size}/apps/langis.png"
done
command -v update-desktop-database > /dev/null && update-desktop-database "$share/applications" || true
command -v gtk-update-icon-cache > /dev/null && gtk-update-icon-cache -q "$share/icons/hicolor" || true
echo "Installed. Make sure ~/.cargo/bin is on PATH; the launcher entry runs 'langis-gui'."
