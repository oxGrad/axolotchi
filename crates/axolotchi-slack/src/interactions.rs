//! Parsing for Socket Mode's `interactive` envelopes: `block_actions`
//! (overflow menus, buttons) and `view_submission` (modal submits).
//!
//! Kept pure and separate from the websocket loop so it's unit testable
//! against hand-built JSON matching Slack's real payload shapes.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockAction {
    pub action_id: String,
    /// Normalized `"action:device_id"` value — from `selected_option.value`
    /// for an overflow menu, or `value` directly for a button, per the
    /// Slack rules in `CLAUDE.md`.
    pub value: String,
    pub trigger_id: String,
    pub user_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewSubmission {
    pub callback_id: String,
    pub private_metadata: String,
    pub view_id: String,
    pub user_id: String,
    /// The raw `view.state.values` object; the caller pulls out whichever
    /// block/action ids it built the view with.
    pub values: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Interaction {
    BlockAction(BlockAction),
    ViewSubmission(ViewSubmission),
    /// A user opened (or switched to) the app's Home tab — the cue to
    /// `views.publish` current state for them. Requires the Slack app to
    /// subscribe to the `app_home_opened` event.
    HomeOpened {
        user_id: String,
    },
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

/// Parses a raw `interactive` envelope's `payload` object.
pub fn parse_interaction(payload: &Value) -> Option<Interaction> {
    match payload.get("type")?.as_str()? {
        "block_actions" => {
            let action = payload.get("actions")?.as_array()?.first()?;
            let action_id = str_field(action, "action_id")?;
            let value = action
                .get("selected_option")
                .and_then(|option| option.get("value"))
                .or_else(|| action.get("value"))?
                .as_str()?
                .to_string();
            Some(Interaction::BlockAction(BlockAction {
                action_id,
                value,
                trigger_id: str_field(payload, "trigger_id")?,
                user_id: str_field(payload.get("user")?, "id")?,
            }))
        }
        "view_submission" => {
            let view = payload.get("view")?;
            Some(Interaction::ViewSubmission(ViewSubmission {
                callback_id: str_field(view, "callback_id")?,
                private_metadata: str_field(view, "private_metadata").unwrap_or_default(),
                view_id: str_field(view, "id")?,
                user_id: str_field(payload.get("user")?, "id")?,
                values: view
                    .get("state")
                    .and_then(|s| s.get("values"))
                    .cloned()
                    .unwrap_or(Value::Null),
            }))
        }
        _ => None,
    }
}

/// Parses an `events_api` envelope's `payload` object, recognizing
/// `app_home_opened` (for the Home tab) and ignoring every other event type
/// (there's nothing else subscribed to yet).
pub fn parse_event(payload: &Value) -> Option<Interaction> {
    if payload.get("type")?.as_str()? != "event_callback" {
        return None;
    }
    let event = payload.get("event")?;
    if event.get("type")?.as_str()? != "app_home_opened" {
        return None;
    }
    Some(Interaction::HomeOpened {
        user_id: str_field(event, "user")?,
    })
}

/// Pulls a single `plain_text_input`/similar element's `value` out of a
/// view submission's `values` object, given the block id and action id it
/// was built with.
pub fn input_value(values: &Value, block_id: &str, action_id: &str) -> Option<String> {
    values
        .get(block_id)?
        .get(action_id)?
        .get("value")?
        .as_str()
        .map(str::to_string)
}

/// Whether a `checkboxes` element's option (by value) was checked in a view
/// submission.
pub fn checkbox_checked(
    values: &Value,
    block_id: &str,
    action_id: &str,
    option_value: &str,
) -> bool {
    let Some(selected) = values
        .get(block_id)
        .and_then(|b| b.get(action_id))
        .and_then(|e| e.get("selected_options"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    selected
        .iter()
        .any(|option| option.get("value").and_then(Value::as_str) == Some(option_value))
}

/// Pulls a `timepicker`'s selected `"HH:MM"` string out, parsed into an
/// hour (0-23). Returns `None` if unset or unparseable.
pub fn timepicker_hour(values: &Value, block_id: &str, action_id: &str) -> Option<u8> {
    let time = values
        .get(block_id)?
        .get(action_id)?
        .get("selected_time")?
        .as_str()?;
    time.split(':').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_overflow_block_action() {
        let payload = json!({
            "type": "block_actions",
            "trigger_id": "trig123",
            "user": { "id": "U1" },
            "actions": [
                { "action_id": "device_overflow", "selected_option": { "value": "forget:aa:bb:cc" } }
            ],
        });
        let interaction = parse_interaction(&payload).unwrap();
        assert_eq!(
            interaction,
            Interaction::BlockAction(BlockAction {
                action_id: "device_overflow".into(),
                value: "forget:aa:bb:cc".into(),
                trigger_id: "trig123".into(),
                user_id: "U1".into(),
            })
        );
    }

    #[test]
    fn parses_button_block_action_value_directly() {
        let payload = json!({
            "type": "block_actions",
            "trigger_id": "trig123",
            "user": { "id": "U1" },
            "actions": [
                { "action_id": "device_card_forget", "value": "forget:aa:bb:cc" }
            ],
        });
        let interaction = parse_interaction(&payload).unwrap();
        assert_eq!(
            interaction,
            Interaction::BlockAction(BlockAction {
                action_id: "device_card_forget".into(),
                value: "forget:aa:bb:cc".into(),
                trigger_id: "trig123".into(),
                user_id: "U1".into(),
            })
        );
    }

    #[test]
    fn parses_view_submission() {
        let payload = json!({
            "type": "view_submission",
            "user": { "id": "U1" },
            "view": {
                "id": "V1",
                "callback_id": "edit_device",
                "private_metadata": "aa:bb:cc",
                "state": { "values": { "label_block": { "label_input": { "value": "Kitchen Pi" } } } },
            },
        });
        let interaction = parse_interaction(&payload).unwrap();
        match interaction {
            Interaction::ViewSubmission(submission) => {
                assert_eq!(submission.callback_id, "edit_device");
                assert_eq!(submission.private_metadata, "aa:bb:cc");
                assert_eq!(submission.view_id, "V1");
                assert_eq!(
                    input_value(&submission.values, "label_block", "label_input"),
                    Some("Kitchen Pi".to_string())
                );
            }
            other => panic!("expected a ViewSubmission, got {other:?}"),
        }
    }

    #[test]
    fn view_submission_without_private_metadata_defaults_to_empty() {
        let payload = json!({
            "type": "view_submission",
            "user": { "id": "U1" },
            "view": { "id": "V1", "callback_id": "quiet_hours", "state": { "values": {} } },
        });
        match parse_interaction(&payload).unwrap() {
            Interaction::ViewSubmission(submission) => assert_eq!(submission.private_metadata, ""),
            other => panic!("expected a ViewSubmission, got {other:?}"),
        }
    }

    #[test]
    fn unknown_interaction_type_is_ignored() {
        let payload = json!({ "type": "shortcut" });
        assert_eq!(parse_interaction(&payload), None);
    }

    #[test]
    fn parses_app_home_opened_event() {
        let payload = json!({
            "type": "event_callback",
            "event": { "type": "app_home_opened", "user": "U1", "tab": "home" },
        });
        assert_eq!(
            parse_event(&payload),
            Some(Interaction::HomeOpened {
                user_id: "U1".into()
            })
        );
    }

    #[test]
    fn ignores_other_event_types() {
        let payload = json!({
            "type": "event_callback",
            "event": { "type": "message", "user": "U1" },
        });
        assert_eq!(parse_event(&payload), None);
    }

    #[test]
    fn ignores_non_event_callback_payloads() {
        let payload = json!({ "type": "url_verification" });
        assert_eq!(parse_event(&payload), None);
    }

    #[test]
    fn checkbox_checked_reads_selected_options() {
        let values = json!({
            "quiet_hours_enabled_block": {
                "quiet_hours_enabled": { "selected_options": [{ "value": "enabled" }] }
            }
        });
        assert!(checkbox_checked(
            &values,
            "quiet_hours_enabled_block",
            "quiet_hours_enabled",
            "enabled"
        ));
        assert!(!checkbox_checked(
            &values,
            "quiet_hours_enabled_block",
            "quiet_hours_enabled",
            "other"
        ));
    }

    #[test]
    fn checkbox_unchecked_when_no_selected_options() {
        let values = json!({ "block": { "action": {} } });
        assert!(!checkbox_checked(&values, "block", "action", "enabled"));
    }

    #[test]
    fn timepicker_hour_parses_hh_mm() {
        let values = json!({ "start_block": { "start_action": { "selected_time": "22:00" } } });
        assert_eq!(
            timepicker_hour(&values, "start_block", "start_action"),
            Some(22)
        );
    }
}
