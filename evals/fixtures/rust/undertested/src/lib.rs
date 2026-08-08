//! Deliberately under-tested fixture for the mutation-hunt loop's real end-to-end
//! verify run (review-pipeline Sprint P2b, section 3). Two small functions with real
//! branch/boundary logic and one weak test, so `cargo mutants --in-diff` has genuine
//! surviving mutants for the loop to work against -- not consumed by any other crate.

/// Loyalty-tier discount, in percentage points. A tenure bonus applies at the 5-year
/// mark regardless of tier.
pub fn tier_discount_pct(tier: &str, years_active: u32) -> u32 {
    let base = match tier {
        "gold" => 15,
        "silver" => 8,
        _ => 0,
    };
    if years_active >= 5 {
        base + 5
    } else {
        base
    }
}

/// Clamp a raw score into the 0..=100 range.
pub fn clamp_score(raw: i32) -> u32 {
    if raw < 0 {
        0
    } else if raw > 100 {
        100
    } else {
        raw as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_tier_has_a_discount() {
        assert!(tier_discount_pct("gold", 1) > 0);
    }
}
