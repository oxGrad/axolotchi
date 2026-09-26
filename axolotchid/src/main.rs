//! Wires the engine loop with three mpsc channels feeding one engine task:
//! net sightings come in from `axolotchi-net` (mock or real hardware),
//! slash command invocations and Socket Mode interactions (block actions,
//! view submissions) come from `axolotchi-slack`. Sightings and slash
//! commands run through `axolotchi_core::step`, with `Persist` effects
//! written to SQLite; interactions open/push Block Kit modals and, on
//! submit, save a nickname, forget a device, or save quiet hours — none of
//! which touch the engine's `step()` directly, since they're Slack-side
//! bookkeeping (nicknames, quiet hours) or a direct store deletion
//! (forget), not presence/game state.
//!
//! Single-threaded tokio runtime, per the Pi Zero W's single core.

use axolotchi_core::{Effect, Event, Mood, Presence, SlashCommand, State};
use axolotchi_slack::blocks::{
    DeviceCardViewModel, DeviceSummary, EditModalViewModel, ForgetConfirmViewModel, HomeViewModel,
    NetdexEntryView, NetdexModalViewModel,
};
use axolotchi_slack::{BlockAction, Interaction, ViewSubmission};
use axolotchi_store::Connection;
use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::time::Duration;

/// How often a pending `Render` effect gets flushed to everyone's Home tab
/// (and the live-tank message, if configured). Coalesces bursts of Render
/// effects into at most one publish per window, per the Slack rules in
/// `CLAUDE.md`.
const RENDER_DEBOUNCE: Duration = Duration::from_secs(3);

/// Reacting with this emoji on the live-tank message feeds Axo — the
/// "reactions as input" mechanism from milestone 7's plan.
const FEED_EMOJI: &str = "fish";

const KV_LIVE_TANK_CHANNEL: &str = "live_tank_channel";
const KV_LIVE_TANK_TS: &str = "live_tank_ts";

/// How often the nightly housekeeping tick fires. It's a fixed interval
/// rather than "at midnight" — simpler, and the exact hour doesn't matter
/// for a job this cheap.
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Devices `Gone` for longer than this get pruned from SQLite.
const PRUNE_GONE_AFTER_SECS: i64 = 30 * 24 * 60 * 60;

/// How often the scanner heartbeat is checked.
const HEARTBEAT_CHECK_INTERVAL: Duration = Duration::from_secs(60);
/// How long the net source can go quiet before it's considered stalled.
/// Generous relative to both the mock source's short demo script and
/// hardware mode's sweep interval, so normal quiet periods never trip it.
const HEARTBEAT_TIMEOUT_SECS: i64 = 5 * 60;

const CONFIG_PATH: &str = "/etc/axolotchi/config.toml";

fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetMode {
    Mock,
    Hardware,
}

