use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Where a user-facing metric's value came from — attached to every number
/// lopi displays (cost, token count, coverage %, etc.) so an unlabeled
/// number is a compile error, not an oversight. See `docs/MEASUREMENT.md`
/// for the policy this type enforces.
///
/// Distinct from `lopi_memory::store::task_row::TaskRow::provenance()`
/// (a *trust* classification for where a task's `source` came from —
/// `"operator"` / `"untrusted"` / `"unknown"`). This type is a *measurement
/// confidence* classification and is unrelated. When serializing this type
/// into a JSON API response, downstream code must use the field name
/// `measurement_provenance`, never bare `provenance`, to avoid colliding
/// with that existing `TaskRow` field's JSON key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Provenance {
    /// lopi counted this itself, directly, from its own runs (e.g. summed
    /// from its own `turn_metrics` table).
    Measured {
        /// What lopi counted and how (e.g. `"turn_metrics table (this process's own agent runs)"`).
        source: String,
    },
    /// A tool or external API reported this value; lopi passes it through
    /// without independently re-deriving it (e.g. the `claude` CLI's own
    /// authoritative `total_cost_usd`).
    Reported {
        /// What reported it (e.g. `"claude CLI result.total_cost_usd"`).
        source: String,
        /// When lopi last received this reported value.
        as_of: DateTime<Utc>,
    },
    /// Derived from measured values plus assumptions that can go stale
    /// (e.g. a token count multiplied against a price table).
    Estimated {
        /// What assumption/basis the estimate rests on (e.g. `"pricing.toml rates x token counts"`).
        basis: String,
        /// When the basis (e.g. the price table) was last verified current.
        as_of: DateTime<Utc>,
    },
    /// Known to exist in principle, but not obtainable through means lopi
    /// is willing to use (see `docs/MEASUREMENT.md`'s three prohibitions) —
    /// e.g. plan quota, which would require bypassing bot protection or an
    /// undocumented internal API to read.
    Unavailable {
        /// Why lopi can't show this (e.g. `"requires an undocumented Anthropic account-usage API"`).
        reason: String,
    },
}

impl Provenance {
    /// A short, human-readable one-line label suitable for a CLI/TUI status
    /// line next to the value it describes (e.g. `"measured — lopi's own runs"`).
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Measured { source } => format!("measured — {source}"),
            Self::Reported { source, as_of } => {
                format!("reported by {source}, as of {}", as_of.format("%Y-%m-%d"))
            }
            Self::Estimated { basis, as_of } => {
                format!("estimated — {basis}, as of {}", as_of.format("%Y-%m-%d"))
            }
            Self::Unavailable { reason } => format!("unavailable — {reason}"),
        }
    }

    /// Convenience constructor for the common "lopi measured this itself" case.
    #[must_use]
    pub fn measured(source: impl Into<String>) -> Self {
        Self::Measured {
            source: source.into(),
        }
    }

    /// Convenience constructor for `Unavailable`.
    #[must_use]
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_serde_round_trip_and_tag() {
        let p = Provenance::measured("turn_metrics table");
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"kind\":\"measured\""));
        let back: Provenance = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn reported_serde_round_trip_and_tag() {
        let as_of = Utc::now();
        let p = Provenance::Reported {
            source: "claude CLI result.total_cost_usd".to_string(),
            as_of,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"kind\":\"reported\""));
        let back: Provenance = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
        match back {
            Provenance::Reported { as_of: got, .. } => assert_eq!(got, as_of),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn estimated_serde_round_trip_and_tag() {
        let as_of = Utc::now();
        let p = Provenance::Estimated {
            basis: "pricing.toml rates x token counts".to_string(),
            as_of,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"kind\":\"estimated\""));
        let back: Provenance = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
        match back {
            Provenance::Estimated { as_of: got, .. } => assert_eq!(got, as_of),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn unavailable_serde_round_trip_and_tag() {
        let p = Provenance::unavailable("requires an undocumented Anthropic account-usage API");
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"kind\":\"unavailable\""));
        let back: Provenance = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn label_measured_contains_source() {
        let label = Provenance::measured("x").label();
        assert!(label.contains("measured"));
        assert!(label.contains('x'));
    }

    #[test]
    fn label_reported_contains_source_and_date() {
        let as_of = Utc::now();
        let label = Provenance::Reported {
            source: "claude CLI".to_string(),
            as_of,
        }
        .label();
        assert!(label.contains("reported"));
        assert!(label.contains("claude CLI"));
        assert!(label.contains(&as_of.format("%Y-%m-%d").to_string()));
    }

    #[test]
    fn label_estimated_contains_basis_and_date() {
        let as_of = Utc::now();
        let label = Provenance::Estimated {
            basis: "pricing.toml".to_string(),
            as_of,
        }
        .label();
        assert!(label.contains("estimated"));
        assert!(label.contains("pricing.toml"));
        assert!(label.contains(&as_of.format("%Y-%m-%d").to_string()));
    }

    #[test]
    fn label_unavailable_contains_reason() {
        let label = Provenance::unavailable("no API").label();
        assert!(label.contains("unavailable"));
        assert!(label.contains("no API"));
    }

    #[test]
    fn as_of_survives_round_trip_not_dropped() {
        let as_of = DateTime::parse_from_rfc3339("2026-01-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let p = Provenance::Estimated {
            basis: "basis".to_string(),
            as_of,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("2026-01-15"));
        let back: Provenance = serde_json::from_str(&json).unwrap();
        match back {
            Provenance::Estimated { as_of: got, .. } => assert_eq!(got, as_of),
            _ => panic!("wrong variant"),
        }
    }
}
