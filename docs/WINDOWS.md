# Windows Client Setup

FarBus does not ship a Windows kernel driver. Use a signed USB/IP 1.1 client such as [usbip-win2](https://github.com/vadimgrn/usbip-win2) against the local FarBus proxy.

## Install

1. Create a restore point before installing the third-party USB/IP driver.
2. Install usbip-win2 from its official release and verify its Windows signature.
3. Download the CI artifact `FarBus-Windows-x64-unsigned`, extract it, and run:

```powershell
powershell -ExecutionPolicy Bypass -File .\install-windows.ps1
```

The package installs `farbus.exe` and `farbus-gui.exe` per user under `%LOCALAPPDATA%\Programs\FarBus` and adds that directory to the user PATH. The package is not Authenticode-signed, does not install a kernel driver, and does not register a Windows service.

Source builds can pass a binary explicitly:

```powershell
cargo build --release -p farbus-client -p farbus-gui
.\scripts\install-windows.ps1 -Binary .\target\release\farbus.exe -GuiBinary .\target\release\farbus-gui.exe
```

Uninstall without deleting pairing sessions:

```powershell
.\install-windows.ps1 -Uninstall
```

Add `-PurgeSession` only when the saved bearer token should also be deleted.

## Connect

On the Linux host, export only the intended bus IDs:

```bash
farbus-server --export 1-1.2 --listen [::]:7420
```

On Windows, run `farbus-gui.exe`. LAN Scan uses UDP broadcast and does not cross Tailscale. Over Tailscale, add `ubuntu` or `100.x.x.x:7420`; the GUI reads the server fingerprint from TLS, then enter the 6-digit PIN and Attach. The PIN field never appears on the CLI and is not written to disk.

The CLI still works:

```powershell
farbus discover
farbus --connect HOST:7420 pair <fingerprint>
farbus devices
farbus attach 1
```

Leave Attach running (GUI or `farbus attach`). It opens a plaintext USB/IP listener only on a loopback address; non-loopback listeners are rejected.

Then attach with usbip-win2:

```powershell
usbip list --remote=127.0.0.1
usbip attach --remote=127.0.0.1 --busid=1-1.2
```

Do not point usbip-win2 at the remote FarBus TLS port.

## Notes

- HID devices can inject keystrokes. Export only trusted devices by exact bus ID.
- Isochronous physical devices such as webcams and USB audio are unsupported.
- If the TLS session drops, FarBus reconnects, reattaches the same device, and retries the in-flight URB once. Persistent `Unauthorized` or lease-conflict errors still require `farbus pair` or `farbus attach` again.
- Windows bearer-token storage is not DPAPI-protected yet. Protect the user profile and do not share `%USERPROFILE%\.config\farbus`.
