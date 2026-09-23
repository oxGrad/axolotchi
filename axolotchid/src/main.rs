//! Wires the engine loop with a single mpsc channel: slash command
//! invocations come in from `axolotchi-slack`'s Socket Mode client, get run
//! through `axolotchi-core::step`, and a reply goes back out over Slack's
//! `response_url`.
//!
//! Single-threaded tokio runtime, per the Pi Zero W's single core.

use axolotchi_core::{Event, State};

const CONFIG_PATH: &str = "/etc/axolotchi/config.toml";

#[derive(Debug, Clone)]
struct Config {
    slack_bot_token: String,
    slack_app_token: String,
    pet_name: String,
}

#[derive(Debug, Default, serde::Deserialize)]
struct FileConfig {
    slack_bot_token: Option<String>,
    slack_app_token: Option<String>,
    pet_name: Option<String>,
}

fn load_config() -> Result<Config, String> {
    let file_config: FileConfig = match std::fs::read_to_string(CONFIG_PATH) {
        Ok(contents) => {
            toml::from_str(&contents).map_err(|e| format!("failed to parse {CONFIG_PATH}: {e}"))?
        }
        Err(_) => FileConfig::default(),
    };

    let slack_bot_token = std::env::var("AXOLOTCHI_SLACK_BOT_TOKEN")
        .ok()
        .or(file_config.slack_bot_token)
        .ok_or_else(|| {
            "missing slack bot token (set AXOLOTCHI_SLACK_BOT_TOKEN or slack_bot_token in config.toml)".to_string()
        })?;

    let slack_app_token = std::env::var("AXOLOTCHI_SLACK_APP_TOKEN")
        .ok()
        .or(file_config.slack_app_token)
        .ok_or_else(|| {
            "missing slack app token (set AXOLOTCHI_SLACK_APP_TOKEN or slack_app_token in config.toml)".to_string()
        })?;

    let pet_name = std::env::var("AXOLOTCHI_PET_NAME")
        .ok()
        .or(file_config.pet_name)
        .unwrap_or_else(|| "Axo".to_string());

    Ok(Config {
        slack_bot_token,
        slack_app_token,
        pet_name,
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

async fn run(config: Config) {
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);

    let slack_config = axolotchi_slack::Config {
        bot_token: config.slack_bot_token.clone(),
        app_token: config.slack_app_token.clone(),
    };
    tokio::spawn(async move {
        if let Err(error) = axolotchi_slack::run(slack_config, tx).await {
            tracing::error!(%error, "slack socket mode task ended");
        }
    });

    let client = reqwest::Client::new();
    let mut state = State::default();

    tracing::info!(pet_name = %config.pet_name, "axolotchid started, waiting for slash commands");

    while let Some(invocation) = rx.recv().await {
        let text = match axolotchi_slack::parse_command(&invocation.text) {
            Some(command) => {
                let (next_state, _effects) =
                    axolotchi_core::step(state, Event::SlashCommand { command });
                state = next_state;
                format!(
                    "{} got your `/axo {}` — full replies are still being built.",
                    config.pet_name,
                    invocation.text.trim()
                )
            }
            None => "Usage: `/axo who|feed|stats|dex`".to_string(),
        };

        if let Err(error) = axolotchi_slack::respond(&client, &invocation.response_url, &text).await
        {
            tracing::warn!(%error, user_id = %invocation.user_id, "failed to respond to slash command");
        }
    }
}
