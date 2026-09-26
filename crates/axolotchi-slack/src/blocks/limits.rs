//! Slack Block Kit limits, enforced here so a builder can never accidentally
//! emit a payload Slack would reject at the API boundary.

/// Home tab and modal views both cap out at 100 blocks total.
pub const MAX_VIEW_BLOCKS: usize = 100;
/// How many device rows the Home view renders before summarizing the rest
/// in a trailing context block. Well under `MAX_VIEW_BLOCKS` since a
/// section-with-accessory is one block per device.
pub const MAX_HOME_DEVICES: usize = 40;
pub const MAX_TEXT_CHARS: usize = 3000;
pub const MAX_CONTEXT_ELEMENTS: usize = 10;
pub const MAX_OVERFLOW_OPTIONS: usize = 5;
pub const MAX_HEADER_CHARS: usize = 150;
/// Modal (and Home) view titles are capped at 24 characters by the Slack API.
pub const MAX_MODAL_TITLE_CHARS: usize = 24;

/// Truncates to at most `max` characters, marking that it was cut rather
/// than silently producing a shorter-than-expected string.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(max.saturating_sub(1)).collect();
    truncated.push('\u{2026}');
    truncated
}

/// Caps an overflow menu's options to Slack's limit, keeping the first
/// (highest-priority) ones rather than letting the API reject the whole
/// payload if a caller ever adds a 6th option.
pub fn cap_overflow_options<T>(mut options: Vec<T>) -> Vec<T> {
    options.truncate(MAX_OVERFLOW_OPTIONS);
    options
}

/// Caps a context block's elements to Slack's limit.
pub fn cap_context_elements<T>(mut elements: Vec<T>) -> Vec<T> {
    elements.truncate(MAX_CONTEXT_ELEMENTS);
    elements
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn long_text_is_cut_with_a_marker() {
        let result = truncate("hello world", 6);
        assert_eq!(result.chars().count(), 6);
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn exact_length_is_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn overflow_options_are_capped_at_five() {
        let options: Vec<i32> = (0..8).collect();
        assert_eq!(cap_overflow_options(options), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn context_elements_are_capped_at_ten() {
        let elements: Vec<i32> = (0..15).collect();
        assert_eq!(cap_context_elements(elements).len(), MAX_CONTEXT_ELEMENTS);
    }
}
