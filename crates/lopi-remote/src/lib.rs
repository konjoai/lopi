//! `lopi-remote`: Telegram bot and Twilio `WhatsApp` webhook for remote control of the lopi orchestrator.

#![warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// Deny-by-default allowlist gating proactive/automated outbound sends.
pub mod egress;
/// Teloxide-based Telegram bot for queueing tasks and querying status.
pub mod telegram;
/// Twilio `WhatsApp` webhook handler with `HMAC-SHA1` signature verification.
pub mod whatsapp;
