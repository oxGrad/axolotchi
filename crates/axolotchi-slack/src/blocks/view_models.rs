//! Everything a builder needs is handed in here; builders themselves never
//! read `axolotchi_core::State`, `axolotchi_store`, or the clock directly,
//! so they stay pure `fn(&ViewModel) -> serde_json::Value`.

use axolotchi_core::{Mood, Presence, Stage};

#[derive(Debug, Clone)]
pub struct DeviceSummary {
    pub id: String,
    pub label: String,
    pub presence: Presence,
    pub vendor_flavor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HomeViewModel {
    pub pet_name: String,
    pub mood: Mood,
    /// The mood's `slack_file` external id, once uploaded. `None` before
    /// the sprite upload has happened, in which case the view falls back
    /// to a text-only mood line instead of an image block.
    pub mood_sprite_file_id: Option<String>,
    pub stage: Stage,
    pub xp: u32,
    pub devices: Vec<DeviceSummary>,
    pub achievements_unlocked: usize,
    pub achievements_total: usize,
}

#[derive(Debug, Clone)]
pub enum AlertKind {
    DeviceJoined,
    DeviceReturned,
    DeviceGone,
    IpChanged { old: Option<String>, new: String },
}

#[derive(Debug, Clone)]
pub struct AlertViewModel {
    pub pet_name: String,
    pub device_label: String,
    pub kind: AlertKind,
    pub mood: Mood,
}

#[derive(Debug, Clone)]
pub struct DeviceCardViewModel {
    pub id: String,
    pub label: String,
    pub ip: Option<String>,
    pub presence: Presence,
    /// Pre-formatted by the caller — core has no clock and no formatting
    /// opinions, so "last seen 3 minutes ago" style text is assembled
    /// upstream of the builder.
    pub last_seen_text: String,
    pub vendor: Option<String>,
    pub dex_flavor: String,
}

#[derive(Debug, Clone)]
pub struct EditModalViewModel {
    pub device_id: String,
    pub current_label: String,
}

#[derive(Debug, Clone)]
pub struct ForgetConfirmViewModel {
    pub device_id: String,
    pub device_label: String,
}

#[derive(Debug, Clone)]
pub struct NetdexEntryView {
    pub vendor: String,
    pub flavor: String,
    pub discovered: bool,
}

#[derive(Debug, Clone)]
pub struct NetdexModalViewModel {
    pub entries: Vec<NetdexEntryView>,
}

#[derive(Debug, Clone)]
pub struct QuietHoursViewModel {
    pub enabled: bool,
    /// 24-hour clock, 0-23.
    pub start_hour: u8,
    pub end_hour: u8,
}
