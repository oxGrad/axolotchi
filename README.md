# axolotchi

A Tamagotchi-style axolotl that guards your LAN. Runs on a Raspberry Pi Zero
W, scans the local network, and uses Slack (Block Kit) as its only UI.

Only scan networks you own or have permission to monitor.

## Status

All 8 milestones from `CLAUDE.md` are done. `axolotchid` connects to Slack over
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
work.

On the ops side, `axolotchid` shuts down cleanly on `SIGTERM` or Ctrl+C
(checkpointing the SQLite WAL before exit), prunes devices that have been
`Gone` for more than 30 days once a day (followed by a `VACUUM` if anything
was deleted), and tracks a scanner heartbeat: if the network source goes
quiet for more than 5 minutes, Axo is forced to `Sleepy` and an error is
logged, with a recovery log line once events resume. See
[Deploying to the Pi](#deploying-to-the-pi) below for the systemd unit that
drives restart-on-failure and boot startup. Mood/XP/achievement/Netdex
progress and the set of users who've opened Home are still in-memory only —
lost across a restart, rebuilt as things happen again; nicknames, quiet
hours, and device presence persist to SQLite.

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

## Deploying to the Pi

`axolotchid` runs as a systemd service (`packaging/axolotchid.service`) so it
restarts on crash and starts on boot without a login session. The daemon
process itself is never root: the unit runs it as an unprivileged
`axolotchi` system user, and hardware net mode's raw sockets are granted via
`AmbientCapabilities=CAP_NET_RAW` instead. A plain user-level (`--user`)
unit was considered instead, since it needs no `sudo` at all to run — but
without `loginctl enable-linger` (which the Pi shouldn't need just to run
this), it stops the moment nobody's logged in, which defeats "survives a
reboot". A system-level unit only needs root once, for installation.

1. Cross-compile the release binary (see above; add `--features hardware`
   for real scanning).
2. Copy the repo (or just `target/.../release/axolotchid` and the
   `packaging/` directory) onto the Pi.
3. Run the installer once, as root:

   ```sh
   sudo ./packaging/install.sh
   ```

   This creates the `axolotchi` system user, `/etc/axolotchi` and
   `/var/lib/axolotchi` (owned by that user), copies the binary to
   `/usr/local/bin/axolotchid`, writes `/etc/axolotchi/config.toml` from
   `packaging/config.toml.example` if one doesn't already exist, installs
   the unit file, and runs `systemctl enable --now axolotchid`.
4. Edit `/etc/axolotchi/config.toml` to add the real Slack tokens (and, for
   hardware mode, the interface/IP/sweep settings — see
   [Network modes](#network-modes) above), then
   `sudo systemctl restart axolotchid`.

Day to day:

```sh
sudo systemctl status axolotchid    # is it running, last few log lines
journalctl -u axolotchid -f         # follow logs live
sudo systemctl restart axolotchid   # after editing config.toml
```

A `SIGTERM` (what `systemctl stop`/`restart` sends) is handled gracefully:
`axolotchid` breaks its engine loop, checkpoints the SQLite WAL, and exits
0. A crash or non-zero exit gets restarted automatically after 5 seconds
(`Restart=on-failure`, capped at 5 restarts per 5 minutes so a persistently
broken build doesn't spin forever). To upgrade, re-run `install.sh` with the
new binary — it overwrites `/usr/local/bin/axolotchid` and the unit file but
leaves the existing config and database alone — then
`sudo systemctl restart axolotchid`.
