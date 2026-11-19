# Registers the nextgen:// URL scheme handler for the current user (Windows).
# No admin rights required: per-user HKCU registry registration + config templates.
param(
    [switch]$Quiet
)

$ErrorActionPreference = 'Stop'

function Write-Info($msg) { if (-not $Quiet) { Write-Host $msg } }

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$SourceHandler = Join-Path $ScriptDir 'nextgen-handler.py'
if (-not (Test-Path $SourceHandler)) {
    throw "Source handler not found: $SourceHandler"
}

$InstallDir = Join-Path $env:LOCALAPPDATA 'NextGEN Handler\bin'
$ConfigDir = Join-Path $env:APPDATA 'nextgen'
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null

$InstalledHandler = Join-Path $InstallDir 'nextgen-handler.py'
Copy-Item -Force $SourceHandler $InstalledHandler

# Launcher .cmd so registry command is simple and doesn't depend on PATH quirks.
$LauncherCmd = Join-Path $InstallDir 'nextgen-handler.cmd'
$launcherContent = @"
@echo off
setlocal
set "HANDLER=$InstalledHandler"
where py >nul 2>nul
if %ERRORLEVEL%==0 (
  py -3 "%HANDLER%" %*
  exit /b %ERRORLEVEL%
)
where python >nul 2>nul
if %ERRORLEVEL%==0 (
  python "%HANDLER%" %*
  exit /b %ERRORLEVEL%
)
where python3 >nul 2>nul
if %ERRORLEVEL%==0 (
  python3 "%HANDLER%" %*
  exit /b %ERRORLEVEL%
)
echo NextGEN Handler requires Python 3. Install Python and try again. 1>&2
exit /b 9009
"@
Set-Content -Path $LauncherCmd -Value $launcherContent -Encoding ASCII

# Register nextgen:// scheme (per-user, no admin)
$baseKey = 'HKCU:\Software\Classes\nextgen'
New-Item -Path $baseKey -Force | Out-Null
Set-ItemProperty -Path $baseKey -Name '(default)' -Value 'URL:NextGEN Protocol'
New-ItemProperty -Path $baseKey -Name 'URL Protocol' -Value '' -PropertyType String -Force | Out-Null
try {
    $iconKey = Join-Path $baseKey 'DefaultIcon'
    New-Item -Path $iconKey -Force | Out-Null
    Set-ItemProperty -Path $iconKey -Name '(default)' -Value "`"$LauncherCmd`",0"
} catch { }

$cmdKey = Join-Path $baseKey 'shell\open\command'
New-Item -Path $cmdKey -Force | Out-Null
# %1 is the full nextgen:// URL passed by Windows
Set-ItemProperty -Path $cmdKey -Name '(default)' -Value "`"$LauncherCmd`" `"%1`""

# First-run config templates (never overwrite existing settings)
$settingsPath = Join-Path $ConfigDir 'settings.json'
if (-not (Test-Path $settingsPath)) {
@'
{
  "player": ["vlc"],
  "titleArg": "--meta-title={t}"
}
'@ | Set-Content -Path $settingsPath -Encoding UTF8
}

$appsPath = Join-Path $ConfigDir 'apps.json'
if (-not (Test-Path $appsPath)) {
@'
{
  "_comment": "Add app ids here. Values are command lists executed directly (no shell).",
  "steam": ["steam", "-tenfoot"]
}
'@ | Set-Content -Path $appsPath -Encoding UTF8
}

$keysPath = Join-Path $ConfigDir 'keys.json'
if (-not (Test-Path $keysPath)) {
@'
{
  "_comment": "Global hotkeys for the --listen daemon. Use --capture to see key names for your system.",
  "bindings": [
    { "keys": ["KEY_LEFTCTRL", "KEY_LEFTMETA"], "action": "focus_nextweb" },
    { "keys": ["KEY_RIGHTCTRL", "KEY_LEFTMETA"], "action": "focus_nextweb" },
    { "keys": ["KEY_LEFTCTRL", "KEY_RIGHTMETA"], "action": "focus_nextweb" },
    { "keys": ["KEY_RIGHTCTRL", "KEY_RIGHTMETA"], "action": "focus_nextweb" }
  ],
  "remap": []
}
'@ | Set-Content -Path $keysPath -Encoding UTF8
}

Write-Info "Installed:"
Write-Info "  handler : $InstalledHandler"
Write-Info "  launcher: $LauncherCmd"
Write-Info "  scheme  : HKCU\Software\Classes\nextgen (x-scheme-handler/nextgen equivalent)"
Write-Info "  config  : $ConfigDir\\{settings,apps,keys}.json"
Write-Info ""
Write-Info "Self-test:"
Write-Info "  set NEXTGEN_DRY_RUN=1 && `"$LauncherCmd`" `"nextgen://play?url=https%3A%2F%2Fexample.com%2Fstream.m3u8&title=Test`""
Write-Info "  `"$LauncherCmd`" --capture   (prints key names, Ctrl+C to stop)"