import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Json = any;

type Status = {
  installed: boolean;
  schemeRegistered: boolean;
  schemeHandler: string;
  handlerDesktopExists: boolean;
  handlerBinaryExists: boolean;
  handlerBinaryPath: string;
  configDir: string;
  configDirExists: boolean;
  appsJsonExists: boolean;
  keysJsonExists: boolean;
  settingsJsonExists: boolean;
  autostartEnabled: boolean;
  autostartPath: string;
  listenRunning: boolean;
  listenPids: number[];
  captureRunning: boolean;
  installScript?: string | null;
  sourceHandler?: string | null;
  platform?: string;
};

type AppEntry = { id: string; command: string[] };

type InstalledApp = {
  name?: string | null;
  icon?: string | null;
  exec?: string;
  command: string[];
  path: string;
  suggestedId?: string | null;
};

type PlayerPreset = {
  id: string;
  label: string;
  player: string[];
  titleArg: string;
  detected: boolean;
  note?: string;
};

function asAppEntries(appsJson: Json): AppEntry[] {
  const obj = (appsJson && typeof appsJson === "object" && !Array.isArray(appsJson)) ? appsJson : {};
  const entries: AppEntry[] = [];
  for (const [k, v] of Object.entries(obj)) {
    if (Array.isArray(v) && v.every((x) => typeof x === "string")) {
      entries.push({ id: k, command: v as string[] });
    }
  }
  entries.sort((a, b) => a.id.localeCompare(b.id));
  return entries;
}

function toAppsObject(entries: AppEntry[]): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  for (const e of entries) {
    if (!e.id.trim()) continue;
    const cmd = e.command.map((s) => s.trim()).filter((s) => s.length > 0);
    if (cmd.length === 0) continue;
    out[e.id.trim()] = cmd;
  }
  return out;
}

function Badge({ kind, children }: { kind: "good" | "bad" | "warn" | "neutral"; children: React.ReactNode }) {
  return <span className={`badge ${kind === "neutral" ? "" : kind}`}><span className="dot" />{children}</span>;
}

function Toggle({ checked, onChange, disabled }: { checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <label className="toggle" style={{ opacity: disabled ? 0.6 : 1 }}>
      <input type="checkbox" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
      <span className="slider" />
    </label>
  );
}

