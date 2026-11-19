# NextGEN Handler GUI (Tauri) — Handoff / Windows Port Guide

GUI configurator/companion for **NextGEN Handler** (`nextgen://` scheme handler + global hotkey listener). Built with **Tauri v2 (Rust backend) + React + TypeScript + Vite**. Styled to match **NextWEB**’s dark UI.

> **Current state:** Linux-first MVP implemented and checked on CachyOS (Arch). Frontend builds (`npm run build`) and Rust compiles (`cargo check`). No Windows build yet and core OS integrations are Linux-specific (see **Windows port checklist** below).

---

## What this app does (current features)

- **Setup / Registration (built-in, `install.sh`-equivalent):**
  - Shows status: `nextgen://` scheme registered (`xdg-mime`), handler desktop entry, handler binary, config dir/files.
  - **Run Install / Repair** (idempotent) performs: copy/install handler to `~/.local/bin/nextgen-handler` when source found, write `~/.local/share/applications/nextgen-handler.desktop` (`MimeType=x-scheme-handler/nextgen;`, `Exec=.../nextgen-handler %u`), `xdg-mime default nextgen-handler.desktop x-scheme-handler/nextgen`, `update-desktop-database`, and ensure config templates in `~/.config/nextgen/` (never overwrites existing files).
- **Listener (global hotkeys, `nextgen-handler --listen`):**
  - Shows running state/PIDs, **Start now / Stop**.
  - **Start listener at boot** toggle manages XDG autostart: `~/.config/autostart/nextgen-handler-listen.desktop` (Exec = preferred handler `--listen`).
- **Capture keys:**
  - **Start capture** stops the listener (evdev devices can’t be shared), spawns `--capture`, streams stdout/stderr lines into the UI (polled), **Stop capture** kills capture and restores the listener if it was running/autostart-enabled.
  - UI hints that key names go into `keys.json` bindings.
- **Apps (`~/.config/nextgen/apps.json`):**
  - Lists existing app entries (id → command argv array), **Add/Edit/Remove**, saves while preserving non-command metadata keys in `apps.json`.
  - **Add app form:** App id, Command/executable, **Arguments (one per line)**, live preview, shows resulting `nextgen://app/<id>` for NextWEB Apps Menu.
  - **Browse installed apps…** (app picker): scans installed `.desktop` applications and lets you pick one to fill id/command/args/path.
  - **Import from .desktop:** parse a given `.desktop` path and prefill.
- **Stream Player (`~/.config/nextgen/settings.json`):**
  - Detects available players (PATH + Flatpak probes) and shows presets (✓ detected): VLC, mpv, Celluloid, Haruna, SMPlayer, Clapper, Totem, Flatpak VLC/mpv/Celluloid with sensible `titleArg` defaults.
  - Custom command/args + `titleArg` (`{t}` placeholder), preview, **Save** merges into `settings.json` preserving other keys.

**Not in scope / gaps (by design for MVP):**
- No editor for `keys.json` bindings/remaps (capture shows names; edit `keys.json` manually or extend UI).
- No bridge to NextWEB’s browser-stored Apps Menu (`localStorage`) — this manages handler-local config only (`apps.json`/`nextgen://app/<id>` mapping).
- Linux-only OS integrations (see below).

---

## Project layout

- `src/` — React/TypeScript UI
  - `App.tsx` — main UI/state, invokes Tauri commands
  - `App.css` — NextWEB-matched styling
  - `main.tsx`, assets
- `src-tauri/` — Rust backend (Tauri v2)
  - `src/lib.rs` — Tauri commands + system logic (status, install/register, config read/write, autostart, listener/capture process mgmt, .desktop parsing/app discovery, player detection)
  - `src/main.rs` — entry
  - `tauri.conf.json` — app config (`productName`/identifier/window/build), `bundle.targets: "all"`
  - `capabilities/default.json` — `core:default`, `opener:default`
  - `Cargo.toml` — deps: `tauri`, `tauri-plugin-opener`, `serde`, `serde_json`, `dirs`
- `package.json` — React/Vite/Tauri CLI scripts
- Sibling handler (source of truth for protocol/listen/capture behavior): `../nextgen-handler.py`, `../install.sh`, `../README.md`

