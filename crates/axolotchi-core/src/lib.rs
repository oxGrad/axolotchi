//! Pure game logic: presence state machine, mood, XP, evolution, achievements.
//!
//! No IO, no clock reads — every timestamp comes in via `Event`, and every
//! vendor name comes in resolved (the OUI lookup lives in
//! `axolotchi-store`). Milestone 5 fills in mood, XP/stages, the
//! achievements rule table, and Netdex flavor text.

mod achievements;
mod mood;
mod netdex;
mod presence;
mod xp;

pub use achievements::{count as achievement_count, Achievement};
pub use mood::{Mood, MoodState};
pub use netdex::DexEntry;
pub use presence::{Device, Presence, PresenceLimits, SightingSource};
pub use xp::{Stage, XpState};

use std::collections::{HashMap, HashSet};

pub type DeviceId = String;
pub type Timestamp = i64;

#[derive(Debug, Clone, Default)]
pub struct State {
    pub devices: HashMap<DeviceId, Device>,
    pub presence_limits: PresenceLimits,
    pub mood: MoodState,
    pub xp: XpState,
    pub achievements: HashSet<Achievement>,
    pub discovered_vendors: HashSet<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Sighting {
        device_id: DeviceId,
        ip: Option<String>,
        source: SightingSource,
        /// The device's OUI vendor, already resolved by the caller (e.g.
        /// via `axolotchi_store::lookup_vendor`) — core never touches
        /// SQLite itself.
        vendor: Option<String>,
        at: Timestamp,
    },
    SweepComplete {
        at: Timestamp,
    },
    SlashCommand {
        command: SlashCommand,
        at: Timestamp,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommand {
    Who,
    Feed,
    Stats,
    Dex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Persist { device_id: DeviceId },
    Render,
    Morph { stage: Stage },
    AchievementUnlocked(Achievement),
}

/// Looks up Netdex flavor text for a vendor name. A thin pass-through to
/// `netdex::lookup`, exposed here since `netdex` itself is private.
pub fn dex_entry(vendor: Option<&str>) -> DexEntry {
    netdex::lookup(vendor)
}

/// Every entry in the Netdex, for rendering the full catalog.
pub fn dex_entries() -> &'static [DexEntry] {
    netdex::entries()
}

/// Advance the game state by one event, returning the new state plus any
/// effects (store writes, re-renders) the caller should carry out.
pub fn step(mut state: State, event: Event) -> (State, Vec<Effect>) {
    let mut effects = match event {
        Event::Sighting {
            device_id,
            ip,
            source,
            vendor,
            at,
        } => {
            let is_new = !state.devices.contains_key(&device_id);
            let mut effects = presence::on_sighting(&mut state.devices, device_id, ip, source, at);

            if !effects.is_empty() {
                mood::on_device_joined_or_returned(&mut state.mood);
            }
            if is_new {
                if let Some(vendor) = vendor {
                    state.discovered_vendors.insert(vendor);
                }
                if let Some(effect) = xp::on_new_device(&mut state.xp) {
                    effects.push(effect);
                }
            }
            effects
        }
        Event::SweepComplete { .. } => {
            let mut effects =
                presence::on_sweep_complete(&mut state.devices, state.presence_limits);

            let went_gone = effects.iter().any(|effect| {
                matches!(effect, Effect::Persist { device_id }
                    if state.devices.get(device_id).map(|d| d.presence) == Some(Presence::Gone))
            });
            let previous_mood = state.mood.mood;
            if went_gone {
                mood::on_device_gone(&mut state.mood);
            } else {
                let known = state.devices.len();
                let present = state
                    .devices
                    .values()
                    .filter(|d| d.presence == Presence::Present)
                    .count();
                mood::on_sweep_settled(&mut state.mood, present, known);
            }
            if state.mood.mood != previous_mood {
                effects.push(Effect::Render);
            }
            effects
        }
        Event::SlashCommand { command, at } => {
            let mut effects = vec![Effect::Render];
            if command == SlashCommand::Feed {
                let (accepted, morph) = xp::on_feed(&mut state.xp, at);
                if accepted {
                    mood::on_fed(&mut state.mood);
                }
                if let Some(effect) = morph {
                    effects.push(effect);
                }
            }
            effects
        }
    };

    for achievement in achievements::evaluate(&mut state) {
        effects.push(Effect::AchievementUnlocked(achievement));
    }

    (state, effects)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sighting(device_id: &str, at: Timestamp) -> Event {
        Event::Sighting {
            device_id: device_id.into(),
            ip: None,
            source: SightingSource::Active,
            vendor: None,
            at,
        }
    }

    fn slash(command: SlashCommand, at: Timestamp) -> Event {
        Event::SlashCommand { command, at }
    }

    #[test]
    fn first_sighting_marks_device_present_and_persists() {
        let state = State::default();
        let (state, effects) = step(state, sighting("aa:bb:cc:dd:ee:ff", 1000));
        let device = state.devices.get("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!(device.presence, Presence::Present);
        assert_eq!(device.last_seen, 1000);
        assert_eq!(
            effects,
            vec![
                Effect::Persist {
                    device_id: "aa:bb:cc:dd:ee:ff".into()
                },
                Effect::Render,
                Effect::AchievementUnlocked(Achievement::FirstContact),
            ]
        );
        assert_eq!(state.mood.mood, Mood::Curious);
        assert_eq!(state.xp.xp, xp::XP_NEW_DEVICE);
    }

    #[test]
    fn repeated_sighting_does_not_re_persist_or_regrant_xp() {
        let state = State::default();
        let (state, _) = step(state, sighting("dev-1", 1000));
        let xp_after_first = state.xp.xp;
        let (state, effects) = step(state, sighting("dev-1", 1010));
        assert!(effects.is_empty());
        assert_eq!(state.xp.xp, xp_after_first);
    }

    #[test]
    fn sweep_complete_with_no_devices_settles_into_lonely() {
        let state = State::default();
        let (state, effects) = step(state, Event::SweepComplete { at: 1000 });
        assert!(state.devices.is_empty());
        assert_eq!(state.mood.mood, Mood::Lonely);
        // Mood moved from the default Curious to Lonely, so this is worth a render.
        assert_eq!(effects, vec![Effect::Render]);
    }

    #[test]
    fn device_going_gone_sets_alert_mood_and_renders() {
        let state = State::default();
        let (state, _) = step(state, sighting("dev-1", 0));
        let limits = PresenceLimits { miss_limit: 1 };
        let state = State {
            presence_limits: limits,
            ..state
        };
        // First sweep just clears the "sighted this cycle" flag; the
        // second is the real miss that (with miss_limit 1) goes Gone.
        let (state, _) = step(state, Event::SweepComplete { at: 1 });
        let (state, effects) = step(state, Event::SweepComplete { at: 2 });
        assert_eq!(state.mood.mood, Mood::Alert);
        assert!(effects.contains(&Effect::Render));
    }

    #[test]
    fn feed_grants_xp_and_happy_mood() {
        let state = State::default();
        let (state, effects) = step(state, slash(SlashCommand::Feed, 0));
        assert_eq!(state.mood.mood, Mood::Happy);
        assert_eq!(state.xp.xp, xp::XP_FEED);
        assert!(effects.contains(&Effect::Render));
    }

    #[test]
    fn feeding_within_the_cooldown_does_not_regrant_xp_or_mood() {
        let state = State::default();
        let (state, _) = step(state, slash(SlashCommand::Feed, 0));
        let (state, _) = step(state, slash(SlashCommand::Who, 1));
        // Who shouldn't change mood away from Happy on its own; feed again
        // immediately, still within the cooldown.
        let (state, _) = step(state, slash(SlashCommand::Feed, 1));
        assert_eq!(state.xp.xp, xp::XP_FEED);
    }

    #[test]
    fn other_slash_commands_only_render() {
        let state = State::default();
        let (_, effects) = step(state, slash(SlashCommand::Who, 0));
        assert_eq!(effects, vec![Effect::Render]);
    }

    #[test]
    fn enough_new_devices_morphs_and_unlocks_achievements() {
        let mut state = State::default();
        let mut all_effects = Vec::new();
        for i in 0..10 {
            let (next_state, effects) = step(state, sighting(&format!("dev-{i}"), i as i64));
            state = next_state;
            all_effects.extend(effects);
        }
        assert_eq!(state.xp.xp, xp::XP_NEW_DEVICE * 10);
        assert!(all_effects.contains(&Effect::Morph {
            stage: Stage::Juvenile
        }));
        assert!(all_effects.contains(&Effect::AchievementUnlocked(Achievement::NetworkDetective)));
    }

    #[test]
    fn discovered_vendor_is_recorded_on_first_sighting_only() {
        let state = State::default();
        let (state, _) = step(
            state,
            Event::Sighting {
                device_id: "dev-1".into(),
                ip: None,
                source: SightingSource::Active,
                vendor: Some("Espressif Inc.".into()),
                at: 0,
            },
        );
        assert!(state.discovered_vendors.contains("Espressif Inc."));
    }

    #[test]
    fn dex_entry_looks_up_flavor_text() {
        assert_eq!(dex_entry(Some("Espressif Inc.")).vendor, "Espressif Inc.");
        assert_eq!(dex_entry(None).vendor, "Unknown Wanderer");
    }

    #[test]
    fn dex_entries_lists_the_full_catalog() {
        assert!(!dex_entries().is_empty());
        assert!(dex_entries().iter().any(|e| e.vendor == "Espressif Inc."));
    }

    #[test]
    fn achievement_count_matches_the_rule_table() {
        assert_eq!(achievement_count(), 4);
    }
}
