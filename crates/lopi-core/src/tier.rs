//! Customer subscription tier — controls concurrent agent cap and feature flags.
//!
//! `CustomerTier` lives in `lopi-core` (tier 1) so both `lopi-memory` and
//! `lopi-app` can reference it without a circular dependency.

use serde::{Deserialize, Serialize};

/// Subscription tier for a lopi customer.
///
/// Controls the maximum number of concurrent agents and which features are
/// available. The tier is stored in the `github_installations` table and
/// updated by Stripe subscription webhook events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CustomerTier {
    /// Unsubscribed — single agent, community features only.
    Free,
    /// $299 / month — 4 agents, Telegram remote, pattern learning.
    Starter,
    /// $999 / month — 16 agents, multi-repo, result caching.
    Growth,
    /// $4 999 / month — 64 agents, dedicated infra, SLA 99.9 %.
    Enterprise,
}

impl CustomerTier {
    /// Maximum concurrent agents allowed for this tier.
    #[must_use]
    pub fn max_agents(self) -> usize {
        match self {
            Self::Free => 1,
            Self::Starter => 4,
            Self::Growth => 16,
            Self::Enterprise => 64,
        }
    }

    /// Human-readable display name.
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::Starter => "Starter",
            Self::Growth => "Growth",
            Self::Enterprise => "Enterprise",
        }
    }

    /// Monthly price in USD cents (0 for Free).
    #[must_use]
    pub fn price_usd_cents_per_month(self) -> u32 {
        match self {
            Self::Free => 0,
            Self::Starter => 29_900,
            Self::Growth => 99_900,
            Self::Enterprise => 499_900,
        }
    }

    /// Features included at this tier (for `/api/plans` display).
    #[must_use]
    pub fn features(self) -> &'static [&'static str] {
        match self {
            Self::Free => &[
                "1 concurrent agent",
                "SQLite memory + pattern learning",
                "TUI dashboard",
                "Git-isolated branches",
            ],
            Self::Starter => &[
                "4 concurrent agents",
                "SQLite memory + pattern learning",
                "TUI + web Forge dashboard",
                "Telegram remote control",
                "GitHub issue triage",
                "Webhook CI integration",
            ],
            Self::Growth => &[
                "16 concurrent agents",
                "Multi-repo dispatch mode",
                "Result caching",
                "Tool registry",
                "OTel observability",
                "Priority support",
            ],
            Self::Enterprise => &[
                "64 concurrent agents",
                "Dedicated infrastructure",
                "Custom integrations",
                "SLA 99.9%",
                "SAML SSO",
                "Audit log export",
                "Slack support channel",
            ],
        }
    }

    /// Wire-format string stored in SQLite `tier` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Starter => "starter",
            Self::Growth => "growth",
            Self::Enterprise => "enterprise",
        }
    }

    /// Parse from a Stripe product name or plan nickname string.
    /// Case-insensitive; unknown strings map to `Free`.
    #[must_use]
    pub fn from_stripe_name(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "starter" => Self::Starter,
            "growth" => Self::Growth,
            "enterprise" => Self::Enterprise,
            _ => Self::Free,
        }
    }
}

impl std::fmt::Display for CustomerTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for CustomerTier {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "starter" => Self::Starter,
            "growth" => Self::Growth,
            "enterprise" => Self::Enterprise,
            _ => Self::Free,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn max_agents_per_tier() {
        assert_eq!(CustomerTier::Free.max_agents(), 1);
        assert_eq!(CustomerTier::Starter.max_agents(), 4);
        assert_eq!(CustomerTier::Growth.max_agents(), 16);
        assert_eq!(CustomerTier::Enterprise.max_agents(), 64);
    }

    #[test]
    fn serde_round_trip() {
        for tier in [
            CustomerTier::Free,
            CustomerTier::Starter,
            CustomerTier::Growth,
            CustomerTier::Enterprise,
        ] {
            let json = serde_json::to_string(&tier).unwrap();
            let back: CustomerTier = serde_json::from_str(&json).unwrap();
            assert_eq!(tier, back);
        }
    }

    #[test]
    fn from_stripe_name_case_insensitive() {
        assert_eq!(
            CustomerTier::from_stripe_name("Starter"),
            CustomerTier::Starter
        );
        assert_eq!(
            CustomerTier::from_stripe_name("GROWTH"),
            CustomerTier::Growth
        );
        assert_eq!(
            CustomerTier::from_stripe_name("enterprise"),
            CustomerTier::Enterprise
        );
        assert_eq!(
            CustomerTier::from_stripe_name("unknown"),
            CustomerTier::Free
        );
        assert_eq!(CustomerTier::from_stripe_name(""), CustomerTier::Free);
    }

    #[test]
    fn from_str_parse() {
        assert_eq!(
            "starter".parse::<CustomerTier>().unwrap(),
            CustomerTier::Starter
        );
        assert_eq!(
            "growth".parse::<CustomerTier>().unwrap(),
            CustomerTier::Growth
        );
        assert_eq!(
            "enterprise".parse::<CustomerTier>().unwrap(),
            CustomerTier::Enterprise
        );
        assert_eq!("free".parse::<CustomerTier>().unwrap(), CustomerTier::Free);
        assert_eq!(
            "garbage".parse::<CustomerTier>().unwrap(),
            CustomerTier::Free
        );
    }

    #[test]
    fn display_matches_as_str() {
        for tier in [
            CustomerTier::Free,
            CustomerTier::Starter,
            CustomerTier::Growth,
            CustomerTier::Enterprise,
        ] {
            assert_eq!(format!("{tier}"), tier.as_str());
        }
    }

    #[test]
    fn price_ordering() {
        assert!(
            CustomerTier::Starter.price_usd_cents_per_month()
                > CustomerTier::Free.price_usd_cents_per_month()
        );
        assert!(
            CustomerTier::Growth.price_usd_cents_per_month()
                > CustomerTier::Starter.price_usd_cents_per_month()
        );
        assert!(
            CustomerTier::Enterprise.price_usd_cents_per_month()
                > CustomerTier::Growth.price_usd_cents_per_month()
        );
    }
}