#[derive(Debug, Clone)]
struct Config {
    slack_bot_token: String,
    slack_app_token: String,
    pet_name: String,
    database_path: String,
    net_mode: NetMode,
    /// Channel to post the pinned "live tank" message in. Optional: with
    /// none set, that feature is simply off — only the Home tab (via
    /// app_home_opened) shows live state.
    home_channel: Option<String>,
    // Only read by spawn_hardware_net_source, which is cfg'd out entirely
    // without the `hardware` feature.
    #[cfg_attr(not(feature = "hardware"), allow(dead_code))]
    interface: Option<String>,
    #[cfg_attr(not(feature = "hardware"), allow(dead_code))]
    our_ip: Option<Ipv4Addr>,
    #[cfg_attr(not(feature = "hardware"), allow(dead_code))]
    sweep_network: Option<Ipv4Addr>,
    #[cfg_attr(not(feature = "hardware"), allow(dead_code))]
    sweep_prefix_len: Option<u8>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct FileConfig {
    slack_bot_token: Option<String>,
    slack_app_token: Option<String>,
    pet_name: Option<String>,
    database_path: Option<String>,
    net_mode: Option<String>,
    home_channel: Option<String>,
    interface: Option<String>,
    our_ip: Option<String>,
    sweep_network: Option<String>,
    sweep_prefix_len: Option<u8>,
}

fn string_opt(env: &str, file: Option<String>) -> Option<String> {
    std::env::var(env).ok().or(file)
}

fn parse_ipv4_opt(env: &str, file: Option<String>) -> Result<Option<Ipv4Addr>, String> {
    string_opt(env, file)
        .map(|s| {
            s.parse::<Ipv4Addr>()
                .map_err(|e| format!("invalid {env}: {e}"))
        })
        .transpose()
}

fn load_config() -> Result<Config, String> {
    let file_config: FileConfig = match std::fs::read_to_string(CONFIG_PATH) {
        Ok(contents) => {
            toml::from_str(&contents).map_err(|e| format!("failed to parse {CONFIG_PATH}: {e}"))?
        }
        Err(_) => FileConfig::default(),
    };

    let slack_bot_token = string_opt("AXOLOTCHI_SLACK_BOT_TOKEN", file_config.slack_bot_token)
        .ok_or_else(|| {
            "missing slack bot token (set AXOLOTCHI_SLACK_BOT_TOKEN or slack_bot_token in config.toml)".to_string()
        })?;
    let slack_app_token = string_opt("AXOLOTCHI_SLACK_APP_TOKEN", file_config.slack_app_token)
        .ok_or_else(|| {
            "missing slack app token (set AXOLOTCHI_SLACK_APP_TOKEN or slack_app_token in config.toml)".to_string()
        })?;
    let pet_name =
        string_opt("AXOLOTCHI_PET_NAME", file_config.pet_name).unwrap_or_else(|| "Axo".to_string());
    let database_path = string_opt("AXOLOTCHI_DATABASE_PATH", file_config.database_path)
        .unwrap_or_else(|| "/var/lib/axolotchi/axolotchi.db".to_string());

    let net_mode_str = string_opt("AXOLOTCHI_NET_MODE", file_config.net_mode)
        .unwrap_or_else(|| "mock".to_string());
    let net_mode = match net_mode_str.as_str() {
        "mock" => NetMode::Mock,
        "hardware" => NetMode::Hardware,
        other => {
            return Err(format!(
                "invalid AXOLOTCHI_NET_MODE/net_mode: {other:?} (expected \"mock\" or \"hardware\")"
            ))
        }
    };

    let home_channel = string_opt("AXOLOTCHI_HOME_CHANNEL", file_config.home_channel);
    let interface = string_opt("AXOLOTCHI_INTERFACE", file_config.interface);
    let our_ip = parse_ipv4_opt("AXOLOTCHI_OUR_IP", file_config.our_ip)?;
    let sweep_network = parse_ipv4_opt("AXOLOTCHI_SWEEP_NETWORK", file_config.sweep_network)?;
    let sweep_prefix_len = std::env::var("AXOLOTCHI_SWEEP_PREFIX")
        .ok()
        .map(|s| {
            s.parse::<u8>()
                .map_err(|e| format!("invalid AXOLOTCHI_SWEEP_PREFIX: {e}"))
        })
        .transpose()?
        .or(file_config.sweep_prefix_len);

    if net_mode == NetMode::Hardware
        && (interface.is_none()
            || our_ip.is_none()
            || sweep_network.is_none()
            || sweep_prefix_len.is_none())
    {
        return Err(
            "hardware net mode requires AXOLOTCHI_INTERFACE, AXOLOTCHI_OUR_IP, AXOLOTCHI_SWEEP_NETWORK, and \
             AXOLOTCHI_SWEEP_PREFIX (or their config.toml equivalents)"
                .to_string(),
        );
    }

    Ok(Config {
        slack_bot_token,
        slack_app_token,
        pet_name,
        database_path,
        net_mode,
        home_channel,
        interface,
        our_ip,
        sweep_network,
        sweep_prefix_len,
    })
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = match load_config() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(%error, "failed to load config");
            std::process::exit(1);
        }
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    runtime.block_on(run(config));
}

/// Applies every `Effect::Persist` in `effects` by writing the current
/// in-memory device it names back to SQLite.
fn persist_effects(db: &Connection, state: &State, effects: &[Effect]) {
    for effect in effects {
        match effect {
            Effect::Persist { device_id } => {
                if let Some(device) = state.devices.get(device_id) {
                    if let Err(error) = axolotchi_store::upsert_device(db, device) {
                        tracing::warn!(%error, %device_id, "failed to persist device");
                    }
                }
            }
            // Mood, XP, achievements and Netdex progress aren't persisted
            // to SQLite yet (a follow-up once the store schema grows to
            // cover them) — logging keeps them visible in the meantime.
            Effect::Morph { stage } => tracing::info!(?stage, "axo morphed"),
            Effect::AchievementUnlocked(achievement) => {
                tracing::info!(name = achievement.name(), "achievement unlocked")
            }
            Effect::Render => {}
        }
    }
}

/// A device's nickname, from `kv`, falling back to its MAC when unset.
fn device_label(db: &Connection, device_id: &str) -> String {
    axolotchi_store::kv_get(db, &format!("nickname:{device_id}"))
        .ok()
        .flatten()
        .unwrap_or_else(|| device_id.to_string())
}

fn format_relative_time(now: i64, last_seen: i64) -> String {
    let diff = (now - last_seen).max(0);
    if diff < 60 {
        format!("{diff}s ago")
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86_400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86_400)
    }
}

fn device_card_vm(
    state: &State,
    db: &Connection,
    device_id: &str,
    now: i64,
) -> Option<DeviceCardViewModel> {
    let device = state.devices.get(device_id)?;
    let vendor = axolotchi_store::lookup_vendor(db, device_id).ok().flatten();
    let dex = axolotchi_core::dex_entry(vendor.as_deref());
    Some(DeviceCardViewModel {
        id: device_id.to_string(),
        label: device_label(db, device_id),
        ip: device.ip.clone(),
        presence: device.presence,
        last_seen_text: format_relative_time(now, device.last_seen),
        vendor,
        dex_flavor: dex.flavor.to_string(),
    })
}

