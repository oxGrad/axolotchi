//! Pure game logic: presence state machine, mood, XP, evolution, achievements.
//!
//! No IO, no clock reads — every timestamp comes in via `Event`. Milestone 2
//! fills in the full presence state machine (limit, regrow, passive rescue,
//! IP change); mood, XP and achievements land in milestone 5.

mod presence;

pub use presence::{Device, Presence, PresenceLimits, SightingSource};

use std::collections::HashMap;

pub type DeviceId = String;
pub type Timestamp = i64;

#[derive(Debug, Clone, Default)]
pub struct State {
    pub devices: HashMap<DeviceId, Device>,
    pub presence_limits: PresenceLimits,
}

#[derive(Debug, Clone)]
pub enum Event {
    Sighting {
        device_id: DeviceId,
        ip: Option<String>,
        source: SightingSource,
        at: Timestamp,
    },
    SweepComplete {
        at: Timestamp,
    },
    SlashCommand {
        command: SlashCommand,
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
}

/// Advance the game state by one event, returning the new state plus any
/// effects (store writes, re-renders) the caller should carry out.
pub fn step(mut state: State, event: Event) -> (State, Vec<Effect>) {
    let effects = match event {
        Event::Sighting {
            device_id,
            ip,
            source,
            at,
        } => presence::on_sighting(&mut state.devices, device_id, ip, source, at),
        Event::SweepComplete { .. } => {
            presence::on_sweep_complete(&mut state.devices, state.presence_limits)
        }
        Event::SlashCommand { .. } => vec![Effect::Render],
    };
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
            at,
        }
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
            ]
        );
    }

    #[test]
    fn repeated_sighting_does_not_re_persist() {
        let state = State::default();
        let (state, _) = step(state, sighting("dev-1", 1000));
        let (_, effects) = step(state, sighting("dev-1", 1010));
        assert!(effects.is_empty());
    }

    #[test]
    fn sweep_complete_with_no_devices_is_a_no_op() {
        let state = State::default();
        let (_, effects) = step(state, Event::SweepComplete { at: 1000 });
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
