//! A small rule table, evaluated against the current state after every
//! step. Anything not already unlocked whose predicate now holds gets
//! unlocked and reported back so the caller can announce it.

use crate::{Stage, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Achievement {
    FirstContact,
    NetworkDetective,
    Guardian,
    NightWatch,
}

const ALL: [Achievement; 4] = [
    Achievement::FirstContact,
    Achievement::NetworkDetective,
    Achievement::Guardian,
    Achievement::NightWatch,
];

impl Achievement {
    pub fn name(self) -> &'static str {
        match self {
            Achievement::FirstContact => "First Contact",
            Achievement::NetworkDetective => "Network Detective",
            Achievement::Guardian => "Guardian",
            Achievement::NightWatch => "Night Watch",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Achievement::FirstContact => "Axo met its first device.",
            Achievement::NetworkDetective => "Axo has catalogued 10 different devices.",
            Achievement::Guardian => "Axo grew into an Adult.",
            Achievement::NightWatch => "Everyone stayed present for 5 sweeps straight.",
        }
    }
}

const NETWORK_DETECTIVE_THRESHOLD: usize = 10;
const NIGHT_WATCH_STREAK: u32 = 5;

fn is_unlocked(achievement: Achievement, state: &State) -> bool {
    match achievement {
        Achievement::FirstContact => !state.devices.is_empty(),
        Achievement::NetworkDetective => state.devices.len() >= NETWORK_DETECTIVE_THRESHOLD,
        Achievement::Guardian => state.xp.stage >= Stage::Adult,
        Achievement::NightWatch => state.mood.content_streak >= NIGHT_WATCH_STREAK,
    }
}

/// Checks every achievement not yet in `state.achievements`, unlocking (and
/// returning) any whose rule now holds.
pub fn evaluate(state: &mut State) -> Vec<Achievement> {
    let mut newly_unlocked = Vec::new();
    for achievement in ALL {
        if !state.achievements.contains(&achievement) && is_unlocked(achievement, state) {
            state.achievements.insert(achievement);
            newly_unlocked.push(achievement);
        }
    }
    newly_unlocked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Device, Presence, SightingSource};

    fn device(id: &str) -> Device {
        Device::hydrate(
            id.to_string(),
            None,
            Presence::Present,
            0,
            SightingSource::Active,
        )
    }

    #[test]
    fn no_devices_unlocks_nothing() {
        let mut state = State::default();
        assert!(evaluate(&mut state).is_empty());
    }

    #[test]
    fn one_device_unlocks_first_contact_only() {
        let mut state = State::default();
        state.devices.insert("dev-1".into(), device("dev-1"));
        assert_eq!(evaluate(&mut state), vec![Achievement::FirstContact]);
        // Already unlocked, so evaluating again reports nothing new.
        assert!(evaluate(&mut state).is_empty());
    }

    #[test]
    fn ten_devices_unlocks_network_detective_too() {
        let mut state = State::default();
        for i in 0..10 {
            let id = format!("dev-{i}");
            state.devices.insert(id.clone(), device(&id));
        }
        let unlocked = evaluate(&mut state);
        assert!(unlocked.contains(&Achievement::FirstContact));
        assert!(unlocked.contains(&Achievement::NetworkDetective));
    }

    #[test]
    fn reaching_adult_stage_unlocks_guardian() {
        let mut state = State::default();
        state.xp.stage = Stage::Adult;
        assert_eq!(evaluate(&mut state), vec![Achievement::Guardian]);
    }

    #[test]
    fn content_streak_unlocks_night_watch() {
        let mut state = State::default();
        state.mood.content_streak = NIGHT_WATCH_STREAK;
        assert_eq!(evaluate(&mut state), vec![Achievement::NightWatch]);
    }
}
