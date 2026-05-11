//! `lopi-remote`: Telegram bot and Twilio `WhatsApp` webhook for remote control of the lopi orchestrator.

/// Teloxide-based Telegram bot for queueing tasks and querying status.
pub mod telegram;
/// Self-modification automation: diagnosis and Telegram approval gate.
pub mod self_modify;
/// Twilio `WhatsApp` webhook handler with `HMAC-SHA1` signature verification.
pub mod whatsapp;
