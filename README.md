# axolotchi

A Tamagotchi-style axolotl that guards your LAN. Runs on a Raspberry Pi Zero
W, scans the local network, and uses Slack (Block Kit) as its only UI.

Only scan networks you own or have permission to monitor.

## Status

Milestones 1-6 plus most of milestone 7: `axolotchid` connects to Slack over
Socket Mode, tracks device presence (`Present -> Missed(n) -> Gone`) and
persists it to SQLite, watches the network via either a scripted mock source
(the default, so the whole pipeline runs on a laptop) or a real ARP sweep
plus passive `AF_PACKET` sniffer behind the `hardware` feature, and reacts to
all of it with mood, XP and evolution stages, an achievements rule table, and
Netdex flavor text for known device vendors.

`/axo who|feed|stats` reply with real text, and `/axo dex` opens the Netdex
modal. Opening the Home tab (`app_home_opened`) publishes it live via
`views.publish`; from there the device overflow menu (Details/Rename/Forget)
opens the matching modal via `views.open`, the device card's own buttons
push a follow-up modal via `views.push`, and submitting a modal saves the
nickname or quiet hours setting to `kv` or deletes the device (Forget).
Every device that joins, misses a check-in, or goes `Gone` marks a `Render`
effect, which a debounced tick (at most once every few seconds) turns into a
fresh `views.publish` for everyone who's opened Home so far. Nicknames and
quiet hours live in the `kv` table; mood/XP/achievement/Netdex-discovery
state, and the set of users who've opened Home, are still in-memory only —
lost on restart, rebuilt as things happen again.

Not done yet: reactions-as-input, and the pinned "live tank" channel message
(a standing message kept current via `chat.update`, independent of anyone
opening their Home tab). See `CLAUDE.md` for the full milestone plan.

## Running locally

1. Create a Slack app with Socket Mode enabled, a bot token (`xoxb-...`),
   an app-level token with `connections:write` (`xapp-...`), and a
   `/axo` slash command. Also enable the **Home Tab** under App Home, and
   subscribe to the `app_home_opened` bot event under Event Subscriptions
   (Socket Mode delivers it, no request URL needed) — without that
   subscription, `axolotchid` never learns a Home tab was opened and so
   never publishes it.
2. Provide the tokens via environment variables (or `/etc/axolotchi/config.toml`,
   see below):

   ```sh
   export AXOLOTCHI_SLACK_BOT_TOKEN=xoxb-...
   export AXOLOTCHI_SLACK_APP_TOKEN=xapp-...
   cargo run -p axolotchid
   ```

3. In Slack, open the app's Home tab to see Axo's mood, stage, XP, and known
   devices, or run `/axo who`, `feed`, `stats`, or `dex`.

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
