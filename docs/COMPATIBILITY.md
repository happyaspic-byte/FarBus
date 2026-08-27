# Compatibility

## Supported now

| Path | Status |
|---|---|
| Linux server, TLS 1.3 control/data plane | Supported |
| Linux physical USB via libusb (`rusb`, vendored) | Supported for control, bulk, interrupt |
| Linux sysfs inventory fallback | Supported |
| Simulated lab devices when no USB hardware is present | Supported |
| Windows/Linux USB/IP 1.1 client against `127.0.0.1:3240` | Supported (`usbip-win2`, Linux `usbip`) |
| IPv6-first dual stack with IPv4 fallback | Supported |
| PIN pairing, one-use PIN rotation, bearer tokens, exclusive leases | Supported |
| Authenticated device list / attach / detach / URB | Supported |
| Multi-interface composite USB devices (interface enumeration & endpoint routing) | Supported |
| USB/IP transfer length cap (65,536 bytes) | Supported |
| USB/IP UNLINK (structured RET_UNLINK; physical rusb cancel is timeout-bounded) | Supported |
| Automatic TLS reconnect and lease re-attach | Supported |

## Experimental

- Isochronous devices (webcams, USB audio)
- USB 3 UASP storage
- WAN / high-latency paths
- Timing-sensitive game controllers and security dongles

Loopback bulk benchmarks measure serialized URB round-trips over one TLS session. They are a transport ceiling, not physical USB or pipelined USB/IP throughput.

## Not supported

- Remote plaintext USB/IP
- Concurrent use of one physical device by multiple clients
- FarBus-authored Windows kernel driver
- macOS server/client
- Enterprise RBAC / 802.1X / fleet management

## USB/IP 1.1 coverage

Implemented: `OP_REQ_DEVLIST`, `OP_REP_DEVLIST`, `OP_REQ_IMPORT`, `OP_REP_IMPORT`, `USBIP_CMD_SUBMIT`, `USBIP_RET_SUBMIT`, `USBIP_CMD_UNLINK` / `USBIP_RET_UNLINK`.

Loopback only. Remote USB/IP clients must not connect to the TLS port.