## Tauri commands (Rust API surface)

- Status/setup: `get_status`, `run_install` (alias behavior with `register_scheme`), `register_scheme`, `ensure_configs`
- Config: `get_apps`/`save_apps(apps, preserveOther?)`, `get_settings`/`save_settings(settings)`, `get_keys`/`save_keys(keys)`
- Autostart (listen): `get_autostart`, `set_autostart(enabled)`
- Listener: `stop_listener`, `start_listener`
- Capture: `start_capture`, `stop_capture`, `read_capture(clear?)`, `capture_running`
- Discovery/helpers: `parse_desktop_exec(path)`, `list_installed_apps`, `detect_players`

> `save_apps` replaces array-valued entries with provided map and (by default) preserves other keys; `save_settings` writes the provided object (frontend merges to preserve keys).

## Config paths & defaults (Linux, current)

- Config dir: `$XDG_CONFIG_HOME/nextgen` or `~/.config/nextgen`
  - `settings.json` — `{"player":["vlc"], "titleArg":"--meta-title={t}"}` (defaults)
  - `apps.json` — id → argv array (direct exec, no shell). **Default template (seeded when missing):** `_comment`, `bitchord` (`/home/tuckerberg/.local/lib/bitchord/bitchord`), `luna` (Vivaldi app-id), `steam` (`["steam","-tenfoot"]`), `xbox` (Vivaldi app-id) — **no `nuclear`**. (Matches user config snapshot; adjust paths/entries as needed per machine.)
  - `keys.json` — default bindings: Ctrl+Super combos, `BTN_SELECT+BTN_START`, `KEY_HOMEPAGE` → `focus_nextweb`, plus `"remap": []`.
- Install/register targets (Linux): `~/.local/bin/nextgen-handler`, `~/.local/share/applications/nextgen-handler.desktop`, `xdg-mime default ... x-scheme-handler/nextgen`.
- Autostart (Linux): `$XDG_CONFIG_HOME/autostart/nextgen-handler-listen.desktop` or `~/.config/autostart/...` with `Exec=<preferred handler> --listen`.
- Handler resolution prefers installed `~/.local/bin/nextgen-handler`, falls back to `../nextgen-handler.py` (workspace source) when locating/running.

## How to run/build (Linux)

Prereqs: Node/npm, Rust toolchain (`cargo` on PATH), Tauri system deps (WebKitGTK 4.1 etc., e.g. Arch/CachyOS: `webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg`).

```fish
cd NextGEN-Handler/nextgen-handler-gui
npm install
npm run tauri dev      # dev (compiles Rust on first run)
npm run build          # frontend only
npm run tauri build    # release binary + bundles
# bundles: src-tauri/target/release/bundle/ (appimage/deb/rpm as available)
# binary:  src-tauri/target/release/nextgen-handler-gui
# options: npm run tauri build -- --bundles appimage|deb|rpm  |  --no-bundle
```

---

## Windows build prerequisites (native, recommended)

- Windows 10/11 with **WebView2 Runtime** (usually present).
- **Node.js/npm**.
- **Rust** via rustup with MSVC target (default on Windows): `x86_64-pc-windows-msvc`.
- **Visual Studio Build Tools** (MSVC C++ toolchain) — required for Rust/Tauri on Windows.
- Then from this folder: `npm install` && `npm run tauri build` → `.exe` and bundles (NSIS/MSI per config) under `src-tauri/target/release/` and `src-tauri/target/release/bundle/`.

> Cross-compiling from Linux is non-trivial (mingw-w64/targets/tooling, bundling differences) and not set up in the original dev environment; native Windows or Windows CI/VM is the reliable path.

## Windows port checklist (what must be adapted)

The UI compiles cross-platform in principle, but **core integrations are Linux-specific**. To make it functional on Windows, implement per-platform behavior (e.g. `#[cfg(target_os = "linux")]` / `#[cfg(windows)]` in Rust, or abstracted helpers):

1. **Config & paths**
   - Linux: `$XDG_CONFIG_HOME/nextgen` / `~/.config/nextgen`. Windows: e.g. `%APPDATA%\nextgen` (or chosen equivalent). Update `config_dir()` and any displayed paths.
   - App/handler discovery/locations differ (`~/.local/bin`, `/usr/share/applications` not applicable).

