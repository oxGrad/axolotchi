//! The rename modal. The device id rides in `private_metadata` rather than
//! `callback_id`, since `callback_id` identifies the modal *type* for
//! routing while `private_metadata` carries this instance's data.

use super::limits::{truncate, MAX_MODAL_TITLE_CHARS};
use super::view_models::EditModalViewModel;
use serde_json::{json, Value};

pub fn edit_modal(vm: &EditModalViewModel) -> Value {
    json!({
        "type": "modal",
        "callback_id": "edit_device",
        "private_metadata": vm.device_id,
        "title": { "type": "plain_text", "text": truncate("Rename device", MAX_MODAL_TITLE_CHARS) },
        "submit": { "type": "plain_text", "text": "Save" },
        "close": { "type": "plain_text", "text": "Cancel" },
        "blocks": [
            {
                "type": "input",
                "block_id": "label_block",
                "label": { "type": "plain_text", "text": "Nickname" },
                "element": {
                    "type": "plain_text_input",
                    "action_id": "label_input",
                    "initial_value": vm.current_label,
                    "max_length": 80,
                },
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_edit_modal() {
        let vm = EditModalViewModel {
            device_id: "aa:bb:cc:dd:ee:01".into(),
            current_label: "Kitchen Pi".into(),
        };
        insta::assert_json_snapshot!(edit_modal(&vm));
    }

    #[test]
    fn device_id_carried_in_private_metadata_not_callback_id() {
        let vm = EditModalViewModel {
            device_id: "aa:bb:cc:dd:ee:01".into(),
            current_label: "Kitchen Pi".into(),
        };
        let view = edit_modal(&vm);
        assert_eq!(view["private_metadata"], "aa:bb:cc:dd:ee:01");
        assert_eq!(view["callback_id"], "edit_device");
    }
}
