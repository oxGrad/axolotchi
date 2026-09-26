//! A dedicated confirmation modal for "Forget", opened in place of Slack's
//! overflow `confirm` object — that confirm would apply to the whole menu
//! (Details/Rename/Forget alike), not just the destructive option.

use super::limits::{truncate, MAX_MODAL_TITLE_CHARS};
use super::view_models::ForgetConfirmViewModel;
use serde_json::{json, Value};

pub fn forget_confirm_modal(vm: &ForgetConfirmViewModel) -> Value {
    json!({
        "type": "modal",
        "callback_id": "forget_device_confirm",
        "private_metadata": vm.device_id,
        "title": { "type": "plain_text", "text": truncate("Forget device?", MAX_MODAL_TITLE_CHARS) },
        "submit": { "type": "plain_text", "text": "Forget" },
        "close": { "type": "plain_text", "text": "Cancel" },
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!("Axo will forget *{}* completely. This can't be undone.", vm.device_label),
                },
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_forget_confirm_modal() {
        let vm = ForgetConfirmViewModel {
            device_id: "aa:bb:cc:dd:ee:01".into(),
            device_label: "Kitchen Pi".into(),
        };
        insta::assert_json_snapshot!(forget_confirm_modal(&vm));
    }

    #[test]
    fn submit_button_is_labeled_forget() {
        let vm = ForgetConfirmViewModel {
            device_id: "aa:bb:cc:dd:ee:01".into(),
            device_label: "Kitchen Pi".into(),
        };
        let view = forget_confirm_modal(&vm);
        assert_eq!(view["submit"]["text"], "Forget");
    }
}
