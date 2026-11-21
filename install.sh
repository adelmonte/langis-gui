#!/bin/bash
# Builds langis-gui and installs it for this user: the program in
# ~/.cargo/bin, the launcher entry and icons under ~/.local/share.
# It also makes langis the handler for sgnl:// and signalcaptcha:// links.
# Uninstall with: ./install.sh --uninstall
set -euo pipefail
cd "$(dirname "$0")"
share=${XDG_DATA_HOME:-$HOME/.local/share}
sizes="16 32 48 64 128 256 512"
schemes="x-scheme-handler/sgnl x-scheme-handler/signalcaptcha"
mimeapps=${XDG_CONFIG_HOME:-$HOME/.config}/mimeapps.list
# What handled each scheme before, to give it back on uninstall.
previous=${XDG_CONFIG_HOME:-$HOME/.config}/langis-gui/previous-handlers

if [ "${1:-}" = "--uninstall" ]; then
    cargo uninstall langis-gui || true
    rm -f "$share/applications/langis.desktop" "$share/applications/langis-gui.desktop" "$share/icons/hicolor/scalable/apps/langis.svg"
    for size in $sizes; do rm -f "$share/icons/hicolor/${size}x${size}/apps/langis.png"; done
    # Our lines in mimeapps.list (the defaults set below), and the cache.
    [ -f "$mimeapps" ] && sed -i '/^x-scheme-handler\/\(sgnl\|signalcaptcha\)=langis\.desktop;\?$/d' "$mimeapps"
    # The handlers they had before (Signal Desktop, say), back again.
    if [ -f "$previous" ] && command -v xdg-mime > /dev/null; then
        while read -r scheme handler; do
            [ -n "$handler" ] && [ -f "$share/applications/$handler" -o -f "/usr/share/applications/$handler" ] && xdg-mime default "$handler" "$scheme"
        done < "$previous"
        rm -f "$previous"
    fi
    command -v update-desktop-database > /dev/null && update-desktop-database "$share/applications" || true
    exit
fi

cargo install --locked --path .
install -Dm644 data/langis.desktop "$share/applications/langis.desktop"
install -Dm644 assets/app/langis.svg "$share/icons/hicolor/scalable/apps/langis.svg"
for size in $sizes; do
    install -Dm644 "assets/app/langis-$size.png" "$share/icons/hicolor/${size}x${size}/apps/langis.png"
done
command -v update-desktop-database > /dev/null && update-desktop-database "$share/applications" || true
# The default for Signal's links, in place of Signal Desktop if it was.
if command -v xdg-mime > /dev/null; then
    mkdir -p "$(dirname "$previous")"
    for scheme in $schemes; do
        current=$(xdg-mime query default "$scheme" 2> /dev/null || true)
        if [ -n "$current" ] && [ "$current" != langis.desktop ]; then
            echo "${scheme#x-scheme-handler/}:// links went to $current; now langis.desktop (uninstall gives them back)"
            grep -q "^$scheme " "$previous" 2> /dev/null || echo "$scheme $current" >> "$previous"
        fi
        xdg-mime default langis.desktop "$scheme"
    done
fi
command -v gtk-update-icon-cache > /dev/null && gtk-update-icon-cache -q "$share/icons/hicolor" || true
echo "Installed. Make sure ~/.cargo/bin is on PATH; the launcher entry runs 'langis-gui'."
