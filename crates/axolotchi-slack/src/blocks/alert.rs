//! A one-off alert message (`chat.postMessage`) for a presence change.
//! Always carries a `text` fallback alongside `blocks`, per the Slack rules
//! in `CLAUDE.md`.

use super::limits::truncate;
use super::view_models::{AlertKind, AlertViewModel};
use axolotchi_core::Mood;
use serde_json::{json, Value};

fn headline(vm: &AlertViewModel) -> String {
    match &vm.kind {
        AlertKind::DeviceJoined => format!("{} just joined the network.", vm.device_label),
        AlertKind::DeviceReturned => format!("{} is back.", vm.device_label),
        AlertKind::DeviceGone => format!("{} has gone missing.", vm.device_label),
        AlertKind::IpChanged { old, new } => match old {
            Some(old) => format!("{}'s IP changed from {old} to {new}.", vm.device_label),
            None => format!("{} showed up at {new}.", vm.device_label),
        },
    }
}

fn mood_emoji(mood: Mood) -> &'static str {
    match mood {
        Mood::Sleepy => "😴",
        Mood::Alert => "🚨",
        Mood::Lonely => "🥺",
        Mood::Curious => "🧐",
        Mood::Happy => "😋",
        Mood::Content => "🙂",
    }
}

pub fn alert_message(vm: &AlertViewModel) -> Value {
    let text = truncate(&headline(vm), 3000);
    json!({
        "text": text,
        "blocks": [
            {
                "type": "section",
                "text": { "type": "mrkdwn", "text": format!("{} {}", mood_emoji(vm.mood), text) },
            },
            {
                "type": "context",
                "elements": [
                    { "type": "mrkdwn", "text": format!("{} is watching the network.", vm.pet_name) }
                ],
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> AlertViewModel {
        AlertViewModel {
            pet_name: "Axo".into(),
            device_label: "Kitchen Pi".into(),
            kind: AlertKind::DeviceJoined,
            mood: Mood::Curious,
        }
    }

    #[test]
    fn snapshot_device_joined() {
        insta::assert_json_snapshot!(alert_message(&base()));
    }

    #[test]
    fn snapshot_device_gone() {
        let vm = AlertViewModel {
            kind: AlertKind::DeviceGone,
            mood: Mood::Alert,
            ..base()
        };
        insta::assert_json_snapshot!(alert_message(&vm));
    }

    #[test]
    fn snapshot_ip_changed() {
        let vm = AlertViewModel {
            kind: AlertKind::IpChanged {
                old: Some("10.0.0.11".into()),
                new: "10.0.0.21".into(),
            },
            ..base()
        };
        insta::assert_json_snapshot!(alert_message(&vm));
    }

    #[test]
    fn always_includes_a_text_fallback() {
        let payload = alert_message(&base());
        assert!(payload["text"].as_str().unwrap().contains("Kitchen Pi"));
    }
}
