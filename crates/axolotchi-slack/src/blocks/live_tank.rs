//! The pinned "live tank" channel message: the same content as the Home
//! tab, but as a standing `chat.postMessage`/`chat.update` payload for
//! anyone in the channel, not just whoever opens their Home tab.

use super::home::body_blocks;
use super::view_models::HomeViewModel;
use serde_json::{json, Value};

fn fallback_text(vm: &HomeViewModel) -> String {
    format!(
        "{} \u{2014} mood: {:?}, stage: {:?}, XP: {}",
        vm.pet_name, vm.mood, vm.stage, vm.xp
    )
}

/// Builds the `{"text": ..., "blocks": [...]}` payload for both
/// `chat.postMessage` (first post) and `chat.update` (every refresh) — the
/// caller supplies `channel`/`ts` separately, since those aren't part of
/// the message content itself.
pub fn live_tank_message(vm: &HomeViewModel) -> Value {
    json!({
        "text": fallback_text(vm),
        "blocks": body_blocks(vm),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{DeviceSummary, HomeViewModel};
    use axolotchi_core::{Mood, Presence, Stage};

    fn sample() -> HomeViewModel {
        HomeViewModel {
            pet_name: "Axo".into(),
            mood: Mood::Content,
            mood_sprite_file_id: None,
            stage: Stage::Juvenile,
            xp: 55,
            devices: vec![DeviceSummary {
                id: "aa:bb:cc:dd:ee:01".into(),
                label: "Kitchen Pi".into(),
                presence: Presence::Present,
                vendor_flavor: None,
            }],
            achievements_unlocked: 2,
            achievements_total: 4,
        }
    }

    #[test]
    fn snapshot_live_tank_message() {
        insta::assert_json_snapshot!(live_tank_message(&sample()));
    }

    #[test]
    fn always_includes_a_text_fallback() {
        let payload = live_tank_message(&sample());
        let text = payload["text"].as_str().unwrap();
        assert!(text.contains("Axo"));
        assert!(text.contains("Content"));
    }

    #[test]
    fn shares_block_content_with_home_view() {
        let payload = live_tank_message(&sample());
        let home = super::super::home_view(&sample());
        assert_eq!(payload["blocks"], home["blocks"]);
    }
}
