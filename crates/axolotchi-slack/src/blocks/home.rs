//! The App Home tab: Axo's mood and stage, then a list of known devices.
//! Each device's overflow menu carries no `confirm` object — Slack's
//! overflow confirm would apply to every option, but only Forget needs
//! confirming, so that happens in a follow-up modal instead (see
//! `forget_confirm_modal`).

use super::limits::{
    cap_context_elements, cap_overflow_options, truncate, MAX_HEADER_CHARS, MAX_HOME_DEVICES,
    MAX_TEXT_CHARS,
};
use super::view_models::{DeviceSummary, HomeViewModel};
use axolotchi_core::Presence;
use serde_json::{json, Value};

fn presence_line(device: &DeviceSummary) -> String {
    let status = match device.presence {
        Presence::Present => "🟢 present",
        Presence::Missed(_) => "🟡 missed a check-in",
        Presence::Gone => "⚪ gone",
    };
    let mut line = format!("*{}*\n{status}", truncate(&device.label, MAX_TEXT_CHARS));
    if let Some(flavor) = &device.vendor_flavor {
        line.push_str(&format!("\n_{}_", truncate(flavor, MAX_TEXT_CHARS)));
    }
    line
}

fn device_block(device: &DeviceSummary) -> Value {
    let options = cap_overflow_options(vec![
        json!({ "text": { "type": "plain_text", "text": "Details" }, "value": format!("details:{}", device.id) }),
        json!({ "text": { "type": "plain_text", "text": "Rename" }, "value": format!("rename:{}", device.id) }),
        json!({ "text": { "type": "plain_text", "text": "Forget" }, "value": format!("forget:{}", device.id) }),
    ]);
    json!({
        "type": "section",
        "text": { "type": "mrkdwn", "text": presence_line(device) },
        "accessory": {
            "type": "overflow",
            "action_id": "device_overflow",
            "options": options,
        },
    })
}

/// The mood/stage/XP/device blocks shared by the Home tab and the pinned
/// live-tank message — the two views show the same content, just wrapped
/// differently (`{"type":"home",...}` vs. a plain message payload).
pub(super) fn body_blocks(vm: &HomeViewModel) -> Vec<Value> {
    let mut blocks = vec![json!({
        "type": "header",
        "text": { "type": "plain_text", "text": truncate(&format!("{} the Axolotl", vm.pet_name), MAX_HEADER_CHARS) },
    })];

    match &vm.mood_sprite_file_id {
        Some(file_id) => blocks.push(json!({
            "type": "image",
            "slack_file": { "id": file_id },
            "alt_text": format!("{:?}", vm.mood),
        })),
        None => blocks.push(json!({
            "type": "section",
            "text": { "type": "mrkdwn", "text": format!("*Mood:* {:?}", vm.mood) },
        })),
    }

    blocks.push(json!({
        "type": "section",
        "text": {
            "type": "mrkdwn",
            "text": format!("*Stage:* {:?}\n*XP:* {}", vm.stage, vm.xp),
        },
    }));
    blocks.push(json!({ "type": "divider" }));

    let shown = vm.devices.iter().take(MAX_HOME_DEVICES);
    let shown_count = shown.len();
    for device in shown {
        blocks.push(device_block(device));
    }

    let mut footer_elements = Vec::new();
    let remaining = vm.devices.len().saturating_sub(shown_count);
    if remaining > 0 {
        footer_elements.push(json!({
            "type": "mrkdwn",
            "text": format!("+{remaining} more devices, see `/axo who`"),
        }));
    }
    footer_elements.push(json!({
        "type": "mrkdwn",
        "text": format!("🏆 {}/{} achievements unlocked", vm.achievements_unlocked, vm.achievements_total),
    }));
    blocks.push(json!({
        "type": "context",
        "elements": cap_context_elements(footer_elements),
    }));

    blocks
}

pub fn home_view(vm: &HomeViewModel) -> Value {
    json!({ "type": "home", "blocks": body_blocks(vm) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axolotchi_core::{Mood, Stage};

    fn sample() -> HomeViewModel {
        HomeViewModel {
            pet_name: "Axo".into(),
            mood: Mood::Curious,
            mood_sprite_file_id: Some("F0123CURIOUS".into()),
            stage: Stage::Hatchling,
            xp: 10,
            devices: vec![
                DeviceSummary {
                    id: "aa:bb:cc:dd:ee:01".into(),
                    label: "Kitchen Pi".into(),
                    presence: Presence::Present,
                    vendor_flavor: Some("A tiny, tireless workhorse.".into()),
                },
                DeviceSummary {
                    id: "aa:bb:cc:dd:ee:02".into(),
                    label: "Unknown Wanderer".into(),
                    presence: Presence::Missed(1),
                    vendor_flavor: None,
                },
            ],
            achievements_unlocked: 1,
            achievements_total: 4,
        }
    }

    #[test]
    fn snapshot_home_view() {
        insta::assert_json_snapshot!(home_view(&sample()));
    }

    #[test]
    fn snapshot_home_view_without_sprite_uploaded_yet() {
        let mut vm = sample();
        vm.mood_sprite_file_id = None;
        insta::assert_json_snapshot!(home_view(&vm));
    }

    #[test]
    fn caps_device_list_and_notes_the_remainder() {
        let mut vm = sample();
        vm.devices = (0..(MAX_HOME_DEVICES + 5))
            .map(|i| DeviceSummary {
                id: format!("dev-{i}"),
                label: format!("Device {i}"),
                presence: Presence::Present,
                vendor_flavor: None,
            })
            .collect();
        let view = home_view(&vm);
        let blocks = view["blocks"].as_array().unwrap();
        let device_blocks = blocks
            .iter()
            .filter(|b| b["type"] == "section" && b["accessory"]["type"] == "overflow")
            .count();
        assert_eq!(device_blocks, MAX_HOME_DEVICES);
        let remainder_note = blocks.iter().any(|b| {
            b["type"] == "context"
                && b["elements"][0]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("+5 more")
        });
        assert!(remainder_note, "expected a '+5 more devices' context block");
    }

    #[test]
    fn overflow_values_are_normalized_action_colon_device_id() {
        let view = home_view(&sample());
        // blocks: 0 header, 1 mood image, 2 stage/xp, 3 divider, 4+ devices.
        let options = &view["blocks"][4]["accessory"]["options"];
        assert_eq!(options[0]["value"], "details:aa:bb:cc:dd:ee:01");
        assert_eq!(options[1]["value"], "rename:aa:bb:cc:dd:ee:01");
        assert_eq!(options[2]["value"], "forget:aa:bb:cc:dd:ee:01");
    }

    #[test]
    fn overflow_has_no_confirm_object() {
        let view = home_view(&sample());
        assert!(view["blocks"][4]["accessory"]["confirm"].is_null());
    }
}
