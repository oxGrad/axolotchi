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

With `AXOLOTCHI_HOME_CHANNEL` set, `axolotchid` also posts and pins a "live
tank" message in that channel (`chat.postMessage` + `pins.add`, once — its
channel/ts are saved in `kv` so a restart resumes updating the same message
instead of posting a new one), refreshed by the same debounced tick as the
Home tab via `chat.update`. Reacting to it with `:fish:` feeds Axo, the same
as `/axo feed` (this is "reactions as input" from the milestone plan) —
matched purely by comparing the reaction's channel/ts against the live
tank's, so reacting anywhere else does nothing. If posting it fails (no
`AXOLOTCHI_HOME_CHANNEL`, a bad channel, missing scopes), `axolotchid` logs
it and carries on without that feature; only the Home tab is required to
work. See `CLAUDE.md` for the full milestone plan — as of this milestone,
Slack's UI for axolotchi is essentially complete: what's left in the plan is
mostly the game layer's own persistence and the Ops milestone (systemd,
graceful shutdown, nightly prune).

## Running locally

1. Create a Slack app with Socket Mode enabled, a bot token (`xoxb-...`)
   with the `chat:write`, `pins:write`, and `reactions:read` scopes, an
   app-level token with `connections:write` (`xapp-...`), and a `/axo`
   slash command. Also enable the **Home Tab** under App Home, and
   subscribe to the `app_home_opened` and `reaction_added` bot events
   under Event Subscriptions (Socket Mode delivers both, no request URL
   needed) — without those subscriptions, `axolotchid` never learns a Home
   tab was opened or a reaction was added, so it can't publish or react to
   either.
2. Provide the tokens via environment variables (or `/etc/axolotchi/config.toml`,
   see below):

   ```sh
   export AXOLOTCHI_SLACK_BOT_TOKEN=xoxb-...
   export AXOLOTCHI_SLACK_APP_TOKEN=xapp-...
   cargo run -p axolotchid
   ```

3. In Slack, open the app's Home tab to see Axo's mood, stage, XP, and known
   devices, or run `/axo who`, `feed`, `stats`, or `dex`. Set
   `AXOLOTCHI_HOME_CHANNEL` (see below) to also get a pinned, live-updating
   message in a channel — invite the bot to that channel first.

### Config file

`/etc/axolotchi/config.toml`:

```toml
slack_bot_token = "xoxb-..."
slack_app_token = "xapp-..."
pet_name = "Axo"
database_path = "/var/lib/axolotchi/axolotchi.db"
net_mode = "mock" # or "hardware"
home_channel = "C0123456789" # optional: enables the pinned live tank message
```

Any `AXOLOTCHI_*` environment variable overrides the matching config file
value (`AXOLOTCHI_SLACK_BOT_TOKEN`, `AXOLOTCHI_DATABASE_PATH`,
`AXOLOTCHI_NET_MODE`, `AXOLOTCHI_HOME_CHANNEL`, ...).

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