fn edit_modal_vm(db: &Connection, device_id: &str) -> EditModalViewModel {
    EditModalViewModel {
        device_id: device_id.to_string(),
        current_label: device_label(db, device_id),
    }
}

fn forget_confirm_vm(db: &Connection, device_id: &str) -> ForgetConfirmViewModel {
    ForgetConfirmViewModel {
        device_id: device_id.to_string(),
        device_label: device_label(db, device_id),
    }
}

fn netdex_modal_vm(state: &State) -> NetdexModalViewModel {
    let entries = axolotchi_core::dex_entries()
        .iter()
        .map(|entry| NetdexEntryView {
            vendor: entry.vendor.to_string(),
            flavor: entry.flavor.to_string(),
            discovered: state.discovered_vendors.contains(entry.vendor),
        })
        .collect();
    NetdexModalViewModel { entries }
}

fn home_view_vm(state: &State, db: &Connection, pet_name: &str) -> HomeViewModel {
    let mut devices: Vec<DeviceSummary> = state
        .devices
        .values()
        .map(|device| {
            let vendor = axolotchi_store::lookup_vendor(db, &device.id)
                .ok()
                .flatten();
            let dex = axolotchi_core::dex_entry(vendor.as_deref());
            DeviceSummary {
                id: device.id.clone(),
                label: device_label(db, &device.id),
                presence: device.presence,
                vendor_flavor: Some(dex.flavor.to_string()),
            }
        })
        .collect();
    devices.sort_by(|a, b| a.label.cmp(&b.label));

    HomeViewModel {
        pet_name: pet_name.to_string(),
        mood: state.mood.mood,
        // Sprite upload-once + kv lookup lands in milestone 8; until then
        // the Home view falls back to a text-only mood line.
        mood_sprite_file_id: None,
        stage: state.xp.stage,
        xp: state.xp.xp,
        devices,
        achievements_unlocked: state.achievements.len(),
        achievements_total: axolotchi_core::achievement_count(),
    }
}

/// Publishes the Home tab for one user: on `app_home_opened`, and from the
/// debounced re-render after a `Render` effect.
async fn publish_home_for(
    client: &reqwest::Client,
    config: &Config,
    state: &State,
    db: &Connection,
    user_id: &str,
) {
    let view = axolotchi_slack::blocks::home_view(&home_view_vm(state, db, &config.pet_name));
    if let Err(error) =
        axolotchi_slack::views_publish(client, &config.slack_bot_token, user_id, view).await
    {
        tracing::warn!(%error, %user_id, "failed to publish home tab");
    }
}

/// Refreshes the pinned live-tank message in place via `chat.update`.
async fn refresh_live_tank(
    client: &reqwest::Client,
    config: &Config,
    state: &State,
    db: &Connection,
    channel: &str,
    ts: &str,
) {
    let message =
        axolotchi_slack::blocks::live_tank_message(&home_view_vm(state, db, &config.pet_name));
    if let Err(error) =
        axolotchi_slack::chat_update(client, &config.slack_bot_token, channel, ts, message).await
    {
        tracing::warn!(%error, %channel, %ts, "failed to update live tank message");
    }
}

/// Sets up the pinned live-tank message: resumes the one already posted
/// (its channel/ts saved in `kv`) if `config.home_channel` still matches,
/// otherwise posts and pins a fresh one. Returns `None` if the feature
/// isn't configured, or posting it fails (a network hiccup at startup
/// shouldn't crash the whole daemon — the Home tab still works).
async fn setup_live_tank(
    client: &reqwest::Client,
    config: &Config,
    state: &State,
    db: &Connection,
) -> Option<(String, String)> {
    let channel = config.home_channel.as_ref()?;

    let saved_channel = axolotchi_store::kv_get(db, KV_LIVE_TANK_CHANNEL)
        .ok()
        .flatten();
    let saved_ts = axolotchi_store::kv_get(db, KV_LIVE_TANK_TS).ok().flatten();
    if let (Some(saved_channel), Some(saved_ts)) = (saved_channel, saved_ts) {
        if &saved_channel == channel {
            tracing::info!(channel = %saved_channel, ts = %saved_ts, "resuming existing live tank message");
            return Some((saved_channel, saved_ts));
        }
    }

    let message =
        axolotchi_slack::blocks::live_tank_message(&home_view_vm(state, db, &config.pet_name));
    let (posted_channel, posted_ts) = match axolotchi_slack::chat_post_message(
        client,
        &config.slack_bot_token,
        channel,
        message,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            tracing::error!(%error, %channel, "failed to post live tank message, continuing without it");
            return None;
        }
    };

    if let Err(error) =
        axolotchi_slack::pins_add(client, &config.slack_bot_token, &posted_channel, &posted_ts)
            .await
    {
        tracing::warn!(%error, "failed to pin live tank message");
    }
    let _ = axolotchi_store::kv_set(db, KV_LIVE_TANK_CHANNEL, &posted_channel);
    let _ = axolotchi_store::kv_set(db, KV_LIVE_TANK_TS, &posted_ts);
    tracing::info!(channel = %posted_channel, ts = %posted_ts, "posted and pinned live tank message");
    Some((posted_channel, posted_ts))
}

