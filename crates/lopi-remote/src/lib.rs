//! `lopi-remote`: Telegram bot and Twilio `WhatsApp` webhook for remote control of the lopi orchestrator.

/// Self-modification automation: diagnosis and Telegram approval gate.
pub mod self_modify;
/// Teloxide-based Telegram bot for queueing tasks and querying status.
pub mod telegram;
/// Twilio `WhatsApp` webhook handler with `HMAC-SHA1` signature verification.
pub mod whatsapp;
