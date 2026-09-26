//! The six mood PNGs (sleepy, alert, lonely, curious, happy, content),
//! uploaded to Slack once with their `slack_file` IDs cached in `kv`.
//!
//! `Mood` itself lives in `axolotchi-core` (the engine decides Axo's mood);
//! this crate just maps a mood onto the asset name used to upload and look
//! up its sprite. The upload-once logic and the actual PNG files land in
//! milestone 8 alongside the systemd unit; the Block Kit `image` blocks
//! that reference a mood's `slack_file` ID land in milestone 6.

use axolotchi_core::Mood;

/// The asset base name (without extension) for a mood's sprite, also used
/// as its key in the `kv` table (e.g. `sprite:sleepy`) once uploaded.
pub fn sprite_name(mood: Mood) -> &'static str {
    match mood {
        Mood::Sleepy => "sleepy",
        Mood::Alert => "alert",
        Mood::Lonely => "lonely",
        Mood::Curious => "curious",
        Mood::Happy => "happy",
        Mood::Content => "content",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mood_has_a_distinct_sprite_name() {
        let moods = [
            Mood::Sleepy,
            Mood::Alert,
            Mood::Lonely,
            Mood::Curious,
            Mood::Happy,
            Mood::Content,
        ];
        let names: std::collections::HashSet<_> = moods.iter().copied().map(sprite_name).collect();
        assert_eq!(names.len(), moods.len());
    }
}
