//! Mood: an event-driven state machine, same shape as presence. Each rule
//! reacts to something that just happened rather than being recomputed from
//! a snapshot, so a specific reason (a device joined, one went missing, Axo
//! got fed) always wins over the ambient sweep-settled rules.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Sleepy,
    Alert,
    Lonely,
    Curious,
    Happy,
    Content,
}

/// How many consecutive "everyone's still here, nothing happened" sweeps it
/// takes before content settles into sleepy.
const IDLE_SWEEPS_UNTIL_SLEEPY: u32 = 3;

#[derive(Debug, Clone, Copy)]
pub struct MoodState {
    pub mood: Mood,
    idle_sweeps: u32,
    /// Consecutive settled sweeps with nobody missing — achievements.rs
    /// reads this for the "held it together" rule.
    pub content_streak: u32,
}

impl Default for MoodState {
    fn default() -> Self {
        // A freshly hatched Axo is curious about its new LAN before it's
        // seen anything at all.
        Self {
            mood: Mood::Curious,
            idle_sweeps: 0,
            content_streak: 0,
        }
    }
}

pub fn on_device_joined_or_returned(state: &mut MoodState) {
    state.mood = Mood::Curious;
    state.idle_sweeps = 0;
    state.content_streak = 0;
}

pub fn on_device_gone(state: &mut MoodState) {
    state.mood = Mood::Alert;
    state.idle_sweeps = 0;
    state.content_streak = 0;
}

pub fn on_fed(state: &mut MoodState) {
    state.mood = Mood::Happy;
    state.idle_sweeps = 0;
}

/// Called after a sweep that didn't just cause a device to go `Gone`, with
/// how many known devices are present right now.
pub fn on_sweep_settled(state: &mut MoodState, present: usize, known: usize) {
    if known == 0 || present == 0 {
        state.mood = Mood::Lonely;
        state.idle_sweeps = 0;
        state.content_streak = 0;
        return;
    }
    if present == known {
        state.idle_sweeps += 1;
        state.content_streak += 1;
        state.mood = if state.idle_sweeps >= IDLE_SWEEPS_UNTIL_SLEEPY {
            Mood::Sleepy
        } else {
            Mood::Content
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_curious() {
        assert_eq!(MoodState::default().mood, Mood::Curious);
    }

    #[test]
    fn device_joining_sets_curious() {
        let mut state = MoodState {
            mood: Mood::Sleepy,
            ..MoodState::default()
        };
        on_device_joined_or_returned(&mut state);
        assert_eq!(state.mood, Mood::Curious);
    }

    #[test]
    fn device_going_gone_sets_alert() {
        let mut state = MoodState::default();
        on_device_gone(&mut state);
        assert_eq!(state.mood, Mood::Alert);
    }

    #[test]
    fn feeding_sets_happy() {
        let mut state = MoodState::default();
        on_fed(&mut state);
        assert_eq!(state.mood, Mood::Happy);
    }

    #[test]
    fn nobody_known_or_present_is_lonely() {
        let mut state = MoodState::default();
        on_sweep_settled(&mut state, 0, 0);
        assert_eq!(state.mood, Mood::Lonely);

        let mut state = MoodState::default();
        on_sweep_settled(&mut state, 0, 3);
        assert_eq!(state.mood, Mood::Lonely);
    }

    #[test]
    fn everyone_present_settles_content_then_sleepy_after_idle_sweeps() {
        let mut state = MoodState::default();
        on_sweep_settled(&mut state, 2, 2);
        assert_eq!(state.mood, Mood::Content);
        on_sweep_settled(&mut state, 2, 2);
        assert_eq!(state.mood, Mood::Content);
        on_sweep_settled(&mut state, 2, 2);
        assert_eq!(state.mood, Mood::Sleepy);
    }

    #[test]
    fn a_reason_to_be_curious_resets_the_idle_streak() {
        let mut state = MoodState::default();
        on_sweep_settled(&mut state, 2, 2);
        on_sweep_settled(&mut state, 2, 2);
        on_sweep_settled(&mut state, 2, 2);
        assert_eq!(state.mood, Mood::Sleepy);

        on_device_joined_or_returned(&mut state);
        on_sweep_settled(&mut state, 2, 2);
        assert_eq!(state.mood, Mood::Content);
    }

    #[test]
    fn content_streak_tracks_consecutive_fully_present_sweeps() {
        let mut state = MoodState::default();
        on_sweep_settled(&mut state, 2, 2);
        on_sweep_settled(&mut state, 2, 2);
        assert_eq!(state.content_streak, 2);
        on_device_gone(&mut state);
        assert_eq!(state.content_streak, 0);
    }
}