fn effects_want_render(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::Render))
}

fn who_reply(state: &State, db: &Connection) -> String {
    if state.devices.is_empty() {
        return "Axo hasn't met anyone on the network yet.".to_string();
    }
    let mut lines: Vec<String> = state
        .devices
        .values()
        .map(|device| {
            let label = device_label(db, &device.id);
            let status = match device.presence {
                Presence::Present => "present".to_string(),
                Presence::Missed(n) => format!("missed {n} check-in(s)"),
                Presence::Gone => "gone".to_string(),
            };
            format!("\u{2022} {label}: {status}")
        })
        .collect();
    lines.sort();
    format!("Known devices:\n{}", lines.join("\n"))
}

fn feed_reply(config: &Config, state: &State, accepted: bool) -> String {
    if accepted {
        format!(
            "Nom nom! {} is feeling {:?} now. ({} XP, {:?})",
            config.pet_name, state.mood.mood, state.xp.xp, state.xp.stage
        )
    } else {
        format!(
            "{} isn't hungry yet \u{2014} try again later.",
            config.pet_name
        )
    }
}

fn stats_reply(config: &Config, state: &State) -> String {
    let present = state
        .devices
        .values()
        .filter(|d| d.presence == Presence::Present)
        .count();
    format!(
        "*{}*\nMood: {:?}\nStage: {:?}\nXP: {}\nDevices: {present}/{} present\nAchievements: {}/{}",
        config.pet_name,
        state.mood.mood,
        state.xp.stage,
        state.xp.xp,
        state.devices.len(),
        state.achievements.len(),
        axolotchi_core::achievement_count(),
    )
}

async fn respond_or_warn(client: &reqwest::Client, response_url: &str, text: &str) {
    if let Err(error) = axolotchi_slack::respond(client, response_url, text).await {
        tracing::warn!(%error, "failed to respond to slash command");
    }
}

/// Routes a `block_actions` interaction: the Home tab's per-device overflow
/// menu opens a fresh modal (`views.open`), while a button clicked from
/// inside `device_card_modal` pushes one onto that modal's stack
/// (`views.push`) instead.
async fn handle_block_action(
    client: &reqwest::Client,
    config: &Config,
    db: &Connection,
    state: &State,
    action: BlockAction,
) {
    let Some((verb, device_id)) = action.value.split_once(':') else {
        return;
    };
    let now = unix_now();
    let view = match verb {
        "details" => device_card_vm(state, db, device_id, now)
            .map(|vm| axolotchi_slack::blocks::device_card_modal(&vm)),
        "rename" => Some(axolotchi_slack::blocks::edit_modal(&edit_modal_vm(
            db, device_id,
        ))),
        "forget" => Some(axolotchi_slack::blocks::forget_confirm_modal(
            &forget_confirm_vm(db, device_id),
        )),
        _ => None,
    };
    let Some(view) = view else {
        return;
    };

    let result = match action.action_id.as_str() {
        "device_overflow" => {
            axolotchi_slack::views_open(client, &config.slack_bot_token, &action.trigger_id, view)
                .await
        }
        "device_card_rename" | "device_card_forget" => {
            axolotchi_slack::views_push(client, &config.slack_bot_token, &action.trigger_id, view)
                .await
        }
        _ => return,
    };
    if let Err(error) = result {
        tracing::warn!(%error, action_id = %action.action_id, "failed to open/push view");
    }
}

/// Handles a modal submit. Acked implicitly (see `socket_mode::handle_envelope`)
/// so the modal is already closing by the time this runs.
fn handle_view_submission(db: &Connection, state: &mut State, submission: ViewSubmission) {
    match submission.callback_id.as_str() {
        "edit_device" => {
            let device_id = &submission.private_metadata;
            if let Some(new_label) =
                axolotchi_slack::input_value(&submission.values, "label_block", "label_input")
            {
                if let Err(error) =
                    axolotchi_store::kv_set(db, &format!("nickname:{device_id}"), &new_label)
                {
                    tracing::warn!(%error, %device_id, "failed to save device nickname");
                }
            }
        }
        "forget_device_confirm" => {
            let device_id = &submission.private_metadata;
            if let Err(error) = axolotchi_store::delete_device(db, device_id) {
                tracing::warn!(%error, %device_id, "failed to delete device");
            }
            state.devices.remove(device_id);
        }
        "quiet_hours" => {
            let enabled = axolotchi_slack::checkbox_checked(
                &submission.values,
                "quiet_hours_enabled_block",
                "quiet_hours_enabled",
                "enabled",
            );
            let start = axolotchi_slack::timepicker_hour(
                &submission.values,
                "quiet_hours_start_block",
                "quiet_hours_start",
            );
            let end = axolotchi_slack::timepicker_hour(
                &submission.values,
                "quiet_hours_end_block",
                "quiet_hours_end",
            );
            if let Err(error) = axolotchi_store::kv_set(
                db,
                "quiet_hours_enabled",
                if enabled { "true" } else { "false" },
            ) {
                tracing::warn!(%error, "failed to save quiet_hours_enabled");
            }
            if let Some(start) = start {
                let _ = axolotchi_store::kv_set(db, "quiet_hours_start", &start.to_string());
            }
            if let Some(end) = end {
                let _ = axolotchi_store::kv_set(db, "quiet_hours_end", &end.to_string());
            }
        }
        other => tracing::warn!(callback_id = other, "unknown view submission"),
    }
}

