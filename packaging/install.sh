#!/usr/bin/env bash
# One-time root setup for axolotchid: creates the unprivileged system user,
# the config/database directories, installs the binary and systemd unit,
# and enables the service to start on boot.
#
# The daemon itself never runs as root — this script needs sudo only
# because creating a system user, installing a unit under
# /etc/systemd/system, and enabling it for boot are root-only operations on
# any Linux system. Run it once per install/upgrade.
#
# Usage: sudo ./packaging/install.sh [path-to-axolotchid-binary]

set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
    echo "must be run as root (sudo ./packaging/install.sh)" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_SRC="${1:-${SCRIPT_DIR}/../target/arm-unknown-linux-gnueabihf/release/axolotchid}"

if [[ ! -f "${BINARY_SRC}" ]]; then
    echo "binary not found at ${BINARY_SRC}" >&2
    echo "build it first with: cross build --release --target arm-unknown-linux-gnueabihf -p axolotchid" >&2
    exit 1
fi

if ! id axolotchi >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin axolotchi
    echo "created system user 'axolotchi'"
fi

install -d -o axolotchi -g axolotchi -m 750 /etc/axolotchi
install -d -o axolotchi -g axolotchi -m 750 /var/lib/axolotchi

if [[ ! -f /etc/axolotchi/config.toml ]]; then
    install -o axolotchi -g axolotchi -m 600 "${SCRIPT_DIR}/config.toml.example" /etc/axolotchi/config.toml
    echo "wrote /etc/axolotchi/config.toml from the example - edit in your Slack tokens before starting"
fi

install -o root -g root -m 755 "${BINARY_SRC}" /usr/local/bin/axolotchid
install -o root -g root -m 644 "${SCRIPT_DIR}/axolotchid.service" /etc/systemd/system/axolotchid.service

systemctl daemon-reload
systemctl enable --now axolotchid.service

echo "axolotchid installed and started - check status with: systemctl status axolotchid"
