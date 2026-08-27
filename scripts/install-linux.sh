#!/usr/bin/env bash
set -euo pipefail

prefix="${PREFIX:-/usr/local}"
script_dir="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

if [[ -x "$script_dir/farbus" && -x "$script_dir/farbus-server" && -x "$script_dir/farbus-bench" ]]; then
  binary_dir="$script_dir"
elif [[ -f "$repo_root/Cargo.toml" ]]; then
  cargo build --release --manifest-path "$repo_root/Cargo.toml" --workspace
  binary_dir="$repo_root/target/release"
else
  echo "FarBus binaries not found beside installer and no source workspace detected." >&2
  exit 2
fi

install -d "$prefix/bin"
install -m 0755 "$binary_dir/farbus-server" "$prefix/bin/farbus-server"
install -m 0755 "$binary_dir/farbus" "$prefix/bin/farbus"
install -m 0755 "$binary_dir/farbus-bench" "$prefix/bin/farbus-bench"

if [[ "${1:-}" == "--systemd" ]]; then
  if [[ "$prefix" != "/usr/local" ]]; then
    echo "--systemd requires PREFIX=/usr/local" >&2
    exit 2
  fi
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
  echo "Installed systemd unit farbus-server.service with no exported devices."
  echo "Edit ExecStart to add exact --export BUS-ID entries, then enable the service."
fi

echo "Installed FarBus to $prefix/bin"
echo "Start safely:   farbus-server --export BUS-ID"
echo "Discover:       farbus discover"
