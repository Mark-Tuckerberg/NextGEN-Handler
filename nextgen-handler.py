#!/usr/bin/env python3
"""nextgen:// URL-scheme handler and global-hotkey listener.

URL mode (one-shot, launched by the desktop on nextgen:// links):
  nextgen://play?url=<http(s) stream url>&title=<optional name>
      Launches the configured local player (VLC by default) with the stream.
  nextgen://app/<id>
      Looks up <id> in the local apps.json and launches that command.

Daemon mode (persistent, for global hotkeys / remote-control keys):
  nextgen-handler --listen
      Watches for configured key combos and runs the mapped action
      (default: focus/raise the NextWEB window).
  nextgen-handler --capture
      Prints key names as you press them so you can configure keys.json.

Security model:
  - play() only accepts http/https URLs, handed only to the configured player.
  - app ids are a strict whitelist from the local apps.json.
  - Key bindings trigger only local actions defined in keys.json (no commands
    from remote pages or input payloads).

Environment:
  NEXTGEN_DRY_RUN=1  print commands instead of launching/executing actions.

Config:
  Linux:   ~/.config/nextgen/ (or $XDG_CONFIG_HOME/nextgen)
  Windows: %APPDATA%\\nextgen
  settings.json  { "player": ["vlc"], "titleArg": "--meta-title={t}" }
  apps.json      { "steam": ["steam", "-tenfoot"] }
  keys.json      { "bindings": [ { "keys": ["KEY_LEFTCTRL","KEY_LEFTMETA"],
                   "action": "focus_nextweb" } ] }
"""

import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.parse

IS_WINDOWS = sys.platform == 'win32'
IS_LINUX = sys.platform.startswith('linux')

if IS_LINUX:
    import select


def default_config_dir():
    if IS_WINDOWS:
        base = os.environ.get('APPDATA') or os.path.expanduser('~')
        return os.path.join(base, 'nextgen')
    base = os.environ.get('XDG_CONFIG_HOME') or os.path.join(os.path.expanduser('~'), '.config')
    return os.path.join(base, 'nextgen')


CONFIG_DIR = os.environ.get('NEXTGEN_CONFIG_DIR') or default_config_dir()
DRY_RUN = os.environ.get('NEXTGEN_DRY_RUN') == '1'


def load_config(name, default):
    try:
        path = os.path.join(CONFIG_DIR, name)
        with open(path, 'r', encoding='utf-8') as f:
            data = json.load(f)
        return data if isinstance(data, (dict, list)) else default
    except Exception:
        return default


