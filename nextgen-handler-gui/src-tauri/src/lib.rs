use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::State;

struct CaptureInner {
    child: Option<Child>,
    lines: Vec<String>,
}

#[derive(Default)]
struct AppState {
    capture: Mutex<CaptureInner>,
}

impl Default for CaptureInner {
    fn default() -> Self {
        Self {
            child: None,
            lines: Vec::new(),
        }
    }
}

fn is_windows() -> bool {
    cfg!(windows)
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| {
        if is_windows() {
            PathBuf::from("C:\\")
        } else {
            PathBuf::from("/")
        }
    })
}

fn config_dir() -> PathBuf {
    if is_windows() {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("nextgen");
        }
        return home_dir().join("AppData").join("Roaming").join("nextgen");
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("nextgen")
}

fn apps_path() -> PathBuf {
    config_dir().join("apps.json")
}
fn keys_path() -> PathBuf {
    config_dir().join("keys.json")
}
fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

fn local_handler_dir() -> PathBuf {
    if is_windows() {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local).join("NextGEN Handler").join("bin");
        }
        return home_dir()
            .join("AppData")
            .join("Local")
            .join("NextGEN Handler")
            .join("bin");
    }
    home_dir().join(".local").join("bin")
}

fn local_handler_script() -> PathBuf {
    if is_windows() {
        local_handler_dir().join("nextgen-handler.py")
    } else {
        local_handler_dir().join("nextgen-handler")
    }
}