export default function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [apps, setApps] = useState<AppEntry[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<{ kind: "good" | "bad" | "warn"; text: string } | null>(null);

  // Add/edit form
  const [editId, setEditId] = useState<string | null>(null);
  const [appId, setAppId] = useState("");
  const [appCmd, setAppCmd] = useState("");
  const [appArgs, setAppArgs] = useState("");
  const [desktopPath, setDesktopPath] = useState("");

  // App picker
  const [pickerOpen, setPickerOpen] = useState(false);
  const [installedApps, setInstalledApps] = useState<InstalledApp[]>([]);
  const [pickerLoading, setPickerLoading] = useState(false);
  const [pickerQuery, setPickerQuery] = useState("");

  // Stream player / settings
  const [settingsRaw, setSettingsRaw] = useState<Json>({});
  const [playerPresets, setPlayerPresets] = useState<PlayerPreset[]>([]);
  const [playerCmd, setPlayerCmd] = useState("vlc");
  const [playerArgs, setPlayerArgs] = useState("");
  const [titleArg, setTitleArg] = useState("--meta-title={t}");

  // Capture
  const [captureLines, setCaptureLines] = useState<string[]>([]);
  const [capturePolling, setCapturePolling] = useState(false);
  const [listenerWasRunning, setListenerWasRunning] = useState(false);
  const [autostartWasEnabled, setAutostartWasEnabled] = useState(false);

  const applySettings = (s: Json) => {
    setSettingsRaw(s && typeof s === "object" ? s : {});
    const p = s && Array.isArray(s.player) ? (s.player as any[]).filter((x) => typeof x === "string") as string[] : [];
    if (p.length > 0) {
      setPlayerCmd(p[0] || "");
      setPlayerArgs(p.slice(1).join("\n"));
    } else {
      setPlayerCmd("vlc");
      setPlayerArgs("");
    }
    setTitleArg(typeof s?.titleArg === "string" ? s.titleArg : "");
  };

  const refresh = async () => {
    const [s, a, st, det] = await Promise.all([
      invoke<Status>("get_status"),
      invoke<Json>("get_apps"),
      invoke<Json>("get_settings"),
      invoke<{ presets: PlayerPreset[] }>("detect_players"),
    ]);
    setStatus(s);
    setApps(asAppEntries(a));
    applySettings(st);
    setPlayerPresets(det?.presets || []);
  };

  useEffect(() => { refresh().catch((e) => setMsg({ kind: "bad", text: String(e) })); }, []);

  useEffect(() => {
    if (!capturePolling) return;
    const t = setInterval(async () => {
      try {
        const [lines, running] = await Promise.all([
          invoke<string[]>("read_capture"),
          invoke<boolean>("capture_running"),
        ]);
        setCaptureLines(lines);
        if (!running) setCapturePolling(false);
      } catch { /* ignore */ }
    }, 700);
    return () => clearInterval(t);
  }, [capturePolling]);

  const run = async (label: string, fn: () => Promise<any>, okMsg?: string) => {
    setBusy(label);
    setMsg(null);
    try {
      const res = await fn();
      await refresh();
      if (okMsg) setMsg({ kind: "good", text: okMsg });
      return res;
    } catch (e) {
      setMsg({ kind: "bad", text: String(e) });
      throw e;
    } finally {
      setBusy(null);
    }
  };

  const onInstall = () => run("install", () => invoke<string>("run_install").then((m) => setMsg({ kind: "good", text: m || "Install completed." })));

  const onAutostartToggle = (v: boolean) => run("autostart", () => invoke("set_autostart", { enabled: v }), v ? "Autostart enabled (listener will start on login)." : "Autostart disabled.");

  const onStartListener = () => run("start-listen", () => invoke<string>("start_listener").then((m) => setMsg({ kind: "good", text: m })));
  const onStopListener = () => run("stop-listen", () => invoke<number[]>("stop_listener").then((p) => setMsg({ kind: "good", text: p.length ? `Stopped listener (pid ${p.join(", ")})` : "Listener not running." })));

  const resetAppForm = () => { setEditId(null); setAppId(""); setAppCmd(""); setAppArgs(""); setDesktopPath(""); };

  const loadAppToForm = (e: AppEntry) => {
    setEditId(e.id);
    setAppId(e.id);
    setAppCmd(e.command[0] || "");
    setAppArgs((e.command.slice(1) || []).join("\n"));
  };

  const buildCommand = (): string[] => {
    const cmd = appCmd.trim();
    const args = appArgs.split(/\r?\n/).map((s) => s.trim()).filter((s) => s.length > 0);
    return cmd ? [cmd, ...args] : [];
  };

  const onSaveApp = async () => {
    const id = appId.trim();
    const command = buildCommand();
    if (!id) { setMsg({ kind: "warn", text: "App id is required." }); return; }
    if (command.length === 0) { setMsg({ kind: "warn", text: "Command/executable is required." }); return; }
    const next = [...apps.filter((a) => a.id !== id && a.id !== editId), { id, command }].sort((a, b) => a.id.localeCompare(b.id));
    await run("save-app", async () => {
      await invoke("ensure_configs");
      await invoke("save_apps", { apps: toAppsObject(next), preserveOther: true });
    }, `Saved app "${id}".`);
    resetAppForm();
  };

  const onDeleteApp = async (id: string) => {
    const next = apps.filter((a) => a.id !== id);
    await run("delete-app", async () => {
      await invoke("save_apps", { apps: toAppsObject(next), preserveOther: true });
    }, `Removed app "${id}".`);
    if (editId === id) resetAppForm();
  };

  const applyInstalledApp = (info: InstalledApp) => {
    const cmd: string[] = Array.isArray(info?.command) ? info.command : [];
    const suggested = (info?.suggestedId || (info?.name ? String(info.name).toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") : "")) || "";
    if (suggested) setAppId(suggested);
    if (cmd.length > 0) {
      setAppCmd(cmd[0] || "");
      setAppArgs(cmd.slice(1).join("\n"));
    }
    if (info?.path) setDesktopPath(info.path);
    setPickerOpen(false);
    setMsg({ kind: "good", text: `Selected ${info?.name || info?.path || "app"} — review and save.` });
  };

  const openPicker = async () => {
    setPickerOpen(true);
    setPickerQuery("");
    if (installedApps.length > 0) return;
    setPickerLoading(true);
    try {
      const list = await invoke<InstalledApp[]>("list_installed_apps");
      setInstalledApps(list || []);
    } catch (e) {
      setMsg({ kind: "bad", text: String(e) });
    } finally {
      setPickerLoading(false);
    }
  };

  const onParseDesktop = async () => {
    const p = desktopPath.trim();
    if (!p) { setMsg({ kind: "warn", text: "Enter a .desktop file path." }); return; }
    try {
      setBusy("parse-desktop");
      const info = await invoke<any>("parse_desktop_exec", { path: p });
      applyInstalledApp(info as InstalledApp);
      setMsg({ kind: "good", text: `Parsed ${p}` });
    } catch (e) {
      setMsg({ kind: "bad", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const onStartCapture = async () => {
    if (!status) return;
    setBusy("capture-start");
    setMsg(null);
    try {
      const wasRunning = status.listenRunning;
      const wasAuto = status.autostartEnabled;
      setListenerWasRunning(wasRunning);
      setAutostartWasEnabled(wasAuto);
      if (wasRunning) {
        await invoke("stop_listener");
      }
      await invoke("start_capture");
      setCapturePolling(true);
      const lines = await invoke<string[]>("read_capture");
      setCaptureLines(lines);
      await refresh();
      setMsg({ kind: "warn", text: wasRunning ? "Listener stopped for capture. Stop capture to restore it." : "Capture started. Stop capture when done." });
    } catch (e) {
      setMsg({ kind: "bad", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const onStopCapture = async () => {
    setBusy("capture-stop");
    setMsg(null);
    try {
      await invoke("stop_capture");
      const lines = await invoke<string[]>("read_capture");
      setCaptureLines(lines);
      setCapturePolling(false);
      // restore listener if it was running before, or if autostart was enabled (user expects it on)
      if (listenerWasRunning || autostartWasEnabled) {
        try { await invoke("start_listener"); } catch { /* may fail if not installed */ }
      }
      await refresh();
      setMsg({ kind: "good", text: (listenerWasRunning || autostartWasEnabled) ? "Capture stopped and listener restored." : "Capture stopped." });
    } catch (e) {
      setMsg({ kind: "bad", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const buildPlayerArray = (): string[] => {
    const cmd = playerCmd.trim();
    const args = playerArgs.split(/\r?\n/).map((s) => s.trim()).filter((s) => s.length > 0);
    return cmd ? [cmd, ...args] : [];
  };

  const applyPreset = (p: PlayerPreset) => {
    const arr = Array.isArray(p.player) ? p.player : [];
    setPlayerCmd(arr[0] || "");
    setPlayerArgs(arr.slice(1).join("\n"));
    setTitleArg(p.titleArg || "");
    setMsg({ kind: "good", text: `Selected preset: ${p.label}. Review and save.` });
  };

  const onSaveSettings = async () => {
    const player = buildPlayerArray();
    if (player.length === 0) { setMsg({ kind: "warn", text: "Player command is required." }); return; }
    const next = { ...(settingsRaw && typeof settingsRaw === "object" ? settingsRaw : {}), player, titleArg: titleArg };
    await run("save-settings", async () => {
      await invoke("ensure_configs");
      await invoke("save_settings", { settings: next });
    }, "Saved player settings to settings.json.");
  };

  const installedBadge = useMemo(() => {
    if (!status) return <Badge kind="neutral">Checking…</Badge>;
    return status.installed ? <Badge kind="good">Installed</Badge> : <Badge kind="warn">Not fully installed</Badge>;
  }, [status]);

  const isWindows = status?.platform === "windows";

  return (
    <div className="app">
      <div className="header">
        <div>
          <h1 className="title">NextGEN Handler</h1>
          <p className="subtitle">Configure <span className="inline-code">nextgen://</span>, local apps, autostart, and key capture{status?.platform ? ` (${isWindows ? "Windows" : "Linux"})` : ""}</p>
        </div>
        <div className="row">
          {status?.listenRunning ? <Badge kind="good">Listener running</Badge> : <Badge kind="neutral">Listener stopped</Badge>}
          {status?.captureRunning ? <Badge kind="warn">Capturing</Badge> : null}
        </div>
      </div>

      {msg && <div className={`notice ${msg.kind}`} style={{ marginBottom: 14 }}>{msg.text}</div>}

      <div className="grid">
        <section className="card">
          <div className="card-h">
            <div>
              <h2 className="card-title">Setup & Registration</h2>
              <p className="card-desc">Built-in install: installs the handler, registers the <span className="inline-code">nextgen://</span> URL scheme, and ensures config templates.</p>
            </div>
            <div className="row">
              {installedBadge}
              <button className="btn primary" onClick={onInstall} disabled={!!busy}>
                {status?.installed ? "Run Install / Repair" : "Run Install"}
              </button>
            </div>
          </div>
          {status && (
            <div className="kv">
              <div className="k">{isWindows ? "Scheme registration" : "Scheme handler"}</div><div className="v">{status.schemeHandler || (status.schemeRegistered ? "Registered" : "—")} {status.schemeRegistered ? "✓" : ""}</div>
              <div className="k">Handler</div><div className="v">{status.handlerBinaryPath} {status.handlerBinaryExists ? "✓" : "(missing)"}</div>
              {!isWindows && <><div className="k">Desktop entry</div><div className="v">~/.local/share/applications/nextgen-handler.desktop {status.handlerDesktopExists ? "✓" : "(missing)"}</div></>}
              <div className="k">Config dir</div><div className="v">{status.configDir} {status.configDirExists ? "✓" : "(missing)"}</div>
              <div className="k">Config files</div><div className="v">apps.json {status.appsJsonExists ? "✓" : "✗"} · keys.json {status.keysJsonExists ? "✓" : "✗"} · settings.json {status.settingsJsonExists ? "✓" : "✗"}</div>
              <div className="k">Source handler</div><div className="v">{status.sourceHandler || "—"}</div>
            </div>
          )}
        </section>

        <section className="card">
          <div className="card-h">
            <div>
              <h2 className="card-title">Listener (Global Hotkeys)</h2>
              <p className="card-desc">Runs <span className="inline-code">nextgen-handler --listen</span>. Start-at-boot uses {isWindows ? "a per-user Run registry entry" : "an XDG autostart entry"}.</p>
            </div>
            <div className="row">
              {status?.listenRunning ? <Badge kind="good">Running{status.listenPids?.length ? ` (${status.listenPids.join(",")})` : ""}</Badge> : <Badge kind="neutral">Stopped</Badge>}
            </div>
          </div>
          <div className="stack">
            <div className="row-between">
              <div>
                <div style={{ fontWeight: 600 }}>Start listener at boot</div>
                <div className="muted small">{status?.autostartPath || (isWindows ? "HKCU\\...\\Run\\NextGENHandlerListen" : "~/.config/autostart/nextgen-handler-listen.desktop")}</div>
              </div>
              <Toggle checked={!!status?.autostartEnabled} disabled={!!busy} onChange={onAutostartToggle} />
            </div>
            <div className="row">
              <button className="btn" onClick={onStartListener} disabled={!!busy || !!status?.listenRunning}>Start now</button>
              <button className="btn" onClick={onStopListener} disabled={!!busy || !status?.listenRunning}>Stop</button>
              <button className="btn ghost" onClick={() => refresh()} disabled={!!busy}>Refresh</button>
            </div>
          </div>
        </section>

        <section className="card">
          <div className="card-h">
            <div>
              <h2 className="card-title">Capture Keys</h2>
              <p className="card-desc">Stops the listener while capturing, then restores it. Use captured key names in <span className="inline-code">keys.json</span> bindings.</p>
            </div>
            <div className="row">
              {status?.captureRunning ? <Badge kind="warn">Capturing</Badge> : <Badge kind="neutral">Idle</Badge>}
              {!status?.captureRunning ? (
                <button className="btn primary" onClick={onStartCapture} disabled={!!busy}>Start capture</button>
              ) : (
                <button className="btn danger" onClick={onStopCapture} disabled={!!busy}>Stop capture</button>
              )}
            </div>
          </div>
          <pre className="log">{captureLines.length ? captureLines.join("\n") : "No capture output yet. Click “Start capture” and press keys/buttons on your devices."}</pre>
          <div className="muted small" style={{ marginTop: 8 }}>Tip: bindings live in <span className="inline-code">{status?.configDir ? `${status.configDir}${isWindows ? "\\" : "/"}keys.json` : "keys.json"}</span> (e.g. <span className="inline-code">{'{"keys":["KEY_LEFTCTRL","KEY_LEFTMETA"],"action":"focus_nextweb"}'}</span>).</div>
        </section>

        <section className="card">
          <div className="card-h">
            <div>
              <h2 className="card-title">Stream Player (settings.json)</h2>
              <p className="card-desc">External player for <span className="inline-code">nextgen://play</span> streams. Select a detected player or enter a custom command.</p>
            </div>
            <button className="btn primary" onClick={onSaveSettings} disabled={!!busy}>Save player settings</button>
          </div>

          <div className="field" style={{ marginBottom: 12 }}>
            <label className="label">Detected players (click to apply preset)</label>
            <div className="row" style={{ gap: 8 }}>
              {playerPresets.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  className={`btn sm ${p.detected ? "" : "ghost"}`}
                  onClick={() => applyPreset(p)}
                  title={p.note || (p.detected ? "Detected" : "Not detected")}
                  style={!p.detected ? { opacity: 0.7 } : undefined}
                >
                  {p.label}{p.detected ? " ✓" : ""}
                </button>
              ))}
              {playerPresets.length === 0 && <span className="muted small">No presets loaded.</span>}
            </div>
            <div className="muted small" style={{ marginTop: 6 }}>✓ = detected on this system. You can still select/enter any player.</div>
          </div>

          <div className="form-grid">
            <div className="field">
              <label className="label">Player command / executable</label>
              <input className="input" value={playerCmd} onChange={(e) => setPlayerCmd(e.target.value)} placeholder="vlc" />
            </div>
            <div className="field">
              <label className="label">Title argument (optional, use {"{t}"} for title)</label>
              <input className="input" value={titleArg} onChange={(e) => setTitleArg(e.target.value)} placeholder="--meta-title={t}" />
            </div>
            <div className="field span-2">
              <label className="label">Additional player arguments (one per line, before stream URL)</label>
              <textarea className="textarea" style={{ minHeight: 70 }} value={playerArgs} onChange={(e) => setPlayerArgs(e.target.value)} placeholder={"(optional)"} />
            </div>
          </div>
          <div className="notice" style={{ marginTop: 10 }}>
            Current command: <span className="inline-code">{JSON.stringify(buildPlayerArray())}</span>
            {titleArg ? <> · titleArg: <span className="inline-code">{titleArg}</span></> : null}
          </div>
        </section>

        <section className="card">
          <div className="card-h">
            <div>
              <h2 className="card-title">Apps (apps.json)</h2>
              <p className="card-desc">Whitelist for <span className="inline-code">nextgen://app/&lt;id&gt;</span>. Command runs directly (no shell). In NextWEB’s Apps Menu, kind “App” with id <span className="inline-code">xbox</span> → <span className="inline-code">nextgen://app/xbox</span>.</p>
            </div>
            <div className="row">
              <button className="btn primary" onClick={openPicker} disabled={!!busy}>Browse installed apps…</button>
              <button className="btn ghost" onClick={resetAppForm} disabled={!!busy}>New app</button>
            </div>
          </div>

          <div className="app-item" style={{ marginBottom: 14 }}>
            <div className="app-id" style={{ marginBottom: 10 }}>{editId ? `Edit app: ${editId}` : "Add app"}</div>
            <div className="form-grid">
              <div className="field">
                <label className="label">App id (used as nextgen://app/&lt;id&gt;)</label>
                <input className="input" value={appId} onChange={(e) => setAppId(e.target.value)} placeholder="xbox" />
              </div>
              <div className="field">
                <label className="label">Command / executable</label>
                <input className="input" value={appCmd} onChange={(e) => setAppCmd(e.target.value)} placeholder={isWindows ? "C:\\\\Program Files\\\\App\\\\app.exe" : "/opt/vivaldi/vivaldi"} />
              </div>
              <div className="field span-2">
                <label className="label">Arguments (one per line)</label>
                <textarea className="textarea" value={appArgs} onChange={(e) => setAppArgs(e.target.value)} placeholder={"--profile-directory=Default\n--app-id=kanajofaghckoijdglhndcjjlemljefb"} />
              </div>
              <div className="field span-2">
                <label className="label">{isWindows ? "Import from app/shortcut path (optional)" : "Import from .desktop file (optional, fills name/id/command)"}</label>
                <div className="row">
                  <input className="input" style={{ flex: 1 }} value={desktopPath} onChange={(e) => setDesktopPath(e.target.value)} placeholder={isWindows ? "C:\\\\Users\\\\You\\\\AppData\\\\Roaming\\\\Microsoft\\\\Windows\\\\Start Menu\\\\Programs\\\\App.lnk" : "/home/you/.local/share/applications/vivaldi-....desktop"} />
                  <button className="btn" onClick={onParseDesktop} disabled={!!busy}>{isWindows ? "Parse path" : "Parse .desktop"}</button>
                </div>
              </div>
            </div>
            <div className="row" style={{ marginTop: 12 }}>
              <button className="btn primary" onClick={onSaveApp} disabled={!!busy}>Save to apps.json</button>
              {editId && <button className="btn" onClick={resetAppForm} disabled={!!busy}>Cancel edit</button>}
            </div>
            <div className="muted small" style={{ marginTop: 8 }}>Preview: <span className="inline-code">{JSON.stringify({ [appId.trim() || "<id>"]: buildCommand() })}</span></div>
          </div>

          <div className="list">
            {apps.length === 0 && <div className="notice">No app entries in apps.json yet.</div>}
            {apps.map((a) => (
              <div className="app-item" key={a.id}>
                <div className="app-item-h">
                  <div>
                    <div className="app-id">{a.id}</div>
                    <div className="cmd">{a.command.join(" ")}</div>
                    <div className="muted small" style={{ marginTop: 4 }}>NextWEB: <span className="inline-code">nextgen://app/{a.id}</span></div>
                  </div>
                  <div className="row">
                    <button className="btn sm" onClick={() => loadAppToForm(a)} disabled={!!busy}>Edit</button>
                    <button className="btn sm danger" onClick={() => onDeleteApp(a.id)} disabled={!!busy}>Remove</button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </section>
      </div>

      {pickerOpen && (
        <div className="picker-backdrop" onClick={() => setPickerOpen(false)}>
          <div className="picker-modal" onClick={(e) => e.stopPropagation()}>
            <div className="picker-head">
              <div>
                <h3 style={{ margin: 0, fontSize: 16 }}>Installed apps</h3>
                <p className="muted small" style={{ margin: "2px 0 0" }}>Select an app to fill in the command and arguments (you can edit before saving).</p>
              </div>
              <button className="btn ghost sm" onClick={() => setPickerOpen(false)}>Close</button>
            </div>
            <div style={{ padding: "0 14px 10px" }}>
              <input
                className="input"
                autoFocus
                value={pickerQuery}
                onChange={(e) => setPickerQuery(e.target.value)}
                placeholder="Search by name, command, or path…"
              />
            </div>
            <div className="picker-list">
              {pickerLoading && <div className="notice">Scanning installed applications…</div>}
              {!pickerLoading && installedApps
                .filter((a) => {
                  const q = pickerQuery.trim().toLowerCase();
                  if (!q) return true;
                  const hay = `${a.name || ""} ${a.path || ""} ${(a.command || []).join(" ")}`.toLowerCase();
                  return hay.includes(q);
                })
                .map((a) => (
                  <button
                    key={a.path}
                    type="button"
                    className="picker-item"
                    onClick={() => { resetAppForm(); applyInstalledApp(a); }}
                  >
                    <div className="picker-name">{a.name || a.suggestedId || "(unnamed)"}</div>
                    <div className="picker-cmd">{(a.command || []).join(" ")}</div>
                    <div className="picker-path">{a.path}</div>
                  </button>
                ))}
              {!pickerLoading && installedApps.length === 0 && <div className="notice">No applications found.</div>}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
