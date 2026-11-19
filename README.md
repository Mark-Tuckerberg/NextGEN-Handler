# NextGEN Handler

A `nextgen://` URL-scheme handler **and** global-hotkey listener for **Linux and Windows**.
Lets NextWEB hand streams to the local video player, launch registered apps,
and (in daemon mode) bring NextWEB to the front with a hotkey.

## Features

- **URL scheme (one-shot)**
  - `nextgen://play?url=<http(s) url>&title=<optional>` — launch the configured player (default **VLC**) with the stream
  - `nextgen://app/<id>` — launch a whitelisted app/command from local `apps.json`
- **Global hotkey daemon** (`--listen`) — runs configured key bindings and triggers local actions (default: `focus_nextweb`)
- **Key capture** (`--capture`) — prints key names so you can configure `keys.json`
- **Security model**
  - `play` accepts only `http(s)` URLs and hands them only to the configured player
  - `app` executes only command lists listed in local `apps.json` (ids, never free-form URLs/commands)
  - key bindings trigger only local actions from `keys.json`

## Install

### Windows (per-user, no admin)

Requirements: **Python 3** installed and available as `py`, `python`, or `python3`.

```powershell
# From this folder
.\install.cmd
# or
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

What it does:
- Installs handler to `%LOCALAPPDATA%\NextGEN Handler\bin\`
  - `nextgen-handler.py`
  - `nextgen-handler.cmd` (launcher that finds Python)
- Registers the `nextgen://` URL protocol in `HKCU\Software\Classes\nextgen`
- Creates config templates in `%APPDATA%\nextgen\` (never overwrites existing files)

After install, clicking a `nextgen://` link in a browser/app should invoke the handler (the browser may prompt once to allow the protocol).

### Linux (per-user, no admin)

```bash
bash install.sh
```

What it does:
- Installs `~/.local/bin/nextgen-handler`
- Registers `x-scheme-handler/nextgen` via `xdg-mime` / `.desktop`
- Creates config templates in `~/.config/nextgen/` (never overwrites existing files)

Linux daemon/capture uses **evdev** (`python3-evdev`) and may require read access to `/dev/input/*` (e.g. being in the `input` group).

## Usage

### URL scheme

| Link | Action |
|------|--------|
| `nextgen://play?url=<http(s) url>&title=<optional>` | Launch configured player with the stream URL |
| `nextgen://app/<id>` | Launch app id from `apps.json` |

Examples:

```text
nextgen://play?url=https%3A%2F%2Fexample.com%2Fstream.m3u8&title=Example
nextgen://app/steam
```

### Global hotkey daemon / capture

```bash
# Capture key names for your system
nextgen-handler --capture

# Run the listener (default binding focuses NextWEB)
nextgen-handler --listen
```

- **Windows:** global key capture/listen is implemented via a low-level keyboard hook (ctypes/user32). Key names are emitted for use in `keys.json` (common keys are mapped to portable names like `KEY_LEFTCTRL`, `KEY_LEFTMETA`, `KEY_A`, etc.). Device-level key remapping from Linux `remap` entries is not applied on Windows.
- **Linux:** capture/listen uses evdev and can watch/remap supported input devices (see `keys.json` `remap`).
- **Focus action:** `focus_nextweb` is best-effort per platform (Linux: KWin scripting path in handler; Windows: finds/activates a window with "NextWEB" in the title).

To keep the listener running at login:
- **Windows:** use the GUI toggle (HKCU Run entry), Task Scheduler/Startup shortcut, or run `nextgen-handler.cmd --listen` at logon.
- **Linux:** use the GUI toggle (XDG autostart), or your desktop/session autostart/service mechanism.

## Config

Config directory:
- **Windows:** `%APPDATA%\nextgen`
- **Linux:** `$XDG_CONFIG_HOME/nextgen` or `~/.config/nextgen`

Optional override: set `NEXTGEN_CONFIG_DIR` to use a custom config directory.

### `settings.json` — stream player

```json
{
  "player": ["vlc"],
  "titleArg": "--meta-title={t}"
}
```

- `player`: command array (executable + args) invoked with the stream URL appended
- `titleArg`: optional argument template; `{t}` is replaced with the `title` query param (when provided)

Examples:
- VLC: `{"player":["vlc"], "titleArg":"--meta-title={t}"}`
- mpv: `{"player":["mpv"], "titleArg":"--force-media-title={t}"}`
- Windows (explicit path): `{"player":["C:\\\\Program Files\\\\VideoLAN\\\\VLC\\\\vlc.exe"], "titleArg":"--meta-title={t}"}`

### `apps.json` — whitelisted app ids

```json
{
  "_comment": "Add app ids here. Values are command lists executed directly (no shell).",
  "steam": ["steam", "-tenfoot"],
  "notepad": ["notepad.exe"]
}
```

- Keys are app ids used by `nextgen://app/<id>`
- Values are command arrays executed directly (no shell). Use full paths on Windows when needed.

### `keys.json` — global hotkey bindings

```json
{
  "bindings": [
    { "keys": ["KEY_LEFTCTRL", "KEY_LEFTMETA"], "action": "focus_nextweb" }
  ],
  "remap": []
}
```

- `keys`: key names from `--capture` (all keys in a binding must be held/pressed as a chord)
- `action`: local action name
- Supported actions: `focus_nextweb`, `open_nextweb_guide`, `open_nextweb_home` (navigation behavior is best-effort/platform-specific)

## GUI (NextGEN Handler GUI)

A Tauri-based GUI is included in `nextgen-handler-gui/` for:
- install/registration status and repair
- listener start/stop and start-at-boot toggle
- key capture
- `apps.json` and `settings.json` editing
- player/app discovery helpers (platform-specific)

See `nextgen-handler-gui/README.md` for development/build details.

## Testing

### Dry run (no launch / no focus action side effects)

```bash
# Linux/macOS shell style
NEXTGEN_DRY_RUN=1 nextgen-handler 'nextgen://play?url=https%3A%2F%2Fexample.com%2Fstream.m3u8&title=Test'
```

```powershell
# Windows (PowerShell)
$env:NEXTGEN_DRY_RUN='1'
& "$env:LOCALAPPDATA\NextGEN Handler\bin\nextgen-handler.cmd" "nextgen://play?url=https%3A%2F%2Fexample.com%2Fstream.m3u8&title=Test"
Remove-Item Env:NEXTGEN_DRY_RUN
```

## Notes

- Windows and Linux use different system integrations (registry vs xdg-mime/.desktop, Run key vs XDG autostart, keyboard hook vs evdev). Core `nextgen://play` and `nextgen://app/<id>` behavior and config formats are shared.
- If a browser does not open `nextgen://` links automatically, ensure the handler is installed/registered and allow the protocol prompt once.