//! Configures the window during which Axo holds back alerts. No native
//! toggle element exists in Block Kit modals, so a single-option
//! `checkboxes` input stands in for an enable/disable switch.

use super::limits::{truncate, MAX_MODAL_TITLE_CHARS};
use super::view_models::QuietHoursViewModel;
use serde_json::{json, Value};

fn time_of_day(hour: u8) -> String {
    format!("{:02}:00", hour.min(23))
}

pub fn quiet_hours_modal(vm: &QuietHoursViewModel) -> Value {
    let mut enabled_element = json!({
        "type": "checkboxes",
        "action_id": "quiet_hours_enabled",
        "options": [
            {
                "text": { "type": "plain_text", "text": "Enable quiet hours" },
                "value": "enabled",
            }
        ],
    });
    if vm.enabled {
        enabled_element["initial_options"] = json!([
            { "text": { "type": "plain_text", "text": "Enable quiet hours" }, "value": "enabled" }
        ]);
    }

    json!({
        "type": "modal",
        "callback_id": "quiet_hours",
        "title": { "type": "plain_text", "text": truncate("Quiet Hours", MAX_MODAL_TITLE_CHARS) },
        "submit": { "type": "plain_text", "text": "Save" },
        "close": { "type": "plain_text", "text": "Cancel" },
        "blocks": [
            {
                "type": "input",
                "block_id": "quiet_hours_enabled_block",
                "optional": true,
                "label": { "type": "plain_text", "text": "Enabled" },
                "element": enabled_element,
            },
            {
                "type": "input",
                "block_id": "quiet_hours_start_block",
                "label": { "type": "plain_text", "text": "Starts at" },
                "element": {
                    "type": "timepicker",
                    "action_id": "quiet_hours_start",
                    "initial_time": time_of_day(vm.start_hour),
                },
            },
            {
                "type": "input",
                "block_id": "quiet_hours_end_block",
                "label": { "type": "plain_text", "text": "Ends at" },
                "element": {
                    "type": "timepicker",
                    "action_id": "quiet_hours_end",
                    "initial_time": time_of_day(vm.end_hour),
                },
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_quiet_hours_enabled() {
        let vm = QuietHoursViewModel {
            enabled: true,
            start_hour: 22,
            end_hour: 7,
        };
        insta::assert_json_snapshot!(quiet_hours_modal(&vm));
    }

    #[test]
    fn snapshot_quiet_hours_disabled() {
        let vm = QuietHoursViewModel {
            enabled: false,
            start_hour: 22,
            end_hour: 7,
        };
        insta::assert_json_snapshot!(quiet_hours_modal(&vm));
    }

    #[test]
    fn disabled_has_no_initial_options() {
        let vm = QuietHoursViewModel {
            enabled: false,
            start_hour: 0,
            end_hour: 0,
        };
        let view = quiet_hours_modal(&vm);
        assert!(view["blocks"][0]["element"]["initial_options"].is_null());
    }

    #[test]
    fn times_are_formatted_as_24_hour_hh_mm() {
        let vm = QuietHoursViewModel {
            enabled: true,
            start_hour: 9,
            end_hour: 17,
        };
        let view = quiet_hours_modal(&vm);
        assert_eq!(view["blocks"][1]["element"]["initial_time"], "09:00");
        assert_eq!(view["blocks"][2]["element"]["initial_time"], "17:00");
    }
}
