//! A scripted sighting source with no hardware involved at all, so the
//! whole engine -> store -> Slack pipeline can be exercised end to end on a
//! laptop. Each step fires at a fixed delay from the previous one and gets
//! its timestamp from the real clock at send time (reading the clock here
//! is fine — only `axolotchi-core` must stay IO/clock-free).

use axolotchi_core::{DeviceId, Event, SightingSource};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ScriptedAction {
    Sighting {
        device_id: DeviceId,
        ip: Option<String>,
        source: SightingSource,
    },
    SweepComplete,
}

#[derive(Debug, Clone)]
pub struct ScriptedEvent {
    /// Delay from the *previous* step (or from startup, for the first one).
    pub after: Duration,
    pub action: ScriptedAction,
}

fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Plays a script of events to `tx`, sleeping between each one. Returns once
/// the script is exhausted or the receiver is dropped.
pub async fn run_mock_source(script: Vec<ScriptedEvent>, tx: mpsc::Sender<Event>) {
    for step in script {
        tokio::time::sleep(step.after).await;
        let event = match step.action {
            ScriptedAction::Sighting {
                device_id,
                ip,
                source,
            } => Event::Sighting {
                device_id,
                ip,
                source,
                // Net never resolves a vendor itself, mock included;
                // axolotchid enriches this centrally via the OUI table
                // before handing the event to axolotchi-core.
                vendor: None,
                at: unix_now(),
            },
            ScriptedAction::SweepComplete => Event::SweepComplete { at: unix_now() },
        };
        if tx.send(event).await.is_err() {
            return;
        }
    }
}

/// A short demo scenario touching every presence transition: two devices
/// join, one goes quiet and is rescued by a passive sighting, the other
/// gets a new IP and eventually goes `Gone` after enough missed sweeps.
pub fn demo_script() -> Vec<ScriptedEvent> {
    use ScriptedAction::*;
    let sighting = |device_id: &str, ip: &str, source: SightingSource| Sighting {
        device_id: device_id.to_string(),
        ip: Some(ip.to_string()),
        source,
    };

    vec![
        ScriptedEvent {
            after: Duration::ZERO,
            action: sighting("aa:bb:cc:dd:ee:01", "10.0.0.11", SightingSource::Active),
        },
        ScriptedEvent {
            after: Duration::from_secs(1),
            action: sighting("aa:bb:cc:dd:ee:02", "10.0.0.12", SightingSource::Active),
        },
        ScriptedEvent {
            after: Duration::from_secs(4),
            action: SweepComplete,
        },
        // dev-02 misses this sweep (nothing resights it) while dev-01 gets
        // reconfirmed with a new IP.
        ScriptedEvent {
            after: Duration::from_secs(1),
            action: sighting("aa:bb:cc:dd:ee:01", "10.0.0.21", SightingSource::Active),
        },
        ScriptedEvent {
            after: Duration::from_secs(4),
            action: SweepComplete,
        },
        // A passive sniff rescues dev-02 before it goes Gone.
        ScriptedEvent {
            after: Duration::from_secs(1),
            action: sighting("aa:bb:cc:dd:ee:02", "10.0.0.12", SightingSource::Passive),
        },
        ScriptedEvent {
            after: Duration::from_secs(4),
            action: SweepComplete,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn plays_the_script_in_order() {
        let script = vec![
            ScriptedEvent {
                after: Duration::ZERO,
                action: ScriptedAction::Sighting {
                    device_id: "dev-1".into(),
                    ip: None,
                    source: SightingSource::Active,
                },
            },
            ScriptedEvent {
                after: Duration::from_secs(5),
                action: ScriptedAction::SweepComplete,
            },
        ];

        let (tx, mut rx) = mpsc::channel(8);
        tokio::spawn(run_mock_source(script, tx));

        let first = rx.recv().await.unwrap();
        assert!(matches!(first, Event::Sighting { .. }));
        let second = rx.recv().await.unwrap();
        assert!(matches!(second, Event::SweepComplete { .. }));
        assert!(rx.recv().await.is_none());
    }

    #[test]
    fn demo_script_is_non_empty() {
        assert!(!demo_script().is_empty());
    }
}
