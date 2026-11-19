# NextGEN Handler – Handoff / Status (Windows port + repo setup)

_Last updated: 2026-11-19 (from Windows workspace session)_

## Current status

### Completed in this session
- **Cross-platform handler (`nextgen-handler.py`)**
  - Windows/Linux config paths: Windows `%APPDATA%\nextgen`, Linux `$XDG_CONFIG_HOME/nextgen` or `~/.config/nextgen` (`NEXTGEN_CONFIG_DIR` override supported)
  - `nextgen://play` and `nextgen://app/<id>` handling retained with cross-platform launch behavior
  - Windows global hotkey **listen/capture** implemented via low-level keyboard hook (ctypes/user32) with portable key names (e.g. `KEY_LEFTCTRL`, `KEY_LEFTMETA`, `KEY_A`)
  - Best-effort `focus_nextweb` on Windows (activates window with "NextWEB" in title); Linux evdev/KWin flow preserved
  - Notes: Linux `remap` entries in `keys.json` are **not applied on Windows** (capture/bindings supported)

- **Windows installer scripts (new)**
  - `install.ps1` (per-user, no admin) and `install.cmd`
  - Installs to `%LOCALAPPDATA%\NextGEN Handler\bin\` (`nextgen-handler.py`, `nextgen-handler.cmd` launcher)
  - Registers `nextgen://` URL protocol in `HKCU\Software\Classes\nextgen`
  - Creates config templates in `%APPDATA%\nextgen\` without overwriting existing files

- **Documentation**
  - `README.md` rewritten for **Linux + Windows** (install/usage/config/testing/GUI notes)

- **GUI (`nextgen-handler-gui`, Tauri v2 + React) ported in source for Windows**
  - `src-tauri/src/lib.rs`: cross-platform config/install/registration, autostart (Windows HKCU Run / Linux XDG autostart), listener/capture/process management, player detection, app discovery (Windows Start Menu shortcut scan best-effort), handler launch resolution
  - `src/App.tsx`: platform-aware labels/paths/hints
  - **No Windows GUI binary/installer built in this environment** (source changes only)

- **Repo prep (local)**
  - Added `NextGEN-Handler/.gitignore` (ignores `node_modules/`, `dist/`, `__pycache__/`, Tauri/Rust `target/`, logs, etc.)
  - `NextGEN-Handler` is **not** a git repo yet; `git`/`gh` were not available in this Windows environment

## Important paths

- **Handler source (this workspace):** `NextGEN-Handler/`
  - `nextgen-handler.py`, `install.sh` (Linux), `install.ps1`/`install.cmd` (Windows), `README.md`
  - GUI: `NextGEN-Handler/nextgen-handler-gui/`
- **Windows install target (after running installer):** `%LOCALAPPDATA%\NextGEN Handler\bin\`
- **Windows config:** `%APPDATA%\nextgen\` (`settings.json`, `apps.json`, `keys.json`)
- **Linux config:** `$XDG_CONFIG_HOME/nextgen` or `~/.config/nextgen`

## Not done yet / next steps

1. **Create private GitHub repo and push (planned to continue on Linux)**
   - Recommended scope: repo root = `NextGEN-Handler/` (not the whole `NextGEN Software` workspace)
   - Suggested flow (on Linux, with git + gh installed/authenticated):
     ```bash
     cd /path/to/NextGEN-Handler
     git init
     git add .
     git status   # verify node_modules/target/dist/__pycache__ are ignored
     git commit -m "Initial commit: NextGEN Handler (Linux/Windows) + GUI"
     gh auth login   # if needed
     gh repo create <repo-name> --private --source=. --remote=origin --push
     ```
   - Alternative: create empty private repo on GitHub, then `git remote add origin ...` and `git push -u origin main`

2. **(Optional but recommended) Add GitHub Actions workflow for Windows GUI builds**
   - Goal: build Tauri Windows `.exe`/installer artifacts on `windows-latest` and upload artifacts (and/or attach to releases on tags)
   - Target app: `nextgen-handler-gui/` (`npm install` + `npm run tauri build`)
   - This allows building Windows binaries without a local Windows machine

3. **Build/test validation**
   - **Windows handler:** test `install.ps1`, `nextgen://` registration, `--capture`, `--listen`, and `nextgen://play`/`nextgen://app/<id>` on a Windows machine with Python installed
     - Dry-run example: set `NEXTGEN_DRY_RUN=1` and run handler with a `nextgen://play?...` URL
   - **Windows GUI:** build on Windows or CI (`npm install && npm run tauri build` in `nextgen-handler-gui`)
     - Outputs: `src-tauri/target/release/nextgen-handler-gui.exe` and `src-tauri/target/release/bundle/...`
   - **Linux:** sanity-check no regressions in handler/listener/capture and GUI behavior

4. **Possible follow-ups (if needed)**
   - Package Python handler as standalone Windows `.exe` (e.g. PyInstaller/Nuitka) to remove Python dependency
   - Improve Windows key naming/remap/focus behavior parity where feasible
   - Refine GUI Windows app discovery (currently Start Menu shortcut scan via PowerShell, best-effort)
   - Add release automation (tag-based builds for Linux/Windows artifacts)

## Known limitations / notes
- Python runtime was not executable in this Windows environment during the session, so handler changes were implemented but **not runtime-tested here**.
- Windows hotkey support is hook-based (global bindings/capture); device-level remapping like Linux evdev/uinput is not implemented.
- `nextgen://` focus/navigation behavior is platform/browse/PWA-setup dependent (best-effort implementations).
- Cross-compiling the Tauri Windows GUI from Linux is possible in some setups but not the reliable path; native Windows or Windows CI is recommended for `.exe`/installer artifacts.
- `nextgen-handler-gui/node_modules/` and `src-tauri/target/` may exist locally; they should remain ignored by git.

## Quick resume checklist (on Linux)
- [ ] `cd` into `NextGEN-Handler`
- [ ] Confirm files look right (`README.md`, `install.ps1`, `.gitignore`, `HANDOFF.md`)
- [ ] `git init` + initial commit (verify ignored artifacts)
- [ ] Create/push **private** GitHub repo
- [ ] (Optional) Add Windows build workflow for `nextgen-handler-gui`
- [ ] Run Linux/Windows validation builds/tests as available
