//! The Netdex: every vendor Axo knows flavor text for, with undiscovered
//! ones shown as a silhouette entry rather than spoiling their flavor text.

use super::limits::{truncate, MAX_MODAL_TITLE_CHARS, MAX_TEXT_CHARS, MAX_VIEW_BLOCKS};
use super::view_models::{NetdexEntryView, NetdexModalViewModel};
use serde_json::{json, Value};

fn entry_block(entry: &NetdexEntryView) -> Value {
    let text = if entry.discovered {
        format!(
            "*{}*\n_{}_",
            truncate(&entry.vendor, MAX_TEXT_CHARS),
            truncate(&entry.flavor, MAX_TEXT_CHARS)
        )
    } else {
        "*???*\n_Not yet discovered._".to_string()
    };
    json!({
        "type": "section",
        "text": { "type": "mrkdwn", "text": text },
    })
}

pub fn netdex_modal(vm: &NetdexModalViewModel) -> Value {
    // Each entry is a section plus a divider: two blocks per entry, minus
    // the title bar which isn't a block, leaves room for MAX_VIEW_BLOCKS/2
    // entries before hitting Slack's cap.
    let mut blocks = Vec::new();
    for entry in vm.entries.iter().take(MAX_VIEW_BLOCKS / 2) {
        blocks.push(entry_block(entry));
        blocks.push(json!({ "type": "divider" }));
    }
    blocks.pop(); // no trailing divider after the last entry

    json!({
        "type": "modal",
        "callback_id": "netdex",
        "title": { "type": "plain_text", "text": truncate("Netdex", MAX_MODAL_TITLE_CHARS) },
        "close": { "type": "plain_text", "text": "Close" },
        "blocks": blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> NetdexModalViewModel {
        NetdexModalViewModel {
            entries: vec![
                NetdexEntryView {
                    vendor: "Raspberry Pi Foundation".into(),
                    flavor: "A tiny, tireless workhorse.".into(),
                    discovered: true,
                },
                NetdexEntryView {
                    vendor: "Espressif Inc.".into(),
                    flavor: "A curious little sensor.".into(),
                    discovered: false,
                },
            ],
        }
    }

    #[test]
    fn snapshot_netdex_modal() {
        insta::assert_json_snapshot!(netdex_modal(&sample()));
    }

    #[test]
    fn undiscovered_entries_hide_their_flavor_text() {
        let view = netdex_modal(&sample());
        let text = view["blocks"][2]["text"]["text"].as_str().unwrap();
        assert!(text.contains("???"));
        assert!(!text.contains("curious little sensor"));
    }

    #[test]
    fn no_trailing_divider() {
        let view = netdex_modal(&sample());
        let blocks = view["blocks"].as_array().unwrap();
        assert_eq!(blocks.last().unwrap()["type"], "section");
    }
}