fn spawn_net_source(config: &Config, tx: tokio::sync::mpsc::Sender<Event>) {
    match config.net_mode {
        NetMode::Mock => {
            tracing::info!("net mode: mock (demo script)");
            tokio::spawn(axolotchi_net::run_mock_source(
                axolotchi_net::demo_script(),
                tx,
            ));
        }
        NetMode::Hardware => spawn_hardware_net_source(config, tx),
    }
}

#[cfg(feature = "hardware")]
fn spawn_hardware_net_source(config: &Config, tx: tokio::sync::mpsc::Sender<Event>) {
    use std::time::Duration;

    let interface = config.interface.clone().expect("validated in load_config");
    let sweep_config = axolotchi_net::SweepConfig {
        our_ip: config.our_ip.expect("validated in load_config"),
        network: config.sweep_network.expect("validated in load_config"),
        prefix_len: config.sweep_prefix_len.expect("validated in load_config"),
        reply_window: Duration::from_millis(300),
        interval: Duration::from_secs(30),
    };

    let sweep_transport =
        axolotchi_net::RawSocketTransport::open(&interface).unwrap_or_else(|error| {
            tracing::error!(%error, interface = %interface, "failed to open sweep raw socket");
            std::process::exit(1);
        });
    let sniff_transport =
        axolotchi_net::RawSocketTransport::open(&interface).unwrap_or_else(|error| {
            tracing::error!(%error, interface = %interface, "failed to open sniffer raw socket");
            std::process::exit(1);
        });

    tracing::info!(interface = %interface, "net mode: hardware (ARP sweep + passive sniffer)");
    tokio::spawn(axolotchi_net::run_arp_sweep(
        sweep_transport,
        sweep_config,
        tx.clone(),
    ));
    tokio::spawn(axolotchi_net::run_passive_sniffer(sniff_transport, tx));
}

#[cfg(not(feature = "hardware"))]
fn spawn_hardware_net_source(_config: &Config, _tx: tokio::sync::mpsc::Sender<Event>) {
    tracing::error!(
        "hardware net mode requested but this binary was built without --features hardware"
    );
    std::process::exit(1);
}