2. **`nextgen://` scheme registration & handler install**
   - Linux: `.desktop` + `xdg-mime default` + `update-desktop-database`, copies `nextgen-handler.py` to `~/.local/bin`.
   - Windows: register URL protocol in registry (per-user `HKCU\Software\Classes\nextgen`) with command to handler executable/script; no xdg-mime/.desktop. Decide handler form on Windows (native exe, Python script invocation, packaged helper) and install location.
   - `get_status`/`run_install` must check and perform Windows registration.

3. **Autostart for listener (`--listen`)**
   - Linux: `~/.config/autostart/*.desktop` (XDG). 
   - Windows: per-user Run key (`HKCU\...\Run`), Startup folder shortcut, or Scheduled Task. `get_autostart`/`set_autostart` must target Windows mechanism.

4. **Process management (listener/capture) & handler invocation**
   - Linux: scans `/proc` cmdlines for `nextgen-handler(.py) --listen`, kills via `kill`; spawns `--listen/--capture`; prefers installed bin or `../nextgen-handler.py`.
   - Windows: enumerate processes/command lines via Windows APIs/tooling (no `/proc`), terminate appropriately, spawn handler correctly (python.exe + script, or exe), handle console/window behavior.
   - `start_capture`/`read_capture` streaming logic is generic once the process runs, but spawning/capture source must work on Windows.

5. **Capture/listen functionality (depends on handler implementation)**
   - Current handler `--listen/--capture` uses **evdev/uinput** and Linux-specific focus/actions (KWin DBus, Vivaldi PWA paths) — **not portable**. Windows needs an equivalent (e.g. global hotkey registration/low-level hooks, window focus via Win32/UI Automation, appropriate key naming/capture UX) and likely a Windows-native handler/daemon.
   - GUI capture flow assumes `--capture` prints key names to stdout; adapt to Windows capture mechanism.

6. **App discovery / `.desktop` parsing (app picker)**
   - Linux: scans `/usr/share/applications`, `~/.local/share/applications`, flatpak exports, `~/Desktop` for `.desktop` files and parses `Exec/Name/Icon`.
   - Windows: discover installed apps via Start Menu shortcuts (`.lnk`), registry, or known app locations; Vivaldi/Chrome PWAs have different artifacts (shortcuts with `--app-id` args, profile paths). `list_installed_apps`/parsing needs a Windows implementation.

7. **Stream player detection**
   - Linux: checks PATH executables and `flatpak info`. Windows: check common install paths/registry/PATH for VLC/mpv/etc., and adjust presets/`titleArg`/command forms as needed.

8. **Protocol/URL handling semantics**
   - `nextgen://play` and `nextgen://app/<id>` behavior lives in the handler; ensure a Windows handler exists and is registered so the GUI’s `nextgen://` status/install and NextWEB integration work end-to-end.

9. **General**
   - Path separators, permissions (`chmod`), and Unix-only assumptions in Rust (`/proc`, XDG envs). Gate with `cfg` and provide Windows equivalents.
   - Test: install/register status, Run Install, autostart toggle, start/stop listener, capture flow, apps.json load/save, app picker, player detection/save, and actual `nextgen://` launches.

## Handler (protocol/listen) notes

- `../nextgen-handler.py` + `../install.sh` are the Linux implementation (Python, evdev/KWin). Windows port likely requires a native/helper implementation for scheme handling and global hotkeys (see handler README “Windows (future)”).
- Security model (preserve): `play` only http(s) to configured player; `app` only whitelisted ids from local `apps.json`; key bindings only local actions.

## UI/UX notes

- NextWEB-matched dark theme (tokens/colors/components in `src/App.css`). Keep visual consistency when extending.
- "Add app" supports command + **arguments (one per line)**; app picker fills from installed apps/.desktop parsing.

---

*Last updated on Linux (CachyOS) after implementing: install/registration, autostart, listener/capture flow, apps.json editor + app picker + .desktop import, stream player detection/settings, NextWEB-style UI, and default config templates (apps minus nuclear; keys as current).* 
