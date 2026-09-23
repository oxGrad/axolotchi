//! The six mood PNGs (sleepy, alert, lonely, curious, happy, content),
//! uploaded to Slack once with their `slack_file` IDs cached in `kv`.
//!
//! Stub crate (milestone 1). Sprite assets and the upload-once logic land
//! in milestone 6 alongside the Block Kit builders that reference them.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mood {
    Sleepy,
    Alert,
    Lonely,
    Curious,
    Happy,
    Content,
}
