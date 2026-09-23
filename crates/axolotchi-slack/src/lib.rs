//! Slack integration: Socket Mode routing and (later) Block Kit builders.
//!
//! Socket Mode only — outbound websocket, no inbound ports on the Pi. Every
//! envelope is acked immediately, before any slower work (parsing into a
//! command, pushing onto the engine channel) happens, per the Slack rules
//! in `CLAUDE.md`: never block the ack on SQLite or anything else.

mod socket_mode;

pub use socket_mode::{parse_command, respond, run, Config, SlackError, SlashCommandInvocation};
