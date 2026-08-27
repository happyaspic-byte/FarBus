# Windows Client Setup

FarBus does not ship a Windows kernel driver. Use a signed USB/IP 1.1 client such as [usbip-win2](https://github.com/vadimgrn/usbip-win2) against the local FarBus proxy.

## Install

1. Create a restore point.
2. Install usbip-win2 from its official releases.
3. Build or copy `farbus-client.exe` (`cargo build --release -p farbus-client`).

## Connect

On the Linux host:

```bash
farbus-server --export-all --listen [::]:7420
```

On Windows:

```powershell
farbus discover
farbus --connect HOST:7420 pair <fingerprint>
farbus devices <fingerprint>
farbus attach <fingerprint> 1
```

Leave `farbus attach` running. It listens on `127.0.0.1:3240`.

Then attach with usbip-win2:

```powershell
usbip list --remote=127.0.0.1
usbip attach --remote=127.0.0.1 --busid=1-1.2
```

Do not point usbip-win2 at the remote FarBus TLS port. Raw USB/IP is loopback-only.

## Notes

- HID devices can inject keystrokes. Only export devices you trust.
- Isochronous devices (webcams, USB audio) are experimental.
- If attach fails after a network drop, rerun `farbus attach`; the client retries with exponential backoff.
