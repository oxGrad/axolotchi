# Axolotchi

A Tamagotchi-style axolotl that guards your LAN. It runs on a Raspberry Pi Zero W, scans the local network, tracks devices joining and leaving, and uses Slack (Block Kit) as its only UI. Think pwnagotchi: cute but functional.

Only scan networks you own or have permission to monitor.

## Hard constraints

- Target: Raspberry Pi Zero W, ARMv6 (`arm-unknown-linux-gnueabihf`), 512MB RAM, single core, SD card.
- Build with `cross`. Never assume compiling on the Pi.
- Rust + SQLite (`rusqlite` with `bundled`, WAL mode). No other DB.
- Slack via Socket Mode only (outbound websocket, no inbound ports). Use `rustls`, not OpenSSL.
- Single-threaded tokio runtime (`current_thread`). Keep RSS small.
- Protect the SD card: raw sightings live in an in-memory ring buffer. Only state changes are written to SQLite. Batch writes.
- Release profile: `opt-level = "s"`, `lto = true`, `panic = "abort"`, `strip = true`.

## Naming

| Thing | Name |
|---|---|
| Repo / product | `axolotchi` |
| Workspace crates | `axolotchi-core`, `axolotchi-net`, `axolotchi-store`, `axolotchi-slack`, `axolotchi-sprites` |
| Daemon binary (crate `axolotchid`) | `axolotchid` |
| systemd unit | `axolotchid.service` |
| Pi hostname / mDNS | `axolotchi` / `axolotchi.local` |
| mDNS service (optional, `mdns-sd`) | `_axolotchi._tcp` |
| Config | `/etc/axolotchi/config.toml` (env overrides: `AXOLOTCHI_*`) |
| Database | `/var/lib/axolotchi/axolotchi.db` |
| Default pet name | `Axo` |

Do not publish to crates.io. Crates reference each other by path. Ship binaries via GitHub Releases.

## Architecture

```
axolotchi-net --Sighting--> engine (axolotchi-core) --Effect--> store + slack
                                  ^                                 |
                                  +--------- Interaction <----------+
```

- `axolotchi-core`: pure logic, zero IO, no clock reads (timestamps are passed in). Presence state machine (`Present -> Missed(n) -> Gone`), mood, XP and evolution, achievements. Entry point is `step(state, event) -> (state, Vec<Effect>)`. Everything here must be unit tested on the dev machine.
- `axolotchi-net`: ARP sweep, `AF_PACKET` passive sniffer, DHCP and mDNS parsing. Ignore Axo's own MAC.
- `axolotchi-store`: rusqlite, migrations, repo functions, OUI loader.
- `axolotchi-slack`: Block Kit builders are pure `fn(&ViewModel) -> serde_json::Value`, snapshot tested with `insta`. Also Socket Mode routing and the render debouncer.
- `axolotchi-sprites`: six mood PNGs (sleepy, alert, lonely, curious, happy, content), uploaded once, file IDs stored in `kv`.
- `axolotchid`: wires everything with one `mpsc` channel into a single engine task.

Reference designs (schema, presence.rs, mood.rs, xp.rs, engine glue, Block Kit builders, interaction routing) are in `docs/reference/`. Treat them as the intended design. Improve them where you find bugs, but keep the architecture.

## Slack rules

- Ack every Socket Mode envelope immediately, then push work onto the engine channel. Never block the ack on SQLite.
- Debounce `views.publish` and `chat.update`: coalesce `Render` effects, at most one publish per few seconds.
- Limits: 100 blocks on Home, 3000 chars per text block, 10 elements per context block, 5 options per overflow, 150 chars per header.
- Always include a `text` fallback alongside `blocks`.
- Overflow menu value is at `action.selected_option.value`. Normalize before routing.
- Overflow `confirm` applies to the whole menu. Confirm "Forget" in a follow-up modal instead.
- Use `slack_file` image blocks so the Pi never needs a public URL.
- Slack tokens come from config or env. Never commit them.

## Milestones (one per session)

1. **Scaffold**: workspace, crate stubs, `cross` config (`Cross.toml`), CI running fmt, clippy, and tests, release profile.
2. **Core presence**: port `presence.rs` with tests (limit, regrow, passive rescue, IP change).
3. **Store**: schema and migrations, hydrate and persist presence, OUI import, ring buffer.
4. **Net**: ARP sweep plus `suspects()` unicast confirm, then the passive sniffer. Add a mock sighting source so everything upstream runs on a laptop.
5. **Game**: mood, XP, stages, achievements rule table, morph unlocks, Netdex data.
6. **Block Kit**: home, alert, device card, edit modal, Netdex modal, quiet hours modal. Snapshot tests.
7. **Socket Mode and engine loop**: interactions, slash commands (`/axo who|feed|stats|dex`), reactions as input, pinned live-tank message.
8. **Ops**: systemd unit with restart-on-failure, graceful shutdown flush, nightly prune and VACUUM, scanner heartbeat (Axo sleeps visibly if the scanner dies), README.

## Working agreements

- Plan first, code second. For each milestone, propose the plan and wait for approval.
- Keep `axolotchi-core` free of IO and clock reads. If a change needs either, it belongs in another crate.
- Run `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test` before calling a task done.
- Prefer small dependencies. Check binary size and RSS impact before adding a crate.
- Anything that needs real hardware (raw sockets, ARP on the actual LAN) must be clearly marked and separated from what can be tested locally.
