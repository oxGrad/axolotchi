//! Block Kit builders: pure `fn(&ViewModel) -> serde_json::Value`, snapshot
//! tested with `insta`. No IO, no clock reads, no `axolotchi_core::State`
//! or `axolotchi_store` access here — everything a builder needs comes in
//! through its `ViewModel`, assembled by the caller (`axolotchid`).

mod alert;
mod device_card;
mod edit_modal;
mod forget_confirm_modal;
mod home;
mod limits;
mod live_tank;
mod netdex_modal;
mod quiet_hours_modal;
mod view_models;

pub use alert::alert_message;
pub use device_card::device_card_modal;
pub use edit_modal::edit_modal;
pub use forget_confirm_modal::forget_confirm_modal;
pub use home::home_view;
pub use live_tank::live_tank_message;
pub use netdex_modal::netdex_modal;
pub use quiet_hours_modal::quiet_hours_modal;
pub use view_models::{
    AlertKind, AlertViewModel, DeviceCardViewModel, DeviceSummary, EditModalViewModel,
    ForgetConfirmViewModel, HomeViewModel, NetdexEntryView, NetdexModalViewModel,
    QuietHoursViewModel,
};
