//! Pure formatting helpers for Telegram messages.
use chrono::{DateTime, Utc};
use lopi_core::Priority;

/// Return the first 8 characters of an ID string (or fewer if the string is shorter).
#[must_use]
pub fn short_id(id: &str) -> &str {
    &id[..id.len().min(8)]
}

/// Short badge string for a priority level used in fleet/queue displays.
#[must_use]
pub fn priority_badge(p: Priority) -> &'static str {
    match p {
        Priority::Critical => "[CRIT]",
        Priority::High => "[HIGH]",
        Priority::Normal => "[NORM]",
        Priority::Low => "[LOW]",
    }
}

/// Status emoji based on a task status string from the database.
#[must_use]
pub fn status_emoji(status: &str) -> &'static str {
    let s = status.to_lowercase();
    if s.contains("success") || s == "succeeded" {
        "✅"
    } else if s.contains("fail") {
        "❌"
    } else if s == "queued" {
        "🔵"
    } else if s.contains("running")
        || s.contains("planning")
        || s.contains("implementing")
        || s.contains("testing")
        || s.contains("scoring")
        || s.contains("retrying")
    {
        "⏳"
    } else if s.contains("cancel") {
        "🗑"
    } else if s.contains("rolled") {
        "↩️"
    } else {
        "•"
    }
}

/// Human-readable relative time string for a past datetime.
#[must_use]
pub fn relative_time(dt: DateTime<Utc>) -> String {
    let secs = Utc::now().signed_duration_since(dt).num_seconds();
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3_600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3_600)
    } else if secs < 172_800 {
        "yesterday".to_string()
    } else {
        format!("{} days ago", secs / 86_400)
    }
}

/// Format an uptime duration in seconds as a human-readable string.
#[must_use]
pub fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h {}m", secs / 3_600, (secs % 3_600) / 60)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_short_id_full_uuid() {
        let uuid = "a3f2c1b8-dead-beef-cafe-123456789012";
        assert_eq!(short_id(uuid), "a3f2c1b8");
    }

    #[test]
    fn test_short_id_already_short() {
        let s = "abc";
        assert_eq!(short_id(s), "abc");
    }

    #[test]
    fn test_priority_badge_all_variants() {
        assert_eq!(priority_badge(Priority::Critical), "[CRIT]");
        assert_eq!(priority_badge(Priority::High), "[HIGH]");
        assert_eq!(priority_badge(Priority::Normal), "[NORM]");
        assert_eq!(priority_badge(Priority::Low), "[LOW]");
    }

    #[test]
    fn test_status_emoji_succeeded() {
        assert_eq!(status_emoji("succeeded"), "✅");
        assert_eq!(status_emoji("success"), "✅");
    }

    #[test]
    fn test_status_emoji_failed() {
        assert_eq!(status_emoji("failed"), "❌");
        assert_eq!(status_emoji("fail"), "❌");
    }

    #[test]
    fn test_status_emoji_running() {
        assert_eq!(status_emoji("running"), "⏳");
        assert_eq!(status_emoji("planning"), "⏳");
        assert_eq!(status_emoji("implementing"), "⏳");
        assert_eq!(status_emoji("testing"), "⏳");
    }

    #[test]
    fn test_relative_time_seconds() {
        let dt = Utc::now() - Duration::seconds(30);
        assert_eq!(relative_time(dt), "just now");
    }

    #[test]
    fn test_relative_time_minutes() {
        let dt = Utc::now() - Duration::seconds(90);
        assert_eq!(relative_time(dt), "1m ago");
    }

    #[test]
    fn test_relative_time_hours() {
        let dt = Utc::now() - Duration::seconds(5_400); // 90 minutes
        assert_eq!(relative_time(dt), "1h ago");
    }

    #[test]
    fn test_relative_time_days() {
        let dt = Utc::now() - Duration::hours(25);
        let result = relative_time(dt);
        assert!(result == "yesterday" || result.contains("day"));
    }
}
