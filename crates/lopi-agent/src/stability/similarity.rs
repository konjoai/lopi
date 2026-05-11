//! Pairwise plan similarity and variance computation.
//!
//! Uses token-level Jaccard similarity on normalised word sets.
//! Normalisation: lowercase, alphabetic tokens only, minimum 4 chars.
//! This keeps the metric implementation-language-agnostic — the same
//! vocabulary shows up in Rust, Python, and TypeScript plans for the same
//! task, so cross-language noise is filtered while intent-words are retained.

use std::collections::HashSet;

/// Normalise a plan into a deduplicated word set.
fn token_set(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|t| t.len() >= 4)
        .map(|t| t.to_lowercase())
        .collect()
}

/// Jaccard similarity between two plan strings: |A ∩ B| / |A ∪ B| ∈ [0, 1].
/// Returns 1.0 when both plans are empty (vacuously identical).
pub fn jaccard(a: &str, b: &str) -> f32 {
    let ta = token_set(a);
    let tb = token_set(b);
    if ta.is_empty() && tb.is_empty() {
        return 1.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let inter = ta.intersection(&tb).count() as f64;
    #[allow(clippy::cast_precision_loss)]
    let union = ta.union(&tb).count() as f64;
    if union == 0.0 {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let ratio = (inter / union) as f32;
    ratio
}

/// Compute variance score and consensus index across N plan strings.
///
/// Returns `(variance_score, consensus_idx)`:
/// - `variance_score` = 1.0 − mean_pairwise_similarity ∈ [0, 1]
///   - 0.0 = all plans identical (perfectly stable)
///   - 1.0 = all plans completely disjoint (maximally unstable)
/// - `consensus_idx` = index of the plan with the highest mean similarity
///   to all other plans (the most representative sample)
///
/// Falls back to `(0.0, 0)` for fewer than two plans.
pub fn variance_and_consensus(plans: &[String]) -> (f32, usize) {
    let n = plans.len();
    if n < 2 {
        return (0.0, 0);
    }

    // mean_sim[i] = mean jaccard(plans[i], plans[j]) for j ≠ i
    let mut mean_sim = vec![0.0_f32; n];
    let mut pair_sum = 0.0_f32;
    let mut pair_count = 0u32;

    for i in 0..n {
        let mut row_sum = 0.0_f32;
        for j in 0..n {
            if i == j {
                continue;
            }
            let s = jaccard(&plans[i], &plans[j]);
            row_sum += s;
            if i < j {
                pair_sum += s;
                pair_count += 1;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        {
            mean_sim[i] = row_sum / (n - 1) as f32;
        }
    }

    #[allow(clippy::cast_precision_loss)]
    let mean_pairwise = if pair_count > 0 {
        pair_sum / pair_count as f32
    } else {
        1.0
    };

    let variance_score = (1.0_f32 - mean_pairwise).clamp(0.0, 1.0);

    let consensus_idx = mean_sim
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0);

    (variance_score, consensus_idx)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn jaccard_identical() {
        assert!((jaccard("add error handling to the parser", "add error handling to the parser") - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn jaccard_disjoint() {
        // completely different words → 0
        assert_eq!(jaccard("alpha beta gamma delta", "zeta theta kappa sigma"), 0.0);
    }

    #[test]
    fn jaccard_partial_overlap() {
        let s = jaccard("must handle errors gracefully", "must handle timeouts carefully");
        assert!(s > 0.0 && s < 1.0, "expected partial overlap, got {s}");
    }

    #[test]
    fn jaccard_both_empty() {
        assert!((jaccard("", "") - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn jaccard_one_empty() {
        assert_eq!(jaccard("", "some plan text here"), 0.0);
    }

    #[test]
    fn jaccard_case_insensitive() {
        let upper = jaccard("Parse The Token Stream", "parse the token stream");
        assert!((upper - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn jaccard_filters_short_tokens() {
        // "is", "an", "of" are shorter than 4 chars and filtered
        let s = jaccard("this is an example of parsing", "this is an example of lexing");
        assert!(s < 1.0); // "lexing" vs "parsing" differ
    }

    #[test]
    fn variance_identical_plans() {
        let plans: Vec<String> = (0..3)
            .map(|_| "step one: write the function step two: add tests".to_string())
            .collect();
        let (variance, _) = variance_and_consensus(&plans);
        assert!(variance < 0.01, "identical plans → near-zero variance, got {variance}");
    }

    #[test]
    fn variance_single_plan() {
        let plans = vec!["only one plan".to_string()];
        let (variance, idx) = variance_and_consensus(&plans);
        assert_eq!(variance, 0.0);
        assert_eq!(idx, 0);
    }

    #[test]
    fn variance_empty_slice() {
        let (variance, idx) = variance_and_consensus(&[]);
        assert_eq!(variance, 0.0);
        assert_eq!(idx, 0);
    }

    #[test]
    fn consensus_picks_most_central_plan() {
        // plan[0] shares words with plan[1] and plan[2]; plan[3] is the outlier
        let plans = vec![
            "handle errors in the parser with proper recovery".to_string(),
            "handle errors gracefully using recovery routines".to_string(),
            "handle errors and ensure recovery paths work".to_string(),
            "completely different unrelated system task foobar".to_string(),
        ];
        let (_, idx) = variance_and_consensus(&plans);
        // Consensus should be one of plans 0-2, not the outlier (plan 3)
        assert!(idx < 3, "consensus should not pick the outlier, got idx={idx}");
    }

    #[test]
    fn variance_score_in_unit_interval() {
        let plans = vec![
            "step one write code".to_string(),
            "completely different unrelated words here always".to_string(),
        ];
        let (variance, _) = variance_and_consensus(&plans);
        assert!((0.0..=1.0).contains(&variance), "variance {variance} out of [0,1]");
    }
}
