//! A modal with everything known about one device, plus Rename/Forget
//! actions. Forget doesn't confirm inline here either, for the same reason
//! it doesn't in the overflow menu — it opens `forget_confirm_modal`.

use super::limits::{truncate, MAX_MODAL_TITLE_CHARS};
use super::view_models::DeviceCardViewModel;
use axolotchi_core::Presence;
use serde_json::{json, Value};

fn presence_text(presence: Presence) -> String {
    match presence {
        Presence::Present => "🟢 Present".to_string(),
        Presence::Missed(n) => format!("🟡 Missed {n} check-in(s)"),
        Presence::Gone => "⚪ Gone".to_string(),
    }
}

pub fn device_card_modal(vm: &DeviceCardViewModel) -> Value {
    let mut fields = vec![
        format!("*Status:*\n{}", presence_text(vm.presence)),
        format!("*Last seen:*\n{}", vm.last_seen_text),
        format!("*IP:*\n{}", vm.ip.as_deref().unwrap_or("unknown")),
        format!(
            "*Vendor:*\n{}",
            vm.vendor.as_deref().unwrap_or("Unknown Wanderer")
        ),
    ];
    fields.truncate(10); // section "fields" cap

    json!({
        "type": "modal",
        "callback_id": "device_card",
        "private_metadata": vm.id,
        "title": { "type": "plain_text", "text": truncate(&vm.label, MAX_MODAL_TITLE_CHARS) },
        "close": { "type": "plain_text", "text": "Close" },
        "blocks": [
            {
                "type": "section",
                "fields": fields.iter().map(|f| json!({ "type": "mrkdwn", "text": f })).collect::<Vec<_>>(),
            },
            { "type": "divider" },
            {
                "type": "section",
                "text": { "type": "mrkdwn", "text": format!("_{}_", vm.dex_flavor) },
            },
            {
                "type": "actions",
                "elements": [
                    {
                        "type": "button",
                        "action_id": "device_card_rename",
                        "text": { "type": "plain_text", "text": "Rename" },
                        "value": format!("rename:{}", vm.id),
                    },
                    {
                        "type": "button",
                        "action_id": "device_card_forget",
                        "text": { "type": "plain_text", "text": "Forget" },
                        "style": "danger",
                        "value": format!("forget:{}", vm.id),
                    },
                ],
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DeviceCardViewModel {
        DeviceCardViewModel {
            id: "aa:bb:cc:dd:ee:01".into(),
            label: "Kitchen Pi".into(),
            ip: Some("10.0.0.11".into()),
            presence: Presence::Present,
            last_seen_text: "just now".into(),
            vendor: Some("Raspberry Pi Foundation".into()),
            dex_flavor: "A tiny, tireless workhorse.".into(),
        }
    }

    #[test]
    fn snapshot_device_card() {
        insta::assert_json_snapshot!(device_card_modal(&sample()));
    }

    #[test]
    fn snapshot_device_card_missing_details() {
        let vm = DeviceCardViewModel {
            ip: None,
            vendor: None,
            ..sample()
        };
        insta::assert_json_snapshot!(device_card_modal(&vm));
    }

    #[test]
    fn buttons_carry_normalized_values() {
        let view = device_card_modal(&sample());
        let elements = &view["blocks"][3]["elements"];
        assert_eq!(elements[0]["value"], "rename:aa:bb:cc:dd:ee:01");
        assert_eq!(elements[1]["value"], "forget:aa:bb:cc:dd:ee:01");
    }
}