async fn run(config: Config) {
    let db = axolotchi_store::open(&config.database_path).unwrap_or_else(|error| {
        tracing::error!(%error, path = %config.database_path, "failed to open database");
        std::process::exit(1);
    });
    if let Err(error) = axolotchi_store::import_oui(&db, axolotchi_store::SEED_OUI_CSV) {
        tracing::warn!(%error, "failed to import seed OUI data");
    }

    let devices = axolotchi_store::load_devices(&db).unwrap_or_else(|error| {
        tracing::warn!(%error, "failed to hydrate devices, starting with none known");
        Default::default()
    });
    let mut state = State {
        devices,
        ..State::default()
    };
    let mut recent_sightings: axolotchi_store::RingBuffer<axolotchi_store::RawSighting> =
        axolotchi_store::RingBuffer::new(256);

    let client = reqwest::Client::new();
    let live_tank = setup_live_tank(&client, &config, &state, &db).await;

    let (slash_tx, mut slash_rx) = tokio::sync::mpsc::channel(32);
    let (interaction_tx, mut interaction_rx) = tokio::sync::mpsc::channel(32);
    let slack_config = axolotchi_slack::Config {
        bot_token: config.slack_bot_token.clone(),
        app_token: config.slack_app_token.clone(),
    };
    tokio::spawn(async move {
        if let Err(error) = axolotchi_slack::run(slack_config, slash_tx, interaction_tx).await {
            tracing::error!(%error, "slack socket mode task ended");
        }
    });

    let (net_tx, mut net_rx) = tokio::sync::mpsc::channel(64);
    spawn_net_source(&config, net_tx);

    // Users who've opened the Home tab at least once — that's who the
    // debounced re-render republishes for. In-memory only: lost on
    // restart, rebuilt as each user reopens Home.
    let mut known_home_users: HashSet<String> = HashSet::new();
    let mut render_pending = false;
    let mut render_tick = tokio::time::interval(RENDER_DEBOUNCE);
    let mut maintenance_tick = tokio::time::interval(MAINTENANCE_INTERVAL);
    let mut heartbeat_tick = tokio::time::interval(HEARTBEAT_CHECK_INTERVAL);
    let mut last_net_event_at = unix_now();
    let mut scanner_stalled = false;
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to install SIGTERM handler");
    tracing::info!(pet_name = %config.pet_name, "axolotchid started");

    loop {
        tokio::select! {
            Some(event) = net_rx.recv() => {
                last_net_event_at = unix_now();
                if scanner_stalled {
                    scanner_stalled = false;
                    tracing::info!("scanner heartbeat recovered");
                }
                // Net never resolves a vendor itself; look it up here from
                // the OUI table so axolotchi-core (which can't touch SQLite)
                // still gets a resolved name for Netdex tracking.
                let event = match event {
                    Event::Sighting { device_id, ip, source, at, .. } => {
                        let vendor = axolotchi_store::lookup_vendor(&db, &device_id)
                            .unwrap_or(None);
                        recent_sightings.push(axolotchi_store::RawSighting {
                            device_id: device_id.clone(),
                            ip: ip.clone(),
                            source,
                            at,
                        });
                        Event::Sighting { device_id, ip, source, vendor, at }
                    }
                    other => other,
                };
                let (next_state, effects) = axolotchi_core::step(state, event);
                state = next_state;
                render_pending |= effects_want_render(&effects);
                persist_effects(&db, &state, &effects);
            }
            Some(invocation) = slash_rx.recv() => {
                match axolotchi_slack::parse_command(&invocation.text) {
                    Some(command) => {
                        let xp_before = state.xp.xp;
                        let (next_state, effects) =
                            axolotchi_core::step(state, Event::SlashCommand { command, at: unix_now() });
                        state = next_state;
                        render_pending |= effects_want_render(&effects);
                        persist_effects(&db, &state, &effects);

                        match command {
                            SlashCommand::Who => {
                                let text = who_reply(&state, &db);
                                respond_or_warn(&client, &invocation.response_url, &text).await;
                            }
                            SlashCommand::Feed => {
                                let accepted = state.xp.xp > xp_before;
                                let text = feed_reply(&config, &state, accepted);
                                respond_or_warn(&client, &invocation.response_url, &text).await;
                            }
                            SlashCommand::Stats => {
                                let text = stats_reply(&config, &state);
                                respond_or_warn(&client, &invocation.response_url, &text).await;
                            }
                            SlashCommand::Dex => {
                                let view = axolotchi_slack::blocks::netdex_modal(&netdex_modal_vm(&state));
                                if let Err(error) = axolotchi_slack::views_open(
                                    &client,
                                    &config.slack_bot_token,
                                    &invocation.trigger_id,
                                    view,
                                )
                                .await
                                {
                                    tracing::warn!(%error, "failed to open netdex modal");
                                    respond_or_warn(&client, &invocation.response_url, "Couldn't open the Netdex, try again.").await;
                                }
                            }
                        }
                    }
                    None => {
                        respond_or_warn(&client, &invocation.response_url, "Usage: `/axo who|feed|stats|dex`").await;
                    }
                }
            }
            Some(interaction) = interaction_rx.recv() => {
                match interaction {
                    Interaction::BlockAction(action) => {
                        handle_block_action(&client, &config, &db, &state, action).await;
                    }
                    Interaction::ViewSubmission(submission) => {
                        handle_view_submission(&db, &mut state, submission);
                    }
                    Interaction::HomeOpened { user_id } => {
                        known_home_users.insert(user_id.clone());
                        publish_home_for(&client, &config, &state, &db, &user_id).await;
                    }
                    Interaction::Reaction(reaction) => {
                        let is_feed_on_live_tank = reaction.emoji == FEED_EMOJI
                            && live_tank.as_ref().is_some_and(|(channel, ts)| {
                                *channel == reaction.channel && *ts == reaction.ts
                            });
                        if is_feed_on_live_tank {
                            let (next_state, effects) = axolotchi_core::step(
                                state,
                                Event::SlashCommand { command: SlashCommand::Feed, at: unix_now() },
                            );
                            state = next_state;
                            render_pending |= effects_want_render(&effects);
                            persist_effects(&db, &state, &effects);
                        }
                    }
                }
            }
            _ = render_tick.tick() => {
                if render_pending {
                    render_pending = false;
                    for user_id in known_home_users.clone() {
                        publish_home_for(&client, &config, &state, &db, &user_id).await;
                    }
                    if let Some((channel, ts)) = &live_tank {
                        refresh_live_tank(&client, &config, &state, &db, channel, ts).await;
                    }
                }
            }
            _ = maintenance_tick.tick() => {
                let now = unix_now();
                match axolotchi_store::prune_gone_devices(&db, now, PRUNE_GONE_AFTER_SECS) {
                    Ok(0) => {}
                    Ok(deleted) => {
                        tracing::info!(deleted, "pruned long-gone devices");
                        if let Err(error) = axolotchi_store::vacuum(&db) {
                            tracing::warn!(%error, "failed to vacuum database after pruning");
                        }
                    }
                    Err(error) => tracing::warn!(%error, "failed to prune gone devices"),
                }
            }
            _ = heartbeat_tick.tick() => {
                let now = unix_now();
                if now - last_net_event_at > HEARTBEAT_TIMEOUT_SECS && !scanner_stalled {
                    scanner_stalled = true;
                    tracing::error!(
                        seconds_quiet = now - last_net_event_at,
                        "scanner heartbeat missed, network watcher appears stalled"
                    );
                    state.mood.mood = Mood::Sleepy;
                    render_pending = true;
                }
            }
            _ = sigterm.recv() => {
                tracing::info!("received SIGTERM, shutting down gracefully");
                break;
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received Ctrl+C, shutting down gracefully");
                break;
            }
            else => {
                tracing::error!("all slack and net channels closed, shutting down");
                break;
            }
        }
    }

    if let Err(error) = axolotchi_store::checkpoint(&db) {
        tracing::warn!(%error, "failed to checkpoint WAL on shutdown");
    }
    tracing::info!("axolotchid stopped");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axolotchi_core::{Device, SightingSource};

    fn test_config() -> Config {
        Config {
            slack_bot_token: "xoxb-test".into(),
            slack_app_token: "xapp-test".into(),
            pet_name: "Axo".into(),
            database_path: ":memory:".into(),
            net_mode: NetMode::Mock,
            home_channel: None,
            interface: None,
            our_ip: None,
            sweep_network: None,
            sweep_prefix_len: None,
        }
    }

    fn test_db() -> Connection {
        axolotchi_store::open(":memory:").unwrap()
    }

    fn state_with_device(id: &str, presence: Presence) -> State {
        let mut state = State::default();
        state.devices.insert(
            id.to_string(),
            Device::hydrate(
                id.to_string(),
                Some("10.0.0.5".into()),
                presence,
                1000,
                SightingSource::Active,
            ),
        );
        state
    }

    #[test]
    fn relative_time_buckets_are_human_sized() {
        assert_eq!(format_relative_time(1005, 1000), "5s ago");
        assert_eq!(format_relative_time(1000 + 300, 1000), "5m ago");
        assert_eq!(format_relative_time(1000 + 7200, 1000), "2h ago");
        assert_eq!(format_relative_time(1000 + 172_800, 1000), "2d ago");
    }

    #[test]
    fn device_label_falls_back_to_id_until_a_nickname_is_set() {
        let db = test_db();
        assert_eq!(device_label(&db, "aa:bb:cc"), "aa:bb:cc");
        axolotchi_store::kv_set(&db, "nickname:aa:bb:cc", "Kitchen Pi").unwrap();
        assert_eq!(device_label(&db, "aa:bb:cc"), "Kitchen Pi");
    }

    #[test]
    fn who_reply_lists_known_devices() {
        let db = test_db();
        let state = state_with_device("aa:bb:cc", Presence::Present);
        let reply = who_reply(&state, &db);
        assert!(reply.contains("aa:bb:cc"));
        assert!(reply.contains("present"));
    }

    #[test]
    fn who_reply_on_empty_state_says_so() {
        let db = test_db();
        let reply = who_reply(&State::default(), &db);
        assert!(reply.contains("hasn't met anyone"));
    }

    #[test]
    fn feed_reply_reflects_acceptance() {
        let config = test_config();
        let state = State::default();
        assert!(feed_reply(&config, &state, true).starts_with("Nom nom"));
        assert!(feed_reply(&config, &state, false).contains("isn't hungry"));
    }

    #[test]
    fn stats_reply_includes_pet_name_and_counts() {
        let config = test_config();
        let state = state_with_device("aa:bb:cc", Presence::Present);
        let reply = stats_reply(&config, &state);
        assert!(reply.contains("Axo"));
        assert!(reply.contains("1/1 present"));
        assert!(reply.contains(&format!("0/{}", axolotchi_core::achievement_count())));
    }

    #[test]
    fn device_card_vm_builds_from_known_device() {
        let db = test_db();
        let state = state_with_device("aa:bb:cc", Presence::Present);
        let vm = device_card_vm(&state, &db, "aa:bb:cc", 1010).unwrap();
        assert_eq!(vm.id, "aa:bb:cc");
        assert_eq!(vm.ip.as_deref(), Some("10.0.0.5"));
        assert_eq!(vm.last_seen_text, "10s ago");
    }

    #[test]
    fn device_card_vm_is_none_for_unknown_device() {
        let db = test_db();
        assert!(device_card_vm(&State::default(), &db, "does-not-exist", 0).is_none());
    }

    #[test]
    fn effects_want_render_detects_render_effect() {
        assert!(effects_want_render(&[Effect::Render]));
        assert!(!effects_want_render(&[Effect::Persist {
            device_id: "dev-1".into()
        }]));
        assert!(!effects_want_render(&[]));
    }

    #[test]
    fn home_view_vm_reflects_state() {
        let db = test_db();
        axolotchi_store::kv_set(&db, "nickname:aa:bb:cc", "Kitchen Pi").unwrap();
        let state = state_with_device("aa:bb:cc", Presence::Present);
        let vm = home_view_vm(&state, &db, "Axo");
        assert_eq!(vm.pet_name, "Axo");
        assert_eq!(vm.devices.len(), 1);
        assert_eq!(vm.devices[0].label, "Kitchen Pi");
        assert_eq!(vm.achievements_total, axolotchi_core::achievement_count());
        assert!(vm.mood_sprite_file_id.is_none());
    }

    #[test]
    fn netdex_modal_vm_marks_discovered_vendors() {
        let mut state = State::default();
        state
            .discovered_vendors
            .insert("Espressif Inc.".to_string());
        let vm = netdex_modal_vm(&state);
        let espressif = vm
            .entries
            .iter()
            .find(|e| e.vendor == "Espressif Inc.")
            .unwrap();
        assert!(espressif.discovered);
        let others_undiscovered = vm
            .entries
            .iter()
            .filter(|e| e.vendor != "Espressif Inc.")
            .all(|e| !e.discovered);
        assert!(others_undiscovered);
    }

    #[test]
    fn view_submission_edit_device_saves_nickname() {
        let db = test_db();
        let mut state = State::default();
        let submission = ViewSubmission {
            callback_id: "edit_device".into(),
            private_metadata: "aa:bb:cc".into(),
            view_id: "V1".into(),
            user_id: "U1".into(),
            values: serde_json::json!({ "label_block": { "label_input": { "value": "Kitchen Pi" } } }),
        };
        handle_view_submission(&db, &mut state, submission);
        assert_eq!(device_label(&db, "aa:bb:cc"), "Kitchen Pi");
    }

    #[test]
    fn view_submission_forget_device_removes_it() {
        let db = test_db();
        let device = Device::hydrate(
            "aa:bb:cc".into(),
            None,
            Presence::Present,
            0,
            SightingSource::Active,
        );
        axolotchi_store::upsert_device(&db, &device).unwrap();
        let mut state = state_with_device("aa:bb:cc", Presence::Present);

        let submission = ViewSubmission {
            callback_id: "forget_device_confirm".into(),
            private_metadata: "aa:bb:cc".into(),
            view_id: "V1".into(),
            user_id: "U1".into(),
            values: serde_json::Value::Null,
        };
        handle_view_submission(&db, &mut state, submission);

        assert!(!state.devices.contains_key("aa:bb:cc"));
        assert!(axolotchi_store::load_devices(&db).unwrap().is_empty());
    }

    #[test]
    fn view_submission_quiet_hours_saves_all_fields() {
        let db = test_db();
        let mut state = State::default();
        let submission = ViewSubmission {
            callback_id: "quiet_hours".into(),
            private_metadata: String::new(),
            view_id: "V1".into(),
            user_id: "U1".into(),
            values: serde_json::json!({
                "quiet_hours_enabled_block": { "quiet_hours_enabled": { "selected_options": [{ "value": "enabled" }] } },
                "quiet_hours_start_block": { "quiet_hours_start": { "selected_time": "22:00" } },
                "quiet_hours_end_block": { "quiet_hours_end": { "selected_time": "07:00" } },
            }),
        };
        handle_view_submission(&db, &mut state, submission);

        assert_eq!(
            axolotchi_store::kv_get(&db, "quiet_hours_enabled")
                .unwrap()
                .as_deref(),
            Some("true")
        );
        assert_eq!(
            axolotchi_store::kv_get(&db, "quiet_hours_start")
                .unwrap()
                .as_deref(),
            Some("22")
        );
        assert_eq!(
            axolotchi_store::kv_get(&db, "quiet_hours_end")
                .unwrap()
                .as_deref(),
            Some("7")
        );
    }

    #[tokio::test]
    async fn setup_live_tank_is_none_when_not_configured() {
        let db = test_db();
        let config = test_config(); // home_channel: None
        let client = reqwest::Client::new();
        assert_eq!(
            setup_live_tank(&client, &config, &State::default(), &db).await,
            None
        );
    }

    #[tokio::test]
    async fn setup_live_tank_resumes_existing_message_from_kv() {
        let db = test_db();
        let config = Config {
            home_channel: Some("C1".into()),
            ..test_config()
        };
        axolotchi_store::kv_set(&db, KV_LIVE_TANK_CHANNEL, "C1").unwrap();
        axolotchi_store::kv_set(&db, KV_LIVE_TANK_TS, "123.456").unwrap();
        let client = reqwest::Client::new();
        let result = setup_live_tank(&client, &config, &State::default(), &db).await;
        assert_eq!(result, Some(("C1".to_string(), "123.456".to_string())));
    }
}
