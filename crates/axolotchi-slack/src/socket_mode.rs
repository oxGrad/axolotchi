use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

const CONNECTIONS_OPEN_URL: &str = "https://slack.com/api/apps.connections.open";
const RECONNECT_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct Config {
    pub bot_token: String,
    pub app_token: String,
}

#[derive(Debug, Clone)]
pub struct SlashCommandInvocation {
    pub command: String,
    pub text: String,
    pub user_id: String,
    pub channel_id: String,
    pub response_url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SlackError {
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("slack api error: {0}")]
    Api(String),
}

#[derive(Debug, Deserialize)]
struct SlashCommandPayload {
    command: String,
    text: String,
    user_id: String,
    channel_id: String,
    response_url: String,
}

impl From<SlashCommandPayload> for SlashCommandInvocation {
    fn from(payload: SlashCommandPayload) -> Self {
        Self {
            command: payload.command,
            text: payload.text,
            user_id: payload.user_id,
            channel_id: payload.channel_id,
            response_url: payload.response_url,
        }
    }
}

/// Runs the Socket Mode connection forever, reconnecting on any error or
/// clean close. Each slash command invocation is pushed onto `tx` after its
/// envelope has already been acked.
pub async fn run(
    config: Config,
    tx: mpsc::Sender<SlashCommandInvocation>,
) -> Result<(), SlackError> {
    let client = reqwest::Client::new();
    loop {
        if let Err(error) = run_once(&client, &config, &tx).await {
            tracing::warn!(%error, "socket mode connection dropped, reconnecting");
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn run_once(
    client: &reqwest::Client,
    config: &Config,
    tx: &mpsc::Sender<SlashCommandInvocation>,
) -> Result<(), SlackError> {
    let url = open_connection_url(client, &config.app_token).await?;
    let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();

    while let Some(message) = read.next().await {
        match message? {
            Message::Text(text) => {
                let envelope: serde_json::Value = serde_json::from_str(&text)?;
                handle_envelope(&mut write, tx, envelope).await?;
            }
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
        }
    }
    Ok(())
}

async fn handle_envelope<W>(
    write: &mut W,
    tx: &mpsc::Sender<SlashCommandInvocation>,
    envelope: serde_json::Value,
) -> Result<(), SlackError>
where
    W: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    // Ack immediately, before any parsing or channel work, so Slack never
    // times out the envelope waiting on us.
    if let Some(envelope_id) = envelope
        .get("envelope_id")
        .and_then(serde_json::Value::as_str)
    {
        let ack = serde_json::json!({ "envelope_id": envelope_id });
        write.send(Message::Text(ack.to_string().into())).await?;
    }

    let envelope_type = envelope
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();

    if envelope_type == "slash_commands" {
        if let Some(payload) = envelope.get("payload") {
            match serde_json::from_value::<SlashCommandPayload>(payload.clone()) {
                Ok(payload) => {
                    if tx.send(payload.into()).await.is_err() {
                        tracing::warn!("engine channel closed, dropping slash command");
                    }
                }
                Err(error) => tracing::warn!(%error, "failed to parse slash command payload"),
            }
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct OpenConnectionResponse {
    ok: bool,
    url: Option<String>,
    error: Option<String>,
}

async fn open_connection_url(
    client: &reqwest::Client,
    app_token: &str,
) -> Result<String, SlackError> {
    let response: OpenConnectionResponse = client
        .post(CONNECTIONS_OPEN_URL)
        .bearer_auth(app_token)
        .send()
        .await?
        .json()
        .await?;

    if response.ok {
        response
            .url
            .ok_or_else(|| SlackError::Api("apps.connections.open: missing url".into()))
    } else {
        Err(SlackError::Api(
            response.error.unwrap_or_else(|| "unknown error".into()),
        ))
    }
}

/// Posts a plain-text response back to Slack via a slash command's
/// `response_url`. `text` doubles as the required fallback since we aren't
/// building Block Kit responses yet.
pub async fn respond(
    client: &reqwest::Client,
    response_url: &str,
    text: &str,
) -> Result<(), SlackError> {
    let body = serde_json::json!({
        "response_type": "ephemeral",
        "text": text,
    });
    let response = client.post(response_url).json(&body).send().await?;
    if !response.status().is_success() {
        return Err(SlackError::Api(format!(
            "response_url post failed: {}",
            response.status()
        )));
    }
    Ok(())
}

/// Maps a raw `/axo <subcommand>` text argument onto the core command enum.
/// Unknown or missing subcommands return `None`; the caller decides how to
/// respond (e.g. a usage message).
pub fn parse_command(text: &str) -> Option<axolotchi_core::SlashCommand> {
    match text.trim().to_ascii_lowercase().as_str() {
        "who" => Some(axolotchi_core::SlashCommand::Who),
        "feed" => Some(axolotchi_core::SlashCommand::Feed),
        "stats" => Some(axolotchi_core::SlashCommand::Stats),
        "dex" => Some(axolotchi_core::SlashCommand::Dex),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_subcommands() {
        assert_eq!(
            parse_command("who"),
            Some(axolotchi_core::SlashCommand::Who)
        );
        assert_eq!(
            parse_command(" Feed "),
            Some(axolotchi_core::SlashCommand::Feed)
        );
        assert_eq!(
            parse_command("STATS"),
            Some(axolotchi_core::SlashCommand::Stats)
        );
        assert_eq!(
            parse_command("dex"),
            Some(axolotchi_core::SlashCommand::Dex)
        );
    }

    #[test]
    fn rejects_unknown_subcommand() {
        assert_eq!(parse_command("bogus"), None);
        assert_eq!(parse_command(""), None);
    }
}
