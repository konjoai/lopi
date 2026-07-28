//! Sprint T0 — a card's cron/MAXX scheduling types, split out of `stack.rs`
//! to keep that file under the 500-line CI file-size gate.

use crate::config::LimitWindow;
use serde::{Deserialize, Serialize};

/// How often a scheduled card's cron fires (`StackTypes.swift:103-110`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CronFreq {
    /// Every minute.
    #[serde(rename = "every minute")]
    EveryMinute,
    /// Once an hour.
    Hourly,
    /// Once a day.
    Daily,
    /// Once a week.
    Weekly,
    /// A hand-written cron expression.
    Custom,
}

/// Day of week for a weekly cron (`StackTypes.swift:118-120`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dow {
    /// Sunday.
    Sun,
    /// Monday.
    Mon,
    /// Tuesday.
    Tue,
    /// Wednesday.
    Wed,
    /// Thursday.
    Thu,
    /// Friday.
    Fri,
    /// Saturday.
    Sat,
}

/// AM/PM for a 12-hour cron picker (`StackTypes.swift:123`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmPm {
    /// Before noon.
    AM,
    /// From noon on.
    PM,
}

/// A card's schedule configuration (`StackTypes.swift:126-138`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CronConfig {
    /// How often the schedule fires.
    pub freq: CronFreq,
    /// Hour in 12-hour form, `1..=12`.
    pub hour12: u8,
    /// Minute, `0..=59`.
    pub min: u8,
    /// AM or PM.
    pub ampm: AmPm,
    /// Day of week (used when `freq == Weekly`).
    pub dow: Dow,
    /// The resolved raw 5-field cron expression.
    pub raw: String,
}

/// A fresh card's cron config: daily at 2:00 AM, matching the resolved raw
/// expression `"0 2 * * *"`.
#[must_use]
pub fn default_cron() -> CronConfig {
    CronConfig {
        freq: CronFreq::Daily,
        hour12: 2,
        min: 0,
        ampm: AmPm::AM,
        dow: Dow::Mon,
        raw: "0 2 * * *".to_string(),
    }
}

/// MAXX (autonomous continuation) settings for a card
/// (`StackTypes.swift:154-174`). Reuses [`LimitWindow`]
/// (`crate::config::LimitWindow`) rather than redefining it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaxxConfig {
    /// Whether MAXX continuation is enabled for this card.
    pub enabled: bool,
    /// `[start, end]` local hours MAXX must stay quiet during, e.g.
    /// `[23, 7]` for 11PM-7AM.
    pub quiet_hours: [u8; 2],
    /// Whether MAXX must check quota headroom before continuing.
    pub headroom_gate: bool,
    /// Which rolling rate-limit windows MAXX respects.
    pub windows: Vec<LimitWindow>,
}

/// A fresh card's MAXX config: disabled, quiet 11PM-7AM, headroom-gated,
/// both windows respected.
#[must_use]
pub fn default_maxx() -> MaxxConfig {
    MaxxConfig {
        enabled: false,
        quiet_hours: [23, 7],
        headroom_gate: true,
        windows: vec![LimitWindow::FiveHour, LimitWindow::SevenDay],
    }
}
