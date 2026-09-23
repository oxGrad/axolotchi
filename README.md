# axolotchi

A Tamagotchi-style axolotl that guards your LAN. Runs on a Raspberry Pi Zero
W, scans the local network, and uses Slack (Block Kit) as its only UI.

Only scan networks you own or have permission to monitor.

## Status

Milestone 1 (scaffold) plus a minimal slice of milestone 7: `axolotchid`
connects to Slack over Socket Mode and answers a `/axo` slash command.
Presence tracking, storage, real network scanning, and the game layer are
stubs — see `CLAUDE.md` for the full milestone plan.

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
```

Any `AXOLOTCHI_*` environment variable overrides the matching config file
value.

## Development

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Cross-compiling for the Pi Zero W (ARMv6) uses [`cross`](https://github.com/cross-rs/cross):

```sh
cross build --release --target arm-unknown-linux-gnueabihf -p axolotchid
```
