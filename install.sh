#!/usr/bin/env bash
# Registers the nextgen:// scheme handler for the current user (Linux).
# No daemon, no admin rights: one .desktop file + xdg-mime registration.
set -euo pipefail

BIN_DIR="$HOME/.local/bin"
APP_DIR="$HOME/.local/share/applications"
CONFIG_DIR="$HOME/.config/nextgen"
SRC_DIR="$(cd "$(dirname "$0")" && pwd)"

mkdir -p "$BIN_DIR" "$APP_DIR" "$CONFIG_DIR"
install -m 755 "$SRC_DIR/nextgen-handler.py" "$BIN_DIR/nextgen-handler"

# Desktop entry: Exec carries the real install path (%u = the nextgen:// URL).
cat > "$APP_DIR/nextgen-handler.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=NextGEN Handler
Comment=Handles nextgen:// links (stream handoff to VLC, app launching)
Exec=$BIN_DIR/nextgen-handler %u
NoDisplay=true
Terminal=false
MimeType=x-scheme-handler/nextgen;
Categories=Utility;
EOF

# Register as the default handler for the scheme.
xdg-mime default nextgen-handler.desktop x-scheme-handler/nextgen
command -v update-desktop-database >/dev/null 2>&1 && \
  update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true

# First-run config templates (never overwrite existing settings).
if [ ! -f "$CONFIG_DIR/settings.json" ]; then
  cat > "$CONFIG_DIR/settings.json" <<'EOF'
{
  "player": ["vlc"],
  "titleArg": "--meta-title={t}"
}
EOF
fi
if [ ! -f "$CONFIG_DIR/apps.json" ]; then
  cat > "$CONFIG_DIR/apps.json" <<'EOF'
{
  "_comment": "Add app ids here. Values are command lists executed directly (no shell).",
  "bitchord": [
    "/home/tuckerberg/.local/lib/bitchord/bitchord"
  ],
  "luna": [
    "/opt/vivaldi/vivaldi",
    "--profile-directory=Default",
    "--app-id=agmpcdnpkedhhjldepagpgebdindblfd",
    "--start-fullscreen"
  ],
  "steam": [
    "steam",
    "-tenfoot"
  ],
  "xbox": [
    "/opt/vivaldi/vivaldi",
    "--profile-directory=Default",
    "--app-id=kanajofaghckoijdglhndcjjlemljefb",
    "--start-fullscreen"
  ]
}
EOF
fi
if [ ! -f "$CONFIG_DIR/keys.json" ]; then
  cat > "$CONFIG_DIR/keys.json" <<'EOF'
{
  "_comment": "Global hotkeys for the --listen daemon. 'keys' are evdev key names (see --capture).",
  "bindings": [
    { "keys": ["KEY_LEFTCTRL", "KEY_LEFTMETA"], "action": "focus_nextweb" },
    { "keys": ["KEY_RIGHTCTRL", "KEY_LEFTMETA"], "action": "focus_nextweb" },
    { "keys": ["KEY_LEFTCTRL", "KEY_RIGHTMETA"], "action": "focus_nextweb" },
    { "keys": ["KEY_RIGHTCTRL", "KEY_RIGHTMETA"], "action": "focus_nextweb" },
    { "keys": ["BTN_SELECT", "BTN_START"], "action": "focus_nextweb" },
    { "keys": ["KEY_HOMEPAGE"], "action": "focus_nextweb" }
  ],
  "remap": []
}
EOF
fi

chmod 600 "$CONFIG_DIR/settings.json" "$CONFIG_DIR/apps.json" "$CONFIG_DIR/keys.json" 2>/dev/null || true

echo "Installed:"
echo "  handler : $BIN_DIR/nextgen-handler"
echo "  desktop : $APP_DIR/nextgen-handler.desktop (x-scheme-handler/nextgen)"
echo "  config  : $CONFIG_DIR/{settings,apps,keys}.json (player: vlc, hotkey: Ctrl+Super)"
echo
echo "Self-test:"
echo "  NEXTGEN_DRY_RUN=1 $BIN_DIR/nextgen-handler 'nextgen://play?url=https%3A%2F%2Fexample.com%2Fstream.m3u8&title=Test'"
echo "  NEXTGEN_DRY_RUN=1 $BIN_DIR/nextgen-handler --listen   (prints focus action on Ctrl+Super, Ctrl+C to stop)"
