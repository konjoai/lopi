//! Report on Finish (Loop Engineering primitive 6) — the channel a finished
//! run's summary can be routed to, instead of only being logged locally.
//!
//! This is deliberately tiny: today there is exactly one reachable channel
//! (`"telegram"`) because that is the only outbound-send path that exists
//! ([`crate::config::RemoteConfig::telegram`] / `lopi-remote`'s Telegram bot).
//! WhatsApp (`crate::config::RemoteConfig::whatsapp`) is inbound-only — a
//! Twilio webhook receiver with no send function — so it is a *named,
//! explained* rejection rather than falling into the generic "unknown
//! channel" bucket.

use thiserror::Error;

/// A destination a completed run's report summary can be sent to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportChannel {
    /// Send via the already-configured Telegram bot
    /// ([`crate::config::TelegramConfig`]).
    Telegram,
}

/// Why a `report` channel name failed to validate.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReportChannelError {
    /// The name isn't any channel lopi knows about.
    #[error("unknown report channel `{0}` — valid channels: telegram")]
    Unknown(String),
    /// WhatsApp is recognized but has no outbound-send path yet.
    #[error(
        "report channel `whatsapp` has no outbound-send path yet (inbound-only Twilio webhook) — use `telegram`"
    )]
    WhatsappUnsupported,
}

impl ReportChannel {
    /// Parse a `report` field's channel name, e.g. from
    /// [`crate::config::ScheduleEntry::report`] or [`crate::task::Task::report`].
    ///
    /// # Errors
    /// Returns [`ReportChannelError::WhatsappUnsupported`] for `"whatsapp"`
    /// (named, not silently dropped) and [`ReportChannelError::Unknown`] for
    /// any other unrecognized name.
    pub fn parse(name: &str) -> Result<Self, ReportChannelError> {
        match name {
            "telegram" => Ok(Self::Telegram),
            "whatsapp" => Err(ReportChannelError::WhatsappUnsupported),
            other => Err(ReportChannelError::Unknown(other.to_string())),
        }
    }

    /// The canonical string form, the inverse of [`Self::parse`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_telegram() {
        assert_eq!(
            ReportChannel::parse("telegram"),
            Ok(ReportChannel::Telegram)
        );
    }

    #[test]
    fn round_trips_as_str() {
        assert_eq!(ReportChannel::Telegram.as_str(), "telegram");
        assert_eq!(
            ReportChannel::parse(ReportChannel::Telegram.as_str()),
            Ok(ReportChannel::Telegram)
        );
    }

    #[test]
    fn whatsapp_names_the_inbound_only_reason() {
        let err = ReportChannel::parse("whatsapp").unwrap_err();
        assert_eq!(err, ReportChannelError::WhatsappUnsupported);
        assert!(err.to_string().contains("inbound-only"));
    }

    #[test]
    fn unknown_channel_names_itself_in_the_error() {
        let err = ReportChannel::parse("carrier-pigeon").unwrap_err();
        assert_eq!(
            err,
            ReportChannelError::Unknown("carrier-pigeon".to_string())
        );
        assert!(err.to_string().contains("carrier-pigeon"));
    }
}
