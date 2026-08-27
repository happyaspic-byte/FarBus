# FarBus Windows client helper. Requires usbip-win2 from its official releases.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $root
try {
    cargo build --release -p farbus-client
    $out = Join-Path $root "target\release\farbus.exe"
    Write-Host "Built $out"
    Write-Host "Pair:    .\target\release\farbus.exe --connect HOST:7420 pair <fingerprint>"
    Write-Host "Attach:  .\target\release\farbus.exe attach <fingerprint> 1"
    Write-Host "Then:    usbip attach --remote=127.0.0.1 --busid=<busid>"
    Write-Host "Do not point usbip-win2 at the remote TLS port."
} finally {
    Pop-Location
}
