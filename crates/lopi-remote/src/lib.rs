//! `lopi-remote`: Twilio `WhatsApp` webhook for remote control of the lopi orchestrator.
//!
//! Sprint S10, Phase 4 removed the Telegram bot transport (`teloxide`,
//! ~2,024 LOC): `is_untrusted_source` classified `TaskSource::Telegram` as
//! untrusted but Sprint S2 Phase 5 deliberately never extended the trifecta
//! human-approval gate to it (see `LEDGER.md`'s Sprint S2 entry) — the one
//! untrusted source classified untrusted and left ungated. The iOS/macOS
//! app covers the remote-control use case the bot existed for. The
//! `TaskSource::Telegram { chat_id, message_id }` variant itself is
//! **not** removed — it is a durable enum persisted in `tasks.source`, and
//! historical rows still deserialize through it (`lopi diag`, `lopi
//! replay`, the dashboard task list, `audit_log` queries) via
//! `lopi_core::TaskSource`, `TaskRow::provenance()`, and
//! `is_untrusted_source` — see `.konjo/killtests/S10/KT-S10.3.md`.

#![warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// Twilio `WhatsApp` webhook handler with `HMAC-SHA1` signature verification.
pub mod whatsapp;
