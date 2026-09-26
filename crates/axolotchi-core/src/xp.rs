//! XP and evolution stages. A device's first-ever sighting and an accepted
//! feed both grant XP; crossing a stage threshold is a "morph" the caller
//! should announce.

use crate::{Effect, Timestamp};

pub const XP_NEW_DEVICE: u32 = 10;
pub const XP_FEED: u32 = 5;

/// Feeding only grants XP outside this window, so `/axo feed` spam can't
/// farm levels.
pub const FEED_COOLDOWN_SECS: i64 = 30 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Hatchling,
    Juvenile,
    Adult,
    Elder,
}

impl Stage {
    fn from_xp(xp: u32) -> Self {
        match xp {
            0..=49 => Stage::Hatchling,
            50..=199 => Stage::Juvenile,
            200..=499 => Stage::Adult,
            _ => Stage::Elder,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct XpState {
    pub xp: u32,
    pub stage: Stage,
    last_fed: Option<Timestamp>,
}

impl Default for XpState {
    fn default() -> Self {
        Self {
            xp: 0,
            stage: Stage::Hatchling,
            last_fed: None,
        }
    }
}

fn gain(state: &mut XpState, amount: u32) -> Option<Effect> {
    let previous_stage = state.stage;
    state.xp += amount;
    state.stage = Stage::from_xp(state.xp);
    if state.stage != previous_stage {
        Some(Effect::Morph { stage: state.stage })
    } else {
        None
    }
}

/// Call once per brand-new device (never seen before), not on every
/// resighting.
pub fn on_new_device(state: &mut XpState) -> Option<Effect> {
    gain(state, XP_NEW_DEVICE)
}

/// Returns whether the feed was accepted (outside the cooldown) so the
/// caller can tell the user, plus a `Morph` effect if it crossed a stage.
pub fn on_feed(state: &mut XpState, at: Timestamp) -> (bool, Option<Effect>) {
    if let Some(last) = state.last_fed {
        if at - last < FEED_COOLDOWN_SECS {
            return (false, None);
        }
    }
    state.last_fed = Some(at);
    (true, gain(state, XP_FEED))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_zero_xp_as_a_hatchling() {
        let state = XpState::default();
        assert_eq!(state.xp, 0);
        assert_eq!(state.stage, Stage::Hatchling);
    }

    #[test]
    fn new_device_grants_xp_without_a_morph_below_threshold() {
        let mut state = XpState::default();
        let effect = on_new_device(&mut state);
        assert_eq!(state.xp, XP_NEW_DEVICE);
        assert_eq!(effect, None);
    }

    #[test]
    fn crossing_a_stage_threshold_emits_a_morph() {
        let mut state = XpState::default();
        for _ in 0..4 {
            on_new_device(&mut state); // 40 xp, still Hatchling
        }
        assert_eq!(state.stage, Stage::Hatchling);

        let effect = on_new_device(&mut state); // 50 xp, crosses into Juvenile
        assert_eq!(state.stage, Stage::Juvenile);
        assert_eq!(
            effect,
            Some(Effect::Morph {
                stage: Stage::Juvenile
            })
        );
    }

    #[test]
    fn feed_is_accepted_outside_the_cooldown() {
        let mut state = XpState::default();
        let (accepted, _) = on_feed(&mut state, 0);
        assert!(accepted);
        assert_eq!(state.xp, XP_FEED);
    }

    #[test]
    fn feed_within_the_cooldown_is_rejected_and_grants_no_xp() {
        let mut state = XpState::default();
        on_feed(&mut state, 0);
        let (accepted, effect) = on_feed(&mut state, FEED_COOLDOWN_SECS - 1);
        assert!(!accepted);
        assert_eq!(effect, None);
        assert_eq!(state.xp, XP_FEED);
    }

    #[test]
    fn feed_after_the_cooldown_elapses_is_accepted_again() {
        let mut state = XpState::default();
        on_feed(&mut state, 0);
        let (accepted, _) = on_feed(&mut state, FEED_COOLDOWN_SECS);
        assert!(accepted);
        assert_eq!(state.xp, XP_FEED * 2);
    }

    #[test]
    fn stage_ordering_matches_progression() {
        assert!(Stage::Hatchling < Stage::Juvenile);
        assert!(Stage::Juvenile < Stage::Adult);
        assert!(Stage::Adult < Stage::Elder);
    }
}
