//! Wires the engine loop with two mpsc channels feeding one engine task:
//! net sightings come in from `axolotchi-net` (mock or real hardware) and
//! slash command invocations come from `axolotchi-slack`'s Socket Mode
//! client. Both get run through `axolotchi_core::step`, with `Persist`
//! effects written to SQLite and a reply going back out over Slack's
//! `response_url` for slash commands.
//!
//! Single-threaded tokio runtime, per the Pi Zero W's single core.

use axolotchi_core::{Effect, Event, State};
use axolotchi_store::Connection;
use std::net::Ipv4Addr;

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

    let (slash_tx, mut slash_rx) = tokio::sync::mpsc::channel(32);
    let slack_config = axolotchi_slack::Config {
        bot_token: config.slack_bot_token.clone(),
        app_token: config.slack_app_token.clone(),
    };
    tokio::spawn(async move {
        if let Err(error) = axolotchi_slack::run(slack_config, slash_tx).await {
            tracing::error!(%error, "slack socket mode task ended");
        }
    });

    let (net_tx, mut net_rx) = tokio::sync::mpsc::channel(64);
    spawn_net_source(&config, net_tx);

    let client = reqwest::Client::new();
    tracing::info!(pet_name = %config.pet_name, "axolotchid started");

    loop {
        tokio::select! {
            Some(event) = net_rx.recv() => {
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
                persist_effects(&db, &state, &effects);
            }
            Some(invocation) = slash_rx.recv() => {
                let text = match axolotchi_slack::parse_command(&invocation.text) {
                    Some(command) => {
                        let (next_state, effects) =
                            axolotchi_core::step(state, Event::SlashCommand { command, at: unix_now() });
                        state = next_state;
                        persist_effects(&db, &state, &effects);
                        format!(
                            "{} got your `/axo {}` — full replies are still being built.",
                            config.pet_name,
                            invocation.text.trim()
                        )
                    }
                    None => "Usage: `/axo who|feed|stats|dex`".to_string(),
                };

                if let Err(error) =
                    axolotchi_slack::respond(&client, &invocation.response_url, &text).await
                {
                    tracing::warn!(%error, user_id = %invocation.user_id, "failed to respond to slash command");
                }
            }
            else => {
                tracing::error!("both net and slack channels closed, shutting down");
                break;
            }
        }
    }
}