def notify(message):
    try:
        print('NextGEN: ' + str(message), file=sys.stderr)
    except Exception:
        pass
    if IS_WINDOWS:
        return
    try:
        subprocess.Popen(
            ['notify-send', '-a', 'NextGEN', 'NextGEN', str(message)],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
    except Exception:
        pass


def launch(cmd):
    if DRY_RUN:
        print('DRY-RUN: ' + ' '.join(cmd))
        return
    try:
        if IS_WINDOWS:
            DETACHED_PROCESS = 0x00000008
            CREATE_NEW_PROCESS_GROUP = 0x00000200
            subprocess.Popen(
                cmd,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                stdin=subprocess.DEVNULL,
                creationflags=DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP,
                close_fds=True,
            )
        else:
            subprocess.Popen(
                cmd,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                stdin=subprocess.DEVNULL, start_new_session=True,
            )
    except Exception as e:
        notify('Failed to launch: ' + str(e))


# ── URL actions ─────────────────────────────────────────────────────────

def play_action(params):
    stream = (params.get('url') or [''])[0]
    title = (params.get('title') or [''])[0]
    if not stream:
        notify('play request missing a URL')
        return
    parsed = urllib.parse.urlsplit(stream)
    if parsed.scheme not in ('http', 'https') or not parsed.netloc:
        notify('play request rejected: only http(s) streams are allowed')
        return
    settings = load_config('settings.json', {})
    player = settings.get('player')
    if not (isinstance(player, list) and player and all(isinstance(p, str) and p for p in player)):
        player = ['vlc']
    cmd = list(player) + [stream]
    title_arg = settings.get('titleArg')
    if title and isinstance(title_arg, str) and '{t}' in title_arg:
        cmd.append(title_arg.replace('{t}', title))
    launch(cmd)


def app_action(app_id):
    if not app_id:
        notify('app request missing an id')
        return
    apps = load_config('apps.json', {})
    cmd = apps.get(app_id)
    if not (isinstance(cmd, list) and cmd and all(isinstance(c, str) and c for c in cmd)):
        notify('unknown app id: ' + app_id)
        return
    launch(cmd)


def handle_url(url):
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme != 'nextgen':
        return 0
    action = (parsed.netloc or '').lower()
    rest = parsed.path.lstrip('/')
    if not action:
        action = rest.split('/')[0].lower() if rest else ''
        rest = rest[len(action):].lstrip('/')
    params = urllib.parse.parse_qs(parsed.query)
    if action == 'play':
        play_action(params)
    elif action == 'app':
        app_action(rest.split('/')[0])
    else:
        notify('unknown nextgen action: ' + (action or '(empty)'))
    return 0


# ── Focus / actions (daemon) ────────────────────────────────────────────

# Match NextWEB by caption or by Vivaldi PWA app-id fragment in resourceName.
_NEXTWEB_MATCH = (
    'cap.indexOf("nextweb") !== -1 || '
    'rc.indexOf("nextweb") !== -1 || rn.indexOf("nextweb") !== -1 || '
    'rn.indexOf("idhlfildfabddhbaajgnmpombjkniibo") !== -1'
)

# Installed NextWEB PWA (Vivaldi, Linux defaults) — used to activate/navigate
# the existing app window where supported.
_NEXTWEB_PWA = {
    'bin': '/opt/vivaldi/vivaldi',
    'profile': 'Default',
    'app_id': 'idhlfildfabddhbaajgnmpombjkniibo',
    'base_url': 'https://nextweb.ondemand.stream',
}


def _focus_nextweb_windows():
    try:
        import ctypes
        from ctypes import wintypes
        user32 = ctypes.windll.user32
        WNDENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        found = []

        def cb(hwnd, lparam):
            try:
                if not user32.IsWindowVisible(hwnd):
                    return True
                length = user32.GetWindowTextLengthW(hwnd)
                if length <= 0:
                    return True
                buf = ctypes.create_unicode_buffer(length + 1)
                user32.GetWindowTextW(hwnd, buf, length + 1)
                title = (buf.value or '').lower()
                if 'nextweb' in title:
                    found.append(hwnd)
                    return False
            except Exception:
                pass
            return True

        user32.EnumWindows(WNDENUMPROC(cb), 0)
        if not found:
            return False
        hwnd = found[0]
        try:
            if user32.IsIconic(hwnd):
                user32.ShowWindow(hwnd, 9)  # SW_RESTORE
            user32.SetForegroundWindow(hwnd)
        except Exception:
            pass
        return True
    except Exception:
        return False


def focus_nextweb():
    if DRY_RUN:
        print('DRY-RUN: focus_nextweb')
        return
    if IS_WINDOWS:
        if not _focus_nextweb_windows():
            notify('NextWEB window not found')
        return

    script = (
        'var clients = workspace.windowList ? workspace.windowList() : workspace.clientList();'
        'var target = null;'
        'for (var i = 0; i < clients.length; i++) {'
        '  var c = clients[i];'
        '  var cap = (c.caption || "").toLowerCase();'
        '  var rc = (c.resourceClass || "").toLowerCase();'
        '  var rn = (c.resourceName || "").toLowerCase();'
        '  if (' + _NEXTWEB_MATCH + ') { target = c; break; }'
        '}'
        'if (target) {'
        '  if (workspace.activeWindow !== undefined) workspace.activeWindow = target;'
        '  else if (typeof target.activate === "function") target.activate();'
        '  print("focused:" + target.caption);'
        '} else { print("notfound"); }'
    )
    path = None
    try:
        with tempfile.NamedTemporaryFile('w', suffix='.js', delete=False, encoding='utf-8') as f:
            f.write(script)
            path = f.name
        name = 'nextgenfocus'
        # Unload previous instance so reloading picks up the script; ignore errors.
        subprocess.run(['qdbus6', 'org.kde.KWin', '/Scripting',
                        'org.kde.kwin.Scripting.unloadScript', name],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        subprocess.run(['qdbus6', 'org.kde.KWin', '/Scripting',
                        'org.kde.kwin.Scripting.loadScript', path, name],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        subprocess.run(['qdbus6', 'org.kde.KWin', '/Scripting',
                        'org.kde.kwin.Scripting.start'],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except Exception:
        pass
    finally:
        if path:
            try:
                os.unlink(path)
            except Exception:
                pass


def _find_windows_browser():
    candidates = []
    for env in ('PROGRAMFILES', 'PROGRAMFILES(X86)', 'LOCALAPPDATA'):
        base = os.environ.get(env)
        if not base:
            continue
        candidates.extend([
            os.path.join(base, 'Vivaldi', 'Application', 'vivaldi.exe'),
            os.path.join(base, 'Google', 'Chrome', 'Application', 'chrome.exe'),
            os.path.join(base, 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
            os.path.join(base, 'BraveSoftware', 'Brave-Browser', 'Application', 'brave.exe'),
        ])
    for p in candidates:
        if os.path.isfile(p):
            return p
    return None


def open_nextweb_path(route):
    """Activate/navigate NextWEB to a route (best-effort per platform)."""
    url = _NEXTWEB_PWA['base_url'].rstrip('/') + '/' + str(route or '').lstrip('/')
    if DRY_RUN:
        print('DRY-RUN: open_nextweb_path ' + url)
        return
    if IS_WINDOWS:
        # Best-effort: open/focus via default browser (PWA activation varies).
        try:
            os.startfile(url)  # type: ignore[attr-defined]
        except Exception:
            browser = _find_windows_browser()
            if browser:
                try:
                    subprocess.Popen([browser, url], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                     stdin=subprocess.DEVNULL)
                except Exception:
                    pass
        _focus_nextweb_windows()
        return

    bin_path = _NEXTWEB_PWA['bin']
    cmd = [bin_path, '--profile-directory=' + _NEXTWEB_PWA['profile'],
           '--app-id=' + _NEXTWEB_PWA['app_id'], url]
    try:
        subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         stdin=subprocess.DEVNULL, start_new_session=True)
    except Exception:
        pass
    # Also drive KWin so the window is raised on Wayland.
    focus_nextweb()


def run_action(action):
    # NOTE: do not navigate the PWA from outside by launching with a URL arg
    # in ways that may open extra windows. Global actions are for bring-to-front.
    if action == 'focus_nextweb':
        focus_nextweb()
    elif action == 'open_nextweb_guide':
        # Supported on platforms where navigation is implemented.
        open_nextweb_path('/guide')
    elif action == 'open_nextweb_home':
        open_nextweb_path('/')
    else:
        notify('unknown key action: ' + str(action))


# ── Key listener daemon ─────────────────────────────────────────────────

def load_bindings():
    cfg = load_config('keys.json', {})
    if isinstance(cfg, list):
        bindings = cfg
    elif isinstance(cfg, dict):
        bindings = cfg.get('bindings', [])
    else:
        bindings = []
    out = []
    for b in bindings:
        if not isinstance(b, dict):
            continue
        keys = b.get('keys')
        action = b.get('action')
        if isinstance(keys, list) and keys and isinstance(action, str):
            out.append((frozenset(str(k) for k in keys), action))
    # Default: Ctrl + Super/Win focuses NextWEB if no config present.
    if not out and not cfg:
        out.append((frozenset(('KEY_LEFTCTRL', 'KEY_LEFTMETA')), 'focus_nextweb'))
        out.append((frozenset(('KEY_RIGHTCTRL', 'KEY_LEFTMETA')), 'focus_nextweb'))
        out.append((frozenset(('KEY_LEFTCTRL', 'KEY_RIGHTMETA')), 'focus_nextweb'))
        out.append((frozenset(('KEY_RIGHTCTRL', 'KEY_RIGHTMETA')), 'focus_nextweb'))
    return out


def load_remaps():
    """Device-level key remaps (Linux/evdev). On Windows, remaps are not applied."""
    cfg = load_config('keys.json', {})
    if not isinstance(cfg, dict):
        return []
    remaps = cfg.get('remap') or cfg.get('remaps') or []
    out = []
    for r in remaps:
        if not isinstance(r, dict):
            continue
        dn = r.get('device_name')
        frm = r.get('from')
        to = r.get('to')
        if isinstance(dn, str) and isinstance(frm, str) and isinstance(to, str):
            out.append((dn, frm, to))
    return out


# --- Linux (evdev) implementation ---

def _linux_key_name(evdev_mod, code):
    try:
        name = evdev_mod.ecodes.KEY.get(code) or evdev_mod.ecodes.BTN.get(code) or code
        if isinstance(name, (list, tuple)):
            name = name[0]
        return str(name)
    except Exception:
        return str(code)


def _linux_open_input_devices(evdev_mod):
    devs = {}
    try:
        paths = evdev_mod.list_devices()
    except Exception:
        paths = []
    for path in paths:
        try:
            d = evdev_mod.InputDevice(path)
            caps = d.capabilities()
            # EV_KEY present => keyboard/remote/button device worth watching.
            if evdev_mod.ecodes.EV_KEY in caps:
                devs[d.fd] = d
            else:
                d.close()
        except Exception:
            pass
    return devs


def _linux_code_for_key(evdev_mod, name):
    try:
        if hasattr(evdev_mod.ecodes, name):
            return getattr(evdev_mod.ecodes, name)
        return evdev_mod.ecodes.ecodes.get(name)
    except Exception:
        return None


def _linux_setup_remap_devices(evdev_mod, remaps):
    """Grab matching devices and create uinput forwarders with key remaps."""
    remapped = {}
    owned_paths = set()
    if not remaps:
        return remapped, owned_paths
    by_name = {}
    for dn, frm, to in remaps:
        by_name.setdefault(dn, []).append((frm, to))
    try:
        paths = evdev_mod.list_devices()
    except Exception:
        paths = []
    for path in paths:
        try:
            d = evdev_mod.InputDevice(path)
        except Exception:
            continue
        rules = by_name.get(d.name)
        if not rules:
            d.close()
            continue
        code_map = {}
        ok = True
        for frm, to in rules:
            fc = _linux_code_for_key(evdev_mod, frm)
            tc = _linux_code_for_key(evdev_mod, to)
            if fc is None or tc is None:
                ok = False
            else:
                code_map[fc] = tc
        if not ok or not code_map:
            d.close()
            continue
        try:
            d.grab()
            ui = evdev_mod.UInput.from_device(d, name=d.name + ' (NextGEN)')
            remapped[d.fd] = {'dev': d, 'ui': ui, 'map': code_map, 'path': path}
            owned_paths.add(path)
        except Exception:
            try:
                d.ungrab()
            except Exception:
                pass
            d.close()
    return remapped, owned_paths


def _listen_keys_linux():
    try:
        import evdev
    except Exception:
        print('nextgen-handler --listen requires python-evdev (pip install evdev or your distro package).', file=sys.stderr)
        return 1

    bindings = load_bindings()
    remaps = load_remaps()
    if not bindings and not remaps:
        print('No key bindings/remaps configured in keys.json.', file=sys.stderr)
        return 1

    remapped, owned_paths = _linux_setup_remap_devices(evdev, remaps)
    devs = _linux_open_input_devices(evdev)
    for fd, d in list(devs.items()):
        try:
            if d.path in owned_paths:
                d.close()
                devs.pop(fd, None)
        except Exception:
            pass

    if not devs and not remapped:
        print('No readable input devices found. Are you in the "input" group?', file=sys.stderr)
        return 1

    pressed = set()
    fired = set()
    last_scan = time.time()
    total = len(devs) + len(remapped)
    print(f'Listening on {total} input device(s) ({len(remapped)} remapped) for {len(bindings)} binding(s). Ctrl+C to stop.')
    try:
        while True:
            if time.time() - last_scan > 10:
                new = _linux_open_input_devices(evdev)
                for fd, d in list(new.items()):
                    try:
                        if d.path in owned_paths:
                            d.close()
                            new.pop(fd, None)
                    except Exception:
                        pass
                for fd, d in list(devs.items()):
                    if fd not in new:
                        try:
                            d.close()
                        except Exception:
                            pass
                        devs.pop(fd, None)
                for fd, d in new.items():
                    if fd not in devs:
                        devs[fd] = d
                cur_paths = set()
                for r in remapped.values():
                    try:
                        cur_paths.add(r['path'])
                    except Exception:
                        pass
                missing = [p for p in owned_paths if p not in cur_paths]
                if missing:
                    r2, p2 = _linux_setup_remap_devices(evdev, remaps)
                    for fd, info in r2.items():
                        if fd not in remapped:
                            remapped[fd] = info
                            owned_paths.update(p2)
                last_scan = time.time()

            fds = list(devs.keys()) + list(remapped.keys())
            if not fds:
                time.sleep(1)
                continue

            rlist, _, _ = select.select(fds, [], [], 1.0)
            for fd in rlist:
                if fd in remapped:
                    info = remapped[fd]
                    dev = info['dev']
                    ui = info['ui']
                    cmap = info['map']
                    try:
                        for ev in dev.read():
                            orig_code = ev.code
                            out_code = ev.code
                            if ev.type == evdev.ecodes.EV_KEY and orig_code in cmap:
                                out_code = cmap[orig_code]
                            try:
                                ui.write(ev.type, out_code, ev.value)
                                ui.syn()
                            except Exception:
                                pass
                            if ev.type == evdev.ecodes.EV_KEY:
                                name = _linux_key_name(evdev, orig_code)
                                if ev.value == 1:
                                    pressed.add(name)
                                elif ev.value == 0:
                                    pressed.discard(name)
                                for i, (combo, action) in enumerate(bindings):
                                    if combo.issubset(pressed):
                                        if i not in fired and ev.value in (1, 2):
                                            fired.add(i)
                                            run_action(action)
                                    else:
                                        fired.discard(i)
                    except (OSError, IOError):
                        try:
                            dev.ungrab()
                        except Exception:
                            pass
                        try:
                            dev.close()
                        except Exception:
                            pass
                        try:
                            ui.close()
                        except Exception:
                            pass
                        remapped.pop(fd, None)
                    continue

                dev = devs.get(fd)
                if not dev:
                    continue
                try:
                    for ev in dev.read():
                        if ev.type != evdev.ecodes.EV_KEY:
                            continue
                        name = _linux_key_name(evdev, ev.code)
                        if ev.value == 1:
                            pressed.add(name)
                        elif ev.value == 0:
                            pressed.discard(name)
                        for i, (combo, action) in enumerate(bindings):
                            if combo.issubset(pressed):
                                if i not in fired and ev.value in (1, 2):
                                    fired.add(i)
                                    run_action(action)
                            else:
                                fired.discard(i)
                except (OSError, IOError):
                    try:
                        dev.close()
                    except Exception:
                        pass
                    devs.pop(fd, None)
    except KeyboardInterrupt:
        pass
    finally:
        for d in devs.values():
            try:
                d.close()
            except Exception:
                pass
        for info in remapped.values():
            try:
                info['dev'].ungrab()
            except Exception:
                pass
            try:
                info['dev'].close()
            except Exception:
                pass
            try:
                info['ui'].close()
            except Exception:
                pass
    return 0


def _capture_keys_linux():
    try:
        import evdev
    except Exception:
        print('nextgen-handler --capture requires python-evdev.', file=sys.stderr)
        return 1
    devs = _linux_open_input_devices(evdev)
    if not devs:
        print('No readable input devices found. Are you in the "input" group?', file=sys.stderr)
        return 1
    print('Press keys/buttons to see their names (Ctrl+C to stop). Use these in keys.json "keys".')
    try:
        while True:
            r, _, _ = select.select(list(devs.keys()), [], [], 1.0)
            for fd in r:
                dev = devs.get(fd)
                if not dev:
                    continue
                try:
                    for ev in dev.read():
                        if ev.type != evdev.ecodes.EV_KEY or ev.value != 1:
                            continue
                        name = _linux_key_name(evdev, ev.code)
                        print(f'  {name}  (device: {dev.name})')
                except (OSError, IOError):
                    try:
                        dev.close()
                    except Exception:
                        pass
                    devs.pop(fd, None)
    except KeyboardInterrupt:
        pass
    finally:
        for d in devs.values():
            try:
                d.close()
            except Exception:
                pass
    return 0


# --- Windows implementation (global keyboard hook) ---

_WIN_VK_NAMES = {
    0x08: 'KEY_BACKSPACE', 0x09: 'KEY_TAB', 0x0D: 'KEY_ENTER', 0x1B: 'KEY_ESC',
    0x20: 'KEY_SPACE', 0x21: 'KEY_PAGEUP', 0x22: 'KEY_PAGEDOWN', 0x23: 'KEY_END',
    0x24: 'KEY_HOME', 0x25: 'KEY_LEFT', 0x26: 'KEY_UP', 0x27: 'KEY_RIGHT', 0x28: 'KEY_DOWN',
    0x2D: 'KEY_INSERT', 0x2E: 'KEY_DELETE',
    0x5B: 'KEY_LEFTMETA', 0x5C: 'KEY_RIGHTMETA', 0x5D: 'KEY_MENU',
    0x70: 'KEY_F1', 0x71: 'KEY_F2', 0x72: 'KEY_F3', 0x73: 'KEY_F4',
    0x74: 'KEY_F5', 0x75: 'KEY_F6', 0x76: 'KEY_F7', 0x77: 'KEY_F8',
    0x78: 'KEY_F9', 0x79: 'KEY_F10', 0x7A: 'KEY_F11', 0x7B: 'KEY_F12',
    0x90: 'KEY_NUMLOCK', 0x91: 'KEY_SCROLLLOCK',
    0xA0: 'KEY_LEFTSHIFT', 0xA1: 'KEY_RIGHTSHIFT',
    0xA2: 'KEY_LEFTCTRL', 0xA3: 'KEY_RIGHTCTRL',
    0xA4: 'KEY_LEFTALT', 0xA5: 'KEY_RIGHTALT',
    0xAC: 'KEY_HOMEPAGE', 0xAD: 'KEY_MUTE', 0xAE: 'KEY_VOLUMEDOWN', 0xAF: 'KEY_VOLUMEUP',
    0xB0: 'KEY_NEXTSONG', 0xB1: 'KEY_PREVIOUSSONG', 0xB2: 'KEY_STOPCD', 0xB3: 'KEY_PLAYPAUSE',
    0xBA: 'KEY_SEMICOLON', 0xBB: 'KEY_EQUAL', 0xBC: 'KEY_COMMA', 0xBD: 'KEY_MINUS',
    0xBE: 'KEY_DOT', 0xBF: 'KEY_SLASH', 0xC0: 'KEY_GRAVE',
    0xDB: 'KEY_LEFTBRACE', 0xDC: 'KEY_BACKSLASH', 0xDD: 'KEY_RIGHTBRACE', 0xDE: 'KEY_APOSTROPHE',
    0x14: 'KEY_CAPSLOCK', 0x2C: 'KEY_PRINT', 0x13: 'KEY_PAUSE',
}
for _i in range(26):
    _WIN_VK_NAMES[0x41 + _i] = 'KEY_' + chr(ord('A') + _i)
for _i in range(10):
    _WIN_VK_NAMES[0x30 + _i] = 'KEY_' + str(_i)


def _win_key_name(vk, scan=0, flags=0):
    # flags bit 0x01 = extended, 0x10 = injected, 0x20 = alt down (context)
    extended = bool(flags & 0x01)
    if vk in (0x10, 0x11, 0x12):  # SHIFT/CTRL/ALT generic -> try to distinguish
        try:
            import ctypes
            user32 = ctypes.windll.user32
            if vk == 0x10:
                if scan == 0x36:
                    return 'KEY_RIGHTSHIFT'
                if scan == 0x2A:
                    return 'KEY_LEFTSHIFT'
                # fallback
                if user32.GetKeyState(0xA1) & 0x8000:
                    return 'KEY_RIGHTSHIFT'
                return 'KEY_LEFTSHIFT'
            if vk == 0x11:
                return 'KEY_RIGHTCTRL' if extended else 'KEY_LEFTCTRL'
            if vk == 0x12:
                return 'KEY_RIGHTALT' if extended else 'KEY_LEFTALT'
        except Exception:
            pass
    if vk == 0x0D and extended:
        return 'KEY_KPENTER'
    name = _WIN_VK_NAMES.get(vk)
    if name:
        return name
    return f'VK_{vk:#04x}'


def _windows_key_loop(bindings, capture_only=False):
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.windll.user32
    WH_KEYBOARD_LL = 13
    WM_KEYDOWN = 0x0100
    WM_KEYUP = 0x0101
    WM_SYSKEYDOWN = 0x0104
    WM_SYSKEYUP = 0x0105

    ULONG_PTR = wintypes.WPARAM

    class KBDLLHOOKSTRUCT(ctypes.Structure):
        _fields_ = [
            ('vkCode', wintypes.DWORD),
            ('scanCode', wintypes.DWORD),
            ('flags', wintypes.DWORD),
            ('time', wintypes.DWORD),
            ('dwExtraInfo', ULONG_PTR),
        ]

    if wintypes.WPARAM is ctypes.c_ulonglong:
        LRESULT = ctypes.c_longlong
    else:
        LRESULT = ctypes.c_long
    HOOKPROC = ctypes.WINFUNCTYPE(LRESULT, ctypes.c_int, wintypes.WPARAM, wintypes.LPARAM)

    pressed = set()
    fired = set()
    hook = None

    def callback(nCode, wParam, lParam):
        try:
            if nCode == 0:
                kb = ctypes.cast(lParam, ctypes.POINTER(KBDLLHOOKSTRUCT)).contents
                vk = int(kb.vkCode)
                name = _win_key_name(vk, int(kb.scanCode), int(kb.flags))
                if wParam in (WM_KEYDOWN, WM_SYSKEYDOWN):
                    first_press = name not in pressed
                    if capture_only and first_press:
                        print(f'  {name}  (vk={vk:#04x})')
                        sys.stdout.flush()
                    pressed.add(name)
                    for i, (combo, action) in enumerate(bindings):
                        if combo.issubset(pressed):
                            if i not in fired:
                                fired.add(i)
                                if not capture_only:
                                    try:
                                        run_action(action)
                                    except Exception:
                                        pass
                        else:
                            fired.discard(i)
                elif wParam in (WM_KEYUP, WM_SYSKEYUP):
                    pressed.discard(name)
                    for i, (combo, _a) in enumerate(bindings):
                        if not combo.issubset(pressed):
                            fired.discard(i)
        except Exception:
            pass
        return user32.CallNextHookEx(hook, nCode, wParam, lParam)

    hook_cb = HOOKPROC(callback)
    hook = user32.SetWindowsHookExW(WH_KEYBOARD_LL, hook_cb, None, 0)
    if not hook:
        err = ctypes.get_last_error()
        print(f'Failed to install keyboard hook (error {err}). Try running with sufficient privileges.', file=sys.stderr)
        return 1
    try:
        msg = wintypes.MSG()
        while True:
            ret = user32.GetMessageW(ctypes.byref(msg), None, 0, 0)
            if ret == 0 or ret == -1:
                break
            user32.TranslateMessage(ctypes.byref(msg))
            user32.DispatchMessageW(ctypes.byref(msg))
    except KeyboardInterrupt:
        pass
    finally:
        try:
            user32.UnhookWindowsHookEx(hook)
        except Exception:
            pass
    return 0


def _listen_keys_windows():
    bindings = load_bindings()
    remaps = load_remaps()
    if remaps:
        print('Note: key remaps are not supported on Windows and will be ignored.', file=sys.stderr)
    if not bindings:
        print('No key bindings configured in keys.json.', file=sys.stderr)
        return 1
    print(f'Listening for {len(bindings)} binding(s) (Windows global hook). Ctrl+C to stop.')
    sys.stdout.flush()
    return _windows_key_loop(bindings, capture_only=False)


def _capture_keys_windows():
    print('Press keys/buttons to see their names (Ctrl+C to stop). Use these in keys.json "keys".')
    sys.stdout.flush()
    return _windows_key_loop([], capture_only=True)


def listen_keys():
    if IS_WINDOWS:
        return _listen_keys_windows()
    return _listen_keys_linux()


def capture_keys():
    if IS_WINDOWS:
        return _capture_keys_windows()
    return _capture_keys_linux()


# ── Entry ───────────────────────────────────────────────────────────────

def main():
    if len(sys.argv) >= 2 and sys.argv[1] == '--listen':
        return listen_keys()
    if len(sys.argv) >= 2 and sys.argv[1] == '--capture':
        return capture_keys()
    if len(sys.argv) < 2:
        return 0
    return handle_url(sys.argv[1])


if __name__ == '__main__':
    sys.exit(main())