fn local_handler_launcher() -> Option<PathBuf> {
    if is_windows() {
        let p = local_handler_dir().join("nextgen-handler.cmd");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn user_apps_dir() -> PathBuf {
    if is_windows() {
        // Not used on Windows; return config-adjacent placeholder
        return local_handler_dir();
    }
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local/share"))
        .join("applications")
}

fn handler_desktop_path() -> PathBuf {
    user_apps_dir().join("nextgen-handler.desktop")
}

fn autostart_path() -> PathBuf {
    if is_windows() {
        // Display string for UI/status (registry location)
        return PathBuf::from(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run\NextGENHandlerListen");
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("autostart")
        .join("nextgen-handler-listen.desktop")
}

fn workspace_source_dir() -> PathBuf {
    // .../NextGEN-Handler/nextgen-handler-gui/src-tauri -> .../NextGEN-Handler
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(home_dir)
}

fn find_install_script() -> Option<PathBuf> {
    let mut candidates = vec![
        workspace_source_dir().join(if is_windows() { "install.ps1" } else { "install.sh" }),
        workspace_source_dir().join(if is_windows() { "install.cmd" } else { "install.sh" }),
    ];
    if !is_windows() {
        candidates.push(PathBuf::from(
            "/run/media/tuckerberg/68039F6637308204/IMPORTANT/NextGEN Software/NextGEN-Handler/install.sh",
        ));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if is_windows() {
                candidates.push(dir.join("install.ps1"));
                candidates.push(dir.join("install.cmd"));
                candidates.push(dir.join("..\\install.ps1"));
                candidates.push(dir.join("..\\install.cmd"));
            } else {
                candidates.push(dir.join("install.sh"));
                candidates.push(dir.join("../install.sh"));
                candidates.push(dir.join("../../install.sh"));
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn find_source_handler() -> Option<PathBuf> {
    let mut candidates = vec![
        workspace_source_dir().join("nextgen-handler.py"),
    ];
    if !is_windows() {
        candidates.push(PathBuf::from(
            "/run/media/tuckerberg/68039F6637308204/IMPORTANT/NextGEN Software/NextGEN-Handler/nextgen-handler.py",
        ));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("nextgen-handler.py"));
            if is_windows() {
                candidates.push(dir.join("..\\nextgen-handler.py"));
            } else {
                candidates.push(dir.join("../nextgen-handler.py"));
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn preferred_handler_script() -> Option<PathBuf> {
    let installed = local_handler_script();
    if installed.is_file() {
        return Some(installed);
    }
    find_source_handler()
}

fn command_exists(name: &str) -> bool {
    if name.contains('/') || name.contains('\\') {
        return Path::new(name).is_file();
    }
    let Some(path_os) = std::env::var_os("PATH") else {
        return false;
    };
    let exts: Vec<String> = if is_windows() {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string());
        pathext
            .split(';')
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path_os) {
        if is_windows() {
            let p = dir.join(name);
            if p.is_file() {
                return true;
            }
            for ext in &exts {
                let pext = dir.join(format!("{name}{ext}"));
                if pext.is_file() {
                    return true;
                }
                let pext_l = dir.join(format!("{name}{}", ext.to_lowercase()));
                if pext_l.is_file() {
                    return true;
                }
            }
        } else {
            let p = dir.join(name);
            if p.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = fs::metadata(&p) {
                        if meta.permissions().mode() & 0o111 != 0 {
                            return true;
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    return true;
                }
            }
        }
    }
    false
}

fn find_python_program() -> Option<String> {
    if is_windows() {
        for cand in ["py", "python", "python3"] {
            if command_exists(cand) {
                return Some(cand.to_string());
            }
        }
        None
    } else {
        for cand in ["python3", "python"] {
            if command_exists(cand) {
                return Some(cand.to_string());
            }
        }
        None
    }
}

fn handler_command_for(arg: &str) -> Result<(String, Vec<String>), String> {
    if is_windows() {
        if let Some(launcher) = local_handler_launcher() {
            return Ok(("cmd".to_string(), vec!["/C".to_string(), launcher.to_string_lossy().to_string(), arg.to_string()]));
        }
        let script = preferred_handler_script().ok_or_else(|| "Handler script not found".to_string())?;
        let py = find_python_program().ok_or_else(|| "Python not found (install Python 3 or run install)".to_string())?;
        if py.eq_ignore_ascii_case("py") {
            return Ok((py, vec!["-3".to_string(), script.to_string_lossy().to_string(), arg.to_string()]));
        }
        return Ok((py, vec![script.to_string_lossy().to_string(), arg.to_string()]));
    }
    let script = local_handler_script();
    let bin = if script.is_file() { script } else { preferred_handler_script().ok_or_else(|| "Handler not found".to_string())? };
    Ok((bin.to_string_lossy().to_string(), vec![arg.to_string()]))
}

fn run_cmd(program: &str, args: &[&str]) -> (bool, String) {
    let mut cmd = Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.output() {
        Ok(out) => {
            let mut s = String::from_utf8_lossy(&out.stdout).to_string();
            let e = String::from_utf8_lossy(&out.stderr);
            if !e.trim().is_empty() {
                if !s.is_empty() {
                    s.push('\n');
                }
                s.push_str(&e);
            }
            (out.status.success(), s.trim().to_string())
        }
        Err(e) => (false, e.to_string()),
    }
}

fn read_json_file(path: &Path) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}))
}

fn write_json_file(path: &Path, val: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(val).map_err(|e| e.to_string())?;
    fs::write(path, data + "\n").map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn ensure_config_templates() -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if !settings_path().exists() {
        write_json_file(
            &settings_path(),
            &json!({"player": ["vlc"], "titleArg": "--meta-title={t}"}),
        )?;
    }
    if !apps_path().exists() {
        if is_windows() {
            write_json_file(
                &apps_path(),
                &json!({
                    "_comment": "Add app ids here. Values are command lists executed directly (no shell).",
                    "steam": ["steam", "-tenfoot"]
                }),
            )?;
        } else {
            write_json_file(
                &apps_path(),
                &json!({
                    "_comment": "Add app ids here. Values are command lists executed directly (no shell).",
                    "bitchord": ["/home/tuckerberg/.local/lib/bitchord/bitchord"],
                    "luna": ["/opt/vivaldi/vivaldi", "--profile-directory=Default", "--app-id=agmpcdnpkedhhjldepagpgebdindblfd", "--start-fullscreen"],
                    "steam": ["steam", "-tenfoot"],
                    "xbox": ["/opt/vivaldi/vivaldi", "--profile-directory=Default", "--app-id=kanajofaghckoijdglhndcjjlemljefb", "--start-fullscreen"]
                }),
            )?;
        }
    }
    if !keys_path().exists() {
        write_json_file(
            &keys_path(),
            &json!({
                "_comment": "Global hotkeys for the --listen daemon. 'keys' are key names from --capture.",
                "bindings": [
                    { "keys": ["KEY_LEFTCTRL", "KEY_LEFTMETA"], "action": "focus_nextweb" },
                    { "keys": ["KEY_RIGHTCTRL", "KEY_LEFTMETA"], "action": "focus_nextweb" },
                    { "keys": ["KEY_LEFTCTRL", "KEY_RIGHTMETA"], "action": "focus_nextweb" },
                    { "keys": ["KEY_RIGHTCTRL", "KEY_RIGHTMETA"], "action": "focus_nextweb" }
                ],
                "remap": []
            }),
        )?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn listen_pids_unix() -> Vec<u32> {
    let mut pids = Vec::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for ent in entries.flatten() {
            let name = ent.file_name();
            let Some(s) = name.to_str() else { continue };
            let Ok(pid) = s.parse::<u32>() else { continue };
            if pid == std::process::id() {
                continue;
            }
            let Ok(bytes) = fs::read(ent.path().join("cmdline")) else { continue };
            let parts: Vec<String> = bytes
                .split(|b| *b == 0)
                .filter(|p| !p.is_empty())
                .map(|p| String::from_utf8_lossy(p).to_string())
                .collect();
            if parts.is_empty() {
                continue;
            }
            let joined = parts.join(" ");
            let is_handler = parts
                .iter()
                .any(|p| p.ends_with("nextgen-handler") || p.ends_with("nextgen-handler.py"));
            let has_listen = parts.iter().any(|p| p == "--listen") || joined.contains("--listen");
            if is_handler && has_listen {
                pids.push(pid);
            }
        }
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}

#[cfg(windows)]
fn listen_pids_windows() -> Vec<u32> {
    let ps = r#"Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine -match 'nextgen-handler' -and $_.CommandLine -match '--listen' } | Select-Object -ExpandProperty ProcessId"#;
    let (ok, out) = run_cmd("powershell", &["-NoProfile", "-Command", ps]);
    if !ok {
        return Vec::new();
    }
    let mut pids = Vec::new();
    for line in out.lines() {
        if let Ok(pid) = line.trim().parse::<u32>() {
            if pid != std::process::id() {
                pids.push(pid);
            }
        }
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}

fn listen_pids() -> Vec<u32> {
    #[cfg(windows)]
    {
        return listen_pids_windows();
    }
    #[cfg(not(windows))]
    {
        return listen_pids_unix();
    }
}

#[cfg(not(windows))]
fn is_autostart_enabled_unix() -> bool {
    let p = autostart_path();
    let Ok(s) = fs::read_to_string(&p) else { return false };
    for line in s.lines() {
        let l = line.trim();
        if l.eq_ignore_ascii_case("X-GNOME-Autostart-enabled=false")
            || l.eq_ignore_ascii_case("Hidden=true")
        {
            return false;
        }
    }
    true
}

#[cfg(windows)]
fn is_autostart_enabled_windows() -> bool {
    let (ok, out) = run_cmd(
        "reg",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "NextGENHandlerListen",
        ],
    );
    ok && out.to_lowercase().contains("nextgenhandlerlisten")
}

fn is_autostart_enabled() -> bool {
    #[cfg(windows)]
    {
        return is_autostart_enabled_windows();
    }
    #[cfg(not(windows))]
    {
        return is_autostart_enabled_unix();
    }
}

#[cfg(not(windows))]
fn write_autostart_unix(enabled: bool) -> Result<(), String> {
    let p = autostart_path();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if !enabled {
        if p.exists() {
            fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let exec = local_handler_script();
    let exec = if exec.is_file() {
        exec
    } else {
        preferred_handler_script().unwrap_or(exec)
    };
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=NextGEN Handler (Listen)\nComment=Start NextGEN Handler global hotkey listener\nExec={} --listen\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\nTerminal=false\nX-KDE-autostart-after=panel\n",
        exec.display()
    );
    fs::write(&p, content).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn windows_autostart_command() -> Result<String, String> {
    if let Some(launcher) = local_handler_launcher() {
        return Ok(format!("\"{}\" --listen", launcher.to_string_lossy()));
    }
    let script = preferred_handler_script().ok_or_else(|| "Handler script not found".to_string())?;
    let py = find_python_program().ok_or_else(|| "Python not found".to_string())?;
    if py.eq_ignore_ascii_case("py") {
        Ok(format!("py -3 \"{}\" --listen", script.to_string_lossy()))
    } else {
        Ok(format!("\"{}\" \"{}\" --listen", py, script.to_string_lossy()))
    }
}

#[cfg(windows)]
fn write_autostart_windows(enabled: bool) -> Result<(), String> {
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    if !enabled {
        let _ = run_cmd("reg", &["delete", key, "/v", "NextGENHandlerListen", "/f"]);
        return Ok(());
    }
    let cmd = windows_autostart_command()?;
    let (ok, out) = run_cmd(
        "reg",
        &[
            "add",
            key,
            "/v",
            "NextGENHandlerListen",
            "/t",
            "REG_SZ",
            "/d",
            &cmd,
            "/f",
        ],
    );
    if ok {
        Ok(())
    } else {
        Err(if out.is_empty() { "Failed to set autostart registry value".to_string() } else { out })
    }
}

fn write_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        return write_autostart_windows(enabled);
    }
    #[cfg(not(windows))]
    {
        return write_autostart_unix(enabled);
    }
}

#[cfg(not(windows))]
fn flatpak_app_installed(app_id: &str) -> bool {
    Command::new("flatpak")
        .args(["info", app_id])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn windows_scheme_registered() -> bool {
    let (ok, out) = run_cmd("reg", &["query", r"HKCU\Software\Classes\nextgen", "/ve"]);
    ok && (out.to_lowercase().contains("nextgen") || out.to_lowercase().contains("url"))
}

fn windows_scheme_command() -> String {
    let (ok, out) = run_cmd(
        "reg",
        &[
            "query",
            r"HKCU\Software\Classes\nextgen\shell\open\command",
            "/ve",
        ],
    );
    if ok { out } else { String::new() }
}

#[cfg(windows)]
fn perform_install_windows() -> Result<String, String> {
    let bin_dir = local_handler_dir();
    fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    let mut msgs: Vec<String> = Vec::new();

    let src = find_source_handler().ok_or_else(|| "nextgen-handler.py source not found".to_string())?;
    let dst = bin_dir.join("nextgen-handler.py");
    fs::copy(&src, &dst).map_err(|e| format!("copy handler: {e}"))?;
    msgs.push(format!("Installed handler: {}", dst.display()));

    let launcher = bin_dir.join("nextgen-handler.cmd");
    let launcher_content = format!(
        "@echo off\r\nsetlocal\r\nset \"HANDLER={}\"\r\nwhere py >nul 2>nul\r\nif %ERRORLEVEL%==0 (\r\n  py -3 \"%HANDLER%\" %*\r\n  exit /b %ERRORLEVEL%\r\n)\r\nwhere python >nul 2>nul\r\nif %ERRORLEVEL%==0 (\r\n  python \"%HANDLER%\" %*\r\n  exit /b %ERRORLEVEL%\r\n)\r\nwhere python3 >nul 2>nul\r\nif %ERRORLEVEL%==0 (\r\n  python3 \"%HANDLER%\" %*\r\n  exit /b %ERRORLEVEL%\r\n)\r\necho NextGEN Handler requires Python 3. Install Python and try again. 1>&2\r\nexit /b 9009\r\n",
        dst.display()
    );
    fs::write(&launcher, launcher_content).map_err(|e| format!("write launcher: {e}"))?;
    msgs.push(format!("Wrote launcher: {}", launcher.display()));

    let base = r"HKCU\Software\Classes\nextgen";
    let _ = run_cmd("reg", &["add", base, "/ve", "/d", "URL:NextGEN Protocol", "/f"]);
    let _ = run_cmd("reg", &["add", base, "/v", "URL Protocol", "/t", "REG_SZ", "/d", "", "/f"]);
    let icon = format!("\"{}\",0", launcher.display());
    let _ = run_cmd("reg", &[r"HKCU\Software\Classes\nextgen\DefaultIcon", "/ve", "/d", &icon, "/f"]);
    let cmdval = format!("\"{}\" \"%1\"", launcher.display());
    let (ok, out) = run_cmd(
        "reg",
        &[
            "add",
            r"HKCU\Software\Classes\nextgen\shell\open\command",
            "/ve",
            "/d",
            &cmdval,
            "/f",
        ],
    );
    if ok {
        msgs.push("Registered nextgen:// URL scheme (HKCU\\Software\\Classes\\nextgen)".to_string());
    } else {
        msgs.push(format!("Registry registration warning: {out}"));
    }

    ensure_config_templates()?;
    msgs.push(format!("Ensured config templates in {}", config_dir().display()));
    Ok(msgs.join("\n"))
}

#[cfg(not(windows))]
fn perform_install_unix() -> Result<String, String> {
    let bin_dir = home_dir().join(".local/bin");
    let app_dir = user_apps_dir();
    fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;

    let mut msgs: Vec<String> = Vec::new();
    if let Some(src) = find_source_handler() {
        let dst = bin_dir.join("nextgen-handler");
        fs::copy(&src, &dst).map_err(|e| format!("copy handler: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dst, fs::Permissions::from_mode(0o755));
        }
        msgs.push(format!("Installed handler: {}", dst.display()));
    } else if local_handler_script().is_file() {
        msgs.push(format!("Handler already present: {}", local_handler_script().display()));
    } else {
        msgs.push("Warning: nextgen-handler.py source not found; could not install binary (desktop registration still applied).".to_string());
    }

    let exec_path = if local_handler_script().is_file() {
        local_handler_script()
    } else {
        preferred_handler_script().unwrap_or_else(|| local_handler_script())
    };
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=NextGEN Handler\nComment=Handles nextgen:// links (stream handoff to VLC, app launching)\nExec={} %u\nNoDisplay=true\nTerminal=false\nMimeType=x-scheme-handler/nextgen;\nCategories=Utility;\n",
        exec_path.display()
    );
    fs::write(handler_desktop_path(), desktop).map_err(|e| format!("write desktop: {e}"))?;
    msgs.push(format!("Wrote {}", handler_desktop_path().display()));

    let (ok1, out1) = run_cmd(
        "xdg-mime",
        &["default", "nextgen-handler.desktop", "x-scheme-handler/nextgen"],
    );
    msgs.push(if ok1 {
        "Registered x-scheme-handler/nextgen (xdg-mime)".to_string()
    } else {
        format!("xdg-mime: {out1}")
    });
    let _ = run_cmd("update-desktop-database", &[user_apps_dir().to_string_lossy().as_ref()]);

    ensure_config_templates()?;
    msgs.push(format!("Ensured config templates in {}", config_dir().display()));
    Ok(msgs.join("\n"))
}

fn perform_install() -> Result<String, String> {
    #[cfg(windows)]
    {
        return perform_install_windows();
    }
    #[cfg(not(windows))]
    {
        return perform_install_unix();
    }
}

#[tauri::command]
fn get_status(state: State<'_, Arc<AppState>>) -> Value {
    let cap_running = state
        .capture
        .lock()
        .ok()
        .map(|c| c.child.is_some())
        .unwrap_or(false);

    let handler_script = preferred_handler_script();
    let handler_script_str = handler_script
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let handler_binary_exists = local_handler_script().is_file() || local_handler_launcher().is_some();
    let configs_ok = apps_path().exists() && keys_path().exists() && settings_path().exists();

    #[cfg(windows)]
    let (scheme_registered, scheme_handler, handler_desktop_exists, installed) = {
        let reg = windows_scheme_registered();
        let cmd = windows_scheme_command();
        let installed = reg && handler_binary_exists && configs_ok;
        (reg, cmd, reg, installed)
    };
    #[cfg(not(windows))]
    let (scheme_registered, scheme_handler, handler_desktop_exists, installed) = {
        let (ok, handler) = run_cmd("xdg-mime", &["query", "default", "x-scheme-handler/nextgen"]);
        let scheme_handler = if ok { handler } else { String::new() };
        let scheme_ok = scheme_handler.contains("nextgen");
        let desktop_exists = handler_desktop_path().exists();
        let installed = scheme_ok && desktop_exists && (local_handler_script().is_file() || find_source_handler().is_some()) && configs_ok;
        (scheme_ok, scheme_handler, desktop_exists, installed)
    };

    json!({
        "installed": installed,
        "schemeRegistered": scheme_registered,
        "schemeHandler": scheme_handler,
        "handlerDesktopExists": handler_desktop_exists,
        "handlerBinaryExists": handler_binary_exists,
        "handlerBinaryPath": if handler_script_str.is_empty() { local_handler_script().display().to_string() } else { handler_script_str },
        "configDir": config_dir().display().to_string(),
        "configDirExists": config_dir().exists(),
        "appsJsonExists": apps_path().exists(),
        "keysJsonExists": keys_path().exists(),
        "settingsJsonExists": settings_path().exists(),
        "autostartEnabled": is_autostart_enabled(),
        "autostartPath": autostart_path().display().to_string(),
        "listenRunning": !listen_pids().is_empty(),
        "listenPids": listen_pids(),
        "captureRunning": cap_running,
        "installScript": find_install_script().map(|p| p.display().to_string()),
        "sourceHandler": find_source_handler().map(|p| p.display().to_string()),
        "platform": if is_windows() { "windows" } else { "linux" },
    })
}

#[tauri::command]
fn run_install() -> Result<String, String> {
    perform_install()
}

#[tauri::command]
fn register_scheme() -> Result<String, String> {
    perform_install()
}

#[tauri::command]
fn ensure_configs() -> Result<String, String> {
    ensure_config_templates()?;
    Ok(config_dir().display().to_string())
}

#[tauri::command]
fn get_apps() -> Result<Value, String> {
    Ok(read_json_file(&apps_path()))
}

#[tauri::command]
fn save_apps(apps: Map<String, Value>, preserve_other: Option<bool>) -> Result<(), String> {
    let keep = preserve_other.unwrap_or(true);
    let mut out = if keep {
        read_json_file(&apps_path())
    } else {
        json!({})
    };
    let obj = out
        .as_object_mut()
        .ok_or_else(|| "apps.json root is not an object".to_string())?;
    let keys_to_remove: Vec<String> = obj
        .iter()
        .filter(|(_, v)| v.is_array())
        .map(|(k, _)| k.clone())
        .collect();
    for k in keys_to_remove {
        obj.remove(&k);
    }
    for (k, v) in apps {
        obj.insert(k, v);
    }
    write_json_file(&apps_path(), &out)
}

#[tauri::command]
fn get_settings() -> Result<Value, String> {
    Ok(read_json_file(&settings_path()))
}
#[tauri::command]
fn save_settings(settings: Value) -> Result<(), String> {
    write_json_file(&settings_path(), &settings)
}
#[tauri::command]
fn get_keys() -> Result<Value, String> {
    Ok(read_json_file(&keys_path()))
}
#[tauri::command]
fn save_keys(keys: Value) -> Result<(), String> {
    write_json_file(&keys_path(), &keys)
}

#[tauri::command]
fn get_autostart() -> bool {
    is_autostart_enabled()
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<bool, String> {
    write_autostart(enabled)?;
    Ok(is_autostart_enabled())
}

#[tauri::command]
fn stop_listener() -> Result<Vec<u32>, String> {
    let pids = listen_pids();
    for pid in &pids {
        #[cfg(windows)]
        {
            let _ = run_cmd("taskkill", &["/PID", &pid.to_string(), "/T", "/F"]);
        }
        #[cfg(not(windows))]
        {
            let _ = Command::new("kill").arg("-TERM").arg(pid.to_string()).status();
        }
    }
    thread::sleep(Duration::from_millis(350));
    for pid in &listen_pids() {
        #[cfg(windows)]
        {
            let _ = run_cmd("taskkill", &["/PID", &pid.to_string(), "/T", "/F"]);
        }
        #[cfg(not(windows))]
        {
            let _ = Command::new("kill").arg("-KILL").arg(pid.to_string()).status();
        }
    }
    thread::sleep(Duration::from_millis(120));
    Ok(pids)
}

#[tauri::command]
fn start_listener() -> Result<String, String> {
    if !listen_pids().is_empty() {
        return Ok("Listener already running".to_string());
    }
    let (prog, args) = handler_command_for("--listen")?;
    let mut cmd = Command::new(&prog);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(format!("Started: {} {}", prog, args.join(" ")))
}

#[tauri::command]
fn start_capture(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let mut cap = state
        .capture
        .lock()
        .map_err(|_| "capture lock poisoned".to_string())?;
    if cap.child.is_some() {
        return Err("Capture already running".to_string());
    }
    let (prog, args) = handler_command_for("--capture")?;
    let mut cmd = Command::new(&prog);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    cap.lines.clear();
    cap.lines.push(format!("Capture started ({} {}).", prog, args.join(" ")));
    cap.lines.push("Press keys/buttons on your keyboard/remote to see their names (then Stop capture).".to_string());

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    cap.child = Some(child);
    drop(cap);

    let state_cloned = Arc::clone(&state.inner());
    let push = move |line: String| {
        if let Ok(mut c) = state_cloned.capture.lock() {
            c.lines.push(line);
            if c.lines.len() > 500 {
                let drain = c.lines.len() - 500;
                c.lines.drain(0..drain);
            }
        }
    };

    if let Some(out) = stdout {
        let push1 = push.clone();
        thread::spawn(move || {
            let reader = BufReader::new(out);
            for line in reader.lines().flatten() {
                push1(line);
            }
        });
    }
    if let Some(err) = stderr {
        thread::spawn(move || {
            let reader = BufReader::new(err);
            for line in reader.lines().flatten() {
                push(format!("[stderr] {line}"));
            }
        });
    }
    Ok("Capture started".to_string())
}

#[tauri::command]
fn stop_capture(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut cap = state
        .capture
        .lock()
        .map_err(|_| "capture lock poisoned".to_string())?;
    if let Some(mut child) = cap.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    cap.lines.push("Capture stopped.".to_string());
    Ok(())
}

#[tauri::command]
fn read_capture(state: State<'_, Arc<AppState>>, clear: Option<bool>) -> Result<Vec<String>, String> {
    let mut cap = state
        .capture
        .lock()
        .map_err(|_| "capture lock poisoned".to_string())?;
    if let Some(child) = cap.child.as_mut() {
        if let Ok(Some(_)) = child.try_wait() {
            cap.child = None;
            cap.lines.push("Capture process exited.".to_string());
        }
    }
    let out = cap.lines.clone();
    if clear.unwrap_or(false) {
        cap.lines.clear();
    }
    Ok(out)
}

#[tauri::command]
fn capture_running(state: State<'_, Arc<AppState>>) -> bool {
    state
        .capture
        .lock()
        .ok()
        .map(|mut c| {
            if let Some(child) = c.child.as_mut() {
                if let Ok(Some(_)) = child.try_wait() {
                    c.child = None;
                    return false;
                }
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
}

fn parse_desktop_file(path: &Path) -> Result<Value, String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut exec_line: Option<String> = None;
    let mut name: Option<String> = None;
    let mut icon: Option<String> = None;
    let mut typ: Option<String> = None;
    let mut nodisplay = false;
    let mut hidden = false;
    let mut in_desktop = false;
    for line in content.lines() {
        let l = line.trim_end();
        if l.trim_start().starts_with('[') {
            in_desktop = l.trim().eq_ignore_ascii_case("[Desktop Entry]");
            continue;
        }
        if !in_desktop {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            match k.trim() {
                "Exec" => if exec_line.is_none() { exec_line = Some(v.to_string()); },
                "Name" => if name.is_none() { name = Some(v.to_string()); },
                "Icon" => if icon.is_none() { icon = Some(v.to_string()); },
                "Type" => if typ.is_none() { typ = Some(v.to_string()); },
                "NoDisplay" => nodisplay = v.trim().eq_ignore_ascii_case("true"),
                "Hidden" => hidden = v.trim().eq_ignore_ascii_case("true"),
                _ => {}
            }
        }
    }
    let exec = exec_line.unwrap_or_default();
    let mut parts: Vec<String> = Vec::new();
    if !exec.is_empty() {
        let mut cur = String::new();
        let mut chars = exec.chars().peekable();
        let mut in_quote: Option<char> = None;
        while let Some(c) = chars.next() {
            match in_quote {
                Some(q) => {
                    if c == q {
                        in_quote = None;
                    } else if c == '\\' {
                        if let Some(nc) = chars.next() { cur.push(nc); } else { cur.push(c); }
                    } else {
                        cur.push(c);
                    }
                }
                None => {
                    if c == '"' || c == '\'' {
                        in_quote = Some(c);
                    } else if c.is_whitespace() {
                        if !cur.is_empty() { parts.push(std::mem::take(&mut cur)); }
                    } else if c == '\\' {
                        if let Some(nc) = chars.next() { cur.push(nc); } else { cur.push(c); }
                    } else {
                        cur.push(c);
                    }
                }
            }
        }
        if !cur.is_empty() { parts.push(cur); }
    }
    let parts: Vec<String> = parts.into_iter().filter(|p| !p.starts_with('%')).collect();
    let suggested_id = name.as_ref().map(|n| {
        let s: String = n
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let mut out = String::new();
        let mut prev_dash = false;
        for c in s.chars() {
            if c == '-' {
                if !prev_dash && !out.is_empty() { out.push('-'); }
                prev_dash = true;
            } else {
                out.push(c);
                prev_dash = false;
            }
        }
        out.trim_matches('-').to_string()
    }).filter(|s| !s.is_empty());
    Ok(json!({
        "name": name,
        "icon": icon,
        "type": typ,
        "noDisplay": nodisplay,
        "hidden": hidden,
        "exec": exec,
        "command": parts,
        "path": path.display().to_string(),
        "suggestedId": suggested_id,
    }))
}

#[tauri::command]
fn parse_desktop_exec(path: String) -> Result<Value, String> {
    let p = Path::new(&path);
    if is_windows() {
        // Best-effort: accept simple .cmd/.bat/.exe paths by treating as command
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if ext == "exe" || ext == "cmd" || ext == "bat" {
            let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("app");
            let suggested = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>();
            return Ok(json!({
                "name": name,
                "icon": null,
                "type": "Application",
                "noDisplay": false,
                "hidden": false,
                "exec": path,
                "command": [path],
                "path": path,
                "suggestedId": suggested.trim_matches('-'),
            }));
        }
    }
    let info = parse_desktop_file(p)?;
    if info.get("exec").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
        return Err("No Exec= found in [Desktop Entry]".to_string());
    }
    Ok(info)
}

fn windows_program_candidates(relative: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for base in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
        if let Ok(b) = std::env::var(base) {
            let mut p = PathBuf::from(b);
            for part in relative {
                p.push(part);
            }
            out.push(p.to_string_lossy().to_string());
        }
    }
    out
}

fn first_existing(paths: &[String]) -> Option<String> {
    for p in paths {
        if Path::new(p).is_file() {
            return Some(p.clone());
        }
    }
    None
}

#[tauri::command]
fn detect_players() -> Value {
    let mut presets: Vec<Value> = Vec::new();
    let mut push = |id: &str, label: &str, player: Vec<String>, title_arg: &str, detected: bool, note: &str| {
        presets.push(json!({
            "id": id,
            "label": label,
            "player": player,
            "titleArg": title_arg,
            "detected": detected,
            "note": note,
        }));
    };

    if is_windows() {
        let vlc = first_existing(&windows_program_candidates(&["VideoLAN", "VLC", "vlc.exe"]))
            .or_else(|| if command_exists("vlc") { Some("vlc".to_string()) } else { None });
        let mpv = first_existing(&windows_program_candidates(&["mpv", "mpv.exe"]))
            .or_else(|| if command_exists("mpv") { Some("mpv".to_string()) } else { None });
        let mpvnet = first_existing(&windows_program_candidates(&["mpv.net", "mpvnet.exe"]));
        push("vlc", "VLC", vec![vlc.clone().unwrap_or_else(|| "vlc".to_string())], "--meta-title={t}", vlc.is_some() || command_exists("vlc"), "Recommended default");
        push("mpv", "mpv", vec![mpv.clone().unwrap_or_else(|| "mpv".to_string())], "--force-media-title={t}", mpv.is_some() || command_exists("mpv"), "Lightweight, powerful");
        if let Some(p) = mpvnet {
            push("mpvnet", "mpv.net", vec![p], "--force-media-title={t}", true, "Windows mpv frontend");
        } else {
            push("mpvnet", "mpv.net", vec!["mpvnet".to_string()], "--force-media-title={t}", command_exists("mpvnet"), "Windows mpv frontend");
        }
        push("browser", "Default browser (open URL)", vec!["cmd".to_string(), "/c".to_string(), "start".to_string(), "".to_string()], "", true, "Fallback: opens stream URL in default browser");
    } else {
        push("vlc", "VLC", vec!["vlc".to_string()], "--meta-title={t}", command_exists("vlc"), "Recommended default");
        push("mpv", "mpv", vec!["mpv".to_string()], "--force-media-title={t}", command_exists("mpv"), "Lightweight, powerful");
        push("celluloid", "Celluloid (mpv frontend)", vec!["celluloid".to_string()], "--mpv-force-media-title={t}", command_exists("celluloid"), "GTK frontend for mpv");
        push("haruna", "Haruna (mpv frontend)", vec!["haruna".to_string()], "", command_exists("haruna"), "Qt/mpv player (title arg may vary)");
        push("smplayer", "SMPlayer", vec!["smplayer".to_string()], "", command_exists("smplayer"), "Frontend for mpv/MPlayer");
        push("clapper", "Clapper", vec!["clapper".to_string()], "", command_exists("clapper"), "GNOME media player");
        push("totem", "Videos (Totem)", vec!["totem".to_string()], "", command_exists("totem"), "GNOME Videos");
        #[cfg(not(windows))]
        if command_exists("flatpak") {
            push("flatpak-vlc", "VLC (Flatpak)", vec!["flatpak".to_string(), "run".to_string(), "org.videolan.VLC".to_string()], "--meta-title={t}", flatpak_app_installed("org.videolan.VLC"), "org.videolan.VLC");
            push("flatpak-mpv", "mpv (Flatpak)", vec!["flatpak".to_string(), "run".to_string(), "io.mpv.Mpv".to_string()], "--force-media-title={t}", flatpak_app_installed("io.mpv.Mpv"), "io.mpv.Mpv");
            push("flatpak-celluloid", "Celluloid (Flatpak)", vec!["flatpak".to_string(), "run".to_string(), "io.github.celluloid_player.Celluloid".to_string()], "--mpv-force-media-title={t}", flatpak_app_installed("io.github.celluloid_player.Celluloid"), "io.github.celluloid_player.Celluloid");
        }
    }

    json!({ "presets": presets })
}

#[cfg(not(windows))]
fn list_installed_apps_unix() -> Result<Vec<Value>, String> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local/share"));
    let mut dirs: Vec<PathBuf> = vec![
        data_home.join("applications"),
        home_dir().join(".local/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        data_home.join("flatpak/exports/share/applications"),
        home_dir().join(".local/share/flatpak/exports/share/applications"),
        home_dir().join("Desktop"),
    ];
    dirs.sort();
    dirs.dedup();

    let mut seen_paths = HashSet::new();
    let mut apps: Vec<Value> = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for ent in entries.flatten() {
            let path = ent.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") { continue; }
            let canon = path.canonicalize().unwrap_or(path.clone());
            let key = canon.display().to_string();
            if !seen_paths.insert(key) { continue; }
            let Ok(info) = parse_desktop_file(&path) else { continue; };
            let typ = info.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if !typ.is_empty() && !typ.eq_ignore_ascii_case("Application") { continue; }
            if info.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false) { continue; }
            if info.get("noDisplay").and_then(|v| v.as_bool()).unwrap_or(false) { continue; }
            let cmd = info.get("command").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            if cmd.is_empty() { continue; }
            apps.push(info);
        }
    }
    apps.sort_by(|a, b| {
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        an.cmp(&bn).then_with(|| a.get("path").and_then(|v| v.as_str()).unwrap_or("").cmp(b.get("path").and_then(|v| v.as_str()).unwrap_or("")))
    });
    Ok(apps)
}

#[cfg(windows)]
fn list_installed_apps_windows() -> Result<Vec<Value>, String> {
    // Best-effort scan of Start Menu shortcuts via PowerShell COM resolver.
    let ps = r#"
$ErrorActionPreference = 'SilentlyContinue'
$wsh = New-Object -ComObject WScript.Shell
$paths = @(
  [Environment]::GetFolderPath('CommonStartMenu'),
  [Environment]::GetFolderPath('StartMenu'),
  [Environment]::GetFolderPath('CommonDesktopDirectory'),
  [Environment]::GetFolderPath('Desktop')
) | Where-Object { $_ -and (Test-Path $_) }
$results = @()
$seen = @{}
foreach ($root in $paths) {
  Get-ChildItem -Path $root -Recurse -Filter *.lnk -File -ErrorAction SilentlyContinue | ForEach-Object {
    $full = $_.FullName
    if ($seen.ContainsKey($full)) { return }
    $seen[$full] = $true
    try {
      $sc = $wsh.CreateShortcut($full)
      $target = [string]$sc.TargetPath
      $args = [string]$sc.Arguments
      if (-not $target) { return }
      $name = $_.BaseName
      $results += [PSCustomObject]@{ name = $name; target = $target; args = $args; path = $full }
    } catch { }
  }
}
$results | ConvertTo-Json -Depth 4 -Compress
"#;
    let (ok, out) = run_cmd("powershell", &["-NoProfile", "-Command", ps]);
    if !ok {
        return Ok(Vec::new());
    }
    let trimmed = out.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(Vec::new());
    }
    let parsed: Value = serde_json::from_str(trimmed).unwrap_or_else(|_| json!([]));
    let arr = match parsed {
        Value::Array(a) => a,
        Value::Object(_) => vec![parsed],
        _ => vec![],
    };
    let mut apps = Vec::new();
    let mut seen = HashSet::new();
    for item in arr {
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let target = item.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let args = item.get("args").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if target.is_empty() {
            continue;
        }
        let key = path.clone();
        if !seen.insert(key) {
            continue;
        }
        let mut cmd = vec![target.clone()];
        if !args.trim().is_empty() {
            // Simple split; Windows shortcut args are a single string
            cmd.push(args.trim().to_string());
        }
        let suggested = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>();
        apps.push(json!({
            "name": if name.is_empty() { Value::Null } else { Value::String(name.clone()) },
            "icon": Value::Null,
            "type": "Application",
            "noDisplay": false,
            "hidden": false,
            "exec": if args.trim().is_empty() { target.clone() } else { format!("{} {}", target, args.trim()) },
            "command": cmd,
            "path": path,
            "suggestedId": suggested.trim_matches('-').to_string(),
        }));
    }
    apps.sort_by(|a, b| {
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        an.cmp(&bn)
    });
    Ok(apps)
}

#[tauri::command]
fn list_installed_apps() -> Result<Vec<Value>, String> {
    #[cfg(windows)]
    {
        return list_installed_apps_windows();
    }
    #[cfg(not(windows))]
    {
        return list_installed_apps_unix();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(AppState::default()))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_status,
            run_install,
            register_scheme,
            ensure_configs,
            get_apps,
            save_apps,
            get_settings,
            save_settings,
            get_keys,
            save_keys,
            get_autostart,
            set_autostart,
            stop_listener,
            start_listener,
            start_capture,
            stop_capture,
            read_capture,
            capture_running,
            parse_desktop_exec,
            list_installed_apps,
            detect_players,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}