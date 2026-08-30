[CmdletBinding()]
param(
    [string]$Binary,
    [string]$GuiBinary,
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\FarBus"),
    [switch]$NoPath,
    [switch]$Uninstall,
    [switch]$PurgeSession
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Get-UserPath {
    [Environment]::GetEnvironmentVariable("Path", "User")
}

function Set-UserPath([string]$Value) {
    [Environment]::SetEnvironmentVariable("Path", $Value, "User")
}

function Update-UserPath([string]$Directory, [bool]$Remove) {
    $segments = @((Get-UserPath) -split ";" | Where-Object { $_ -and $_.Trim() })
    $filtered = @($segments | Where-Object {
        -not [string]::Equals($_.TrimEnd("\"), $Directory.TrimEnd("\"), [StringComparison]::OrdinalIgnoreCase)
    })
    if (-not $Remove) {
        $filtered += $Directory
    }
    Set-UserPath ($filtered -join ";")
}

if ($Uninstall) {
    if (-not $NoPath) {
        Update-UserPath $InstallDir $true
    }
    if (Test-Path $InstallDir) {
        Remove-Item -LiteralPath $InstallDir -Recurse -Force
    }
    if ($PurgeSession) {
        $sessionDir = Join-Path $env:USERPROFILE ".config\farbus"
        if (Test-Path $sessionDir) {
            Remove-Item -LiteralPath $sessionDir -Recurse -Force
        }
    }
    Write-Host "FarBus uninstalled from $InstallDir"
    exit 0
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir
$sourceRoot = if (Test-Path (Join-Path $scriptDir "LICENSE-MIT")) { $scriptDir } else { $repoRoot }
if (-not $Binary) {
    $packagedBinary = Join-Path $scriptDir "farbus.exe"
    $builtBinary = Join-Path $repoRoot "target\release\farbus.exe"
    $Binary = if (Test-Path $packagedBinary) { $packagedBinary } else { $builtBinary }
}
if (-not $GuiBinary) {
    $packagedGui = Join-Path $scriptDir "farbus-gui.exe"
    $builtGui = Join-Path $repoRoot "target\release\farbus-gui.exe"
    $GuiBinary = if (Test-Path $packagedGui) { $packagedGui } else { $builtGui }
}
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
    throw "FarBus binary not found: $Binary. Use the release ZIP or run cargo build --release -p farbus-client first."
}

New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
$destination = Join-Path $InstallDir "farbus.exe"
Copy-Item -LiteralPath $Binary -Destination $destination -Force
if (Test-Path -LiteralPath $GuiBinary -PathType Leaf) {
    Copy-Item -LiteralPath $GuiBinary -Destination (Join-Path $InstallDir "farbus-gui.exe") -Force
}
Copy-Item -LiteralPath (Join-Path $sourceRoot "LICENSE-MIT") -Destination $InstallDir -Force
Copy-Item -LiteralPath (Join-Path $sourceRoot "LICENSE-APACHE") -Destination $InstallDir -Force

if (-not $NoPath) {
    Update-UserPath $InstallDir $false
}

Write-Host "Installed FarBus to $destination"
if (Test-Path (Join-Path $InstallDir "farbus-gui.exe")) {
    Write-Host "GUI:    farbus-gui.exe"
}
if (Get-Command usbip.exe -ErrorAction SilentlyContinue) {
    Write-Host "usbip-win2 client detected."
} else {
    Write-Warning "usbip.exe not found. Install the signed usbip-win2 client separately."
}
Write-Host "Pair:   farbus --connect HOST:7420 pair <fingerprint>"
Write-Host "Attach: farbus attach 1"
Write-Host "FarBus runs in the foreground. This package does not register a Windows service or install a kernel driver."
Write-Host "Do not point usbip-win2 at the remote TLS port."
