# axolotchi

A Tamagotchi-style axolotl that guards your LAN. Runs on a Raspberry Pi Zero
W, scans the local network, and uses Slack (Block Kit) as its only UI.

Only scan networks you own or have permission to monitor.

## Status

Milestones 1-5 plus a minimal slice of milestone 7: `axolotchid` connects to
Slack over Socket Mode and answers a `/axo` slash command, tracks device
presence (`Present -> Missed(n) -> Gone`) and persists it to SQLite, watches
the network via either a scripted mock source (the default, so the whole
pipeline runs on a laptop) or a real ARP sweep plus passive `AF_PACKET`
sniffer behind the `hardware` feature, and reacts to all of it with mood, XP
and evolution stages, an achievements rule table, and Netdex flavor text for
known device vendors. Mood/XP/achievement state lives in memory only for
now; persisting it to SQLite is a follow-up. The Block Kit UI is still a
stub, since there is no view of any of this yet beyond log lines and the
plain-text slash command reply. See `CLAUDE.md` for the full milestone
plan.

## Running locally

1. Create a Slack app with Socket Mode enabled, a bot token (`xoxb-...`),
   an app-level token with `connections:write` (`xapp-...`), and a
   `/axo` slash command.
2. Provide the tokens via environment variables (or `/etc/axolotchi/config.toml`,
   see below):

   ```sh
   export AXOLOTCHI_SLACK_BOT_TOKEN=xoxb-...
   export AXOLOTCHI_SLACK_APP_TOKEN=xapp-...
   cargo run -p axolotchid
   ```

3. In Slack, run `/axo who` (or `feed`, `stats`, `dex`) — Axo replies with a
   placeholder message confirming the round trip works.

### Config file

`/etc/axolotchi/config.toml`:

```toml
slack_bot_token = "xoxb-..."
slack_app_token = "xapp-..."
pet_name = "Axo"
database_path = "/var/lib/axolotchi/axolotchi.db"
net_mode = "mock" # or "hardware"
```

Any `AXOLOTCHI_*` environment variable overrides the matching config file
value (`AXOLOTCHI_SLACK_BOT_TOKEN`, `AXOLOTCHI_DATABASE_PATH`,
`AXOLOTCHI_NET_MODE`, ...).

### Network modes

- **`mock`** (default): plays a short scripted scenario touching every
  presence transition (join, miss, passive rescue, IP change, gone) with no
  hardware involved — this is what you want for local development.
- **`hardware`**: a real ARP sweep plus passive `AF_PACKET` sniffing on a
  live interface. Needs `CAP_NET_RAW` (or root), a real LAN, and the binary
  built with `--features hardware` (see below). Also requires
  `AXOLOTCHI_INTERFACE`, `AXOLOTCHI_OUR_IP`, `AXOLOTCHI_SWEEP_NETWORK`, and
  `AXOLOTCHI_SWEEP_PREFIX` (e.g. `eth0`, `192.168.1.42`, `192.168.1.0`,
  `24`). This path hasn't been exercised against real hardware yet — see
  `crates/axolotchi-net/src/raw_socket.rs`.

## Development

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Cross-compiling for the Pi Zero W (ARMv6) uses [`cross`](https://github.com/cross-rs/cross):

```sh
cross build --release --target arm-unknown-linux-gnueabihf -p axolotchid
# with real network scanning:
cross build --release --target arm-unknown-linux-gnueabihf -p axolotchid --features hardware
```
