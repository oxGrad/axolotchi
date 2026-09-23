//! Pure game logic: presence state machine, mood, XP, evolution, achievements.
//!
//! No IO, no clock reads — every timestamp comes in via `Event`. Milestone 1
//! wires up the `step` entry point with a minimal presence skeleton;
//! milestone 2 fills in the full state machine (limit, regrow, passive
//! rescue, IP change) per `docs/reference/presence.rs`.

use std::collections::HashMap;

pub type DeviceId = String;
pub type Timestamp = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Present,
    Missed(u32),
    Gone,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub id: DeviceId,
    pub presence: Presence,
    pub last_seen: Timestamp,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub devices: HashMap<DeviceId, Device>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Sighting { device_id: DeviceId, at: Timestamp },
    SweepComplete { at: Timestamp },
    SlashCommand { command: SlashCommand },
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
}

/// Advance the game state by one event, returning the new state plus any
/// effects (store writes, re-renders) the caller should carry out.
pub fn step(mut state: State, event: Event) -> (State, Vec<Effect>) {
    match event {
        Event::Sighting { device_id, at } => {
            let mut effects = Vec::new();
            let existed = state.devices.contains_key(&device_id);
            let device = state
                .devices
                .entry(device_id.clone())
                .or_insert_with(|| Device {
                    id: device_id.clone(),
                    presence: Presence::Gone,
                    last_seen: at,
                });
            let was_present = device.presence == Presence::Present;
            device.presence = Presence::Present;
            device.last_seen = at;
            if !existed || !was_present {
                effects.push(Effect::Persist { device_id });
            }
            (state, effects)
        }
        Event::SweepComplete { .. } => (state, Vec::new()),
        Event::SlashCommand { .. } => (state, vec![Effect::Render]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sighting_marks_device_present_and_persists() {
        let state = State::default();
        let (state, effects) = step(
            state,
            Event::Sighting {
                device_id: "aa:bb:cc:dd:ee:ff".into(),
                at: 1000,
            },
        );
        let device = state.devices.get("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!(device.presence, Presence::Present);
        assert_eq!(device.last_seen, 1000);
        assert_eq!(
            effects,
            vec![Effect::Persist {
                device_id: "aa:bb:cc:dd:ee:ff".into()
            }]
        );
    }

    #[test]
    fn repeated_sighting_does_not_re_persist() {
        let state = State::default();
        let (state, _) = step(
            state,
            Event::Sighting {
                device_id: "dev-1".into(),
                at: 1000,
            },
        );
        let (_, effects) = step(
            state,
            Event::Sighting {
                device_id: "dev-1".into(),
                at: 1010,
            },
        );
        assert!(effects.is_empty());
    }

    #[test]
    fn slash_command_triggers_render() {
        let state = State::default();
        let (_, effects) = step(
            state,
            Event::SlashCommand {
                command: SlashCommand::Who,
            },
        );
        assert_eq!(effects, vec![Effect::Render]);
    }
}
