#!/usr/bin/env bash
set -euo pipefail

prefix="${PREFIX:-/usr/local}"
root="$(cd "$(dirname "$0")/.." && pwd)"

cargo build --release --manifest-path "$root/Cargo.toml" --workspace
install -d "$prefix/bin"
install -m 0755 "$root/target/release/farbus-server" "$prefix/bin/farbus-server"
install -m 0755 "$root/target/release/farbus-client" "$prefix/bin/farbus"
install -m 0755 "$root/target/release/farbus-bench" "$prefix/bin/farbus-bench"

if [[ "${1:-}" == "--systemd" ]]; then
  install -d /etc/systemd/system
  cat >/etc/systemd/system/farbus-server.service <<'UNIT'
[Unit]
Description=FarBus USB export server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/farbus-server --listen [::]:7420
Restart=on-failure
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
UNIT
  systemctl daemon-reload
  echo "Installed systemd unit farbus-server.service"
  echo "Enable sharing with: systemctl enable --now farbus-server"
  echo "Physical USB export requires --export-all and typically root/udev access."
fi

echo "Installed FarBus to $prefix/bin"
echo "Start a server: farbus-server --export-all"
echo "Discover:       farbus discover"
