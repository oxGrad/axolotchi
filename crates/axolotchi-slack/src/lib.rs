//! Slack integration: Socket Mode routing and Block Kit builders.
//!
//! Socket Mode only — outbound websocket, no inbound ports on the Pi. Every
//! envelope is acked immediately, before any slower work (parsing into a
//! command, pushing onto the engine channel) happens, per the Slack rules
//! in `CLAUDE.md`: never block the ack on SQLite or anything else.

pub mod blocks;
mod interactions;
mod socket_mode;

pub use interactions::{
    checkbox_checked, input_value, timepicker_hour, BlockAction, Interaction, Reaction,
    ViewSubmission,
};
pub use socket_mode::{
    chat_post_message, chat_update, parse_command, pins_add, respond, run, views_open,
    views_publish, views_push, Config, SlackError, SlashCommandInvocation,
};
