//! `lopi trust` — show trust calibration stats derived from pattern annotations.
//!
//! Reads the annotated pattern ledger to show:
//! - How many patterns have been approved / rejected
//! - Current score weight adjustments being applied
//! - Reliability signal (approved avg attempts vs rejected avg attempts)

use anyhow::Result;
use lopi_memory::MemoryStore;

use crate::util::db_path;

pub async fn show() -> Result<()> {
    let store = MemoryStore::open(db_path()).await?;
    let annotated = store.load_annotated_patterns().await?;

    let approved: Vec<_> = annotated
        .iter()
        .filter(|p| p.user_annotation.as_deref() == Some("approved"))
        .collect();
    let rejected: Vec<_> = annotated
        .iter()
        .filter(|p| p.user_annotation.as_deref() == Some("rejected"))
        .collect();

    println!("🎯 lopi trust calibration\n");
    println!("  Annotated patterns: {}", annotated.len());
    println!("  ✅ approved: {}", approved.len());
    println!("  ❌ rejected: {}", rejected.len());

    if annotated.is_empty() {
        println!();
        println!("  No annotated patterns yet.");
        println!("  Use `lopi learn annotate <id> approved|rejected` to build the trust ledger.");
        return Ok(());
    }

    let avg_attempts = |patterns: &[&lopi_memory::PatternRow]| -> f64 {
        if patterns.is_empty() {
            return 0.0;
        }
        patterns.iter().filter_map(|p| p.avg_attempts).sum::<f64>() / patterns.len() as f64
    };

    let approved_avg = avg_attempts(&approved);
    let rejected_avg = avg_attempts(&rejected);
    println!();
    println!("  Avg attempts — approved: {approved_avg:.1}  rejected: {rejected_avg:.1}");

    if approved_avg > 0.0 || rejected_avg > 0.0 {
        let signal = rejected_avg - approved_avg;
        let direction = if signal > 0.1 {
            "tightening quality bar (rejected patterns needed more attempts)"
        } else if signal < -0.1 {
            "loosening quality bar (approved patterns needed more attempts)"
        } else {
            "balanced — weights at defaults"
        };
        println!("  Signal: {signal:+.2} → {direction}");
    }

    // Show computed weights.
    let weights = store.compute_weight_adjustments().await?;
    println!();
    println!("  Current score weights (from trust calibration):");
    println!(
        "    lint_penalty_per_error:  {:.4}",
        weights.lint_penalty_per_error
    );
    println!(
        "    lint_penalty_cap:        {:.4}",
        weights.lint_penalty_cap
    );
    println!(
        "    diff_penalty_per_kloc:   {:.4}",
        weights.diff_penalty_per_kloc
    );
    println!(
        "    diff_penalty_cap:        {:.4}",
        weights.diff_penalty_cap
    );

    if weights.lint_penalty_per_error == lopi_core::ScoreWeights::default().lint_penalty_per_error {
        println!();
        println!("  (defaults — annotate more patterns to calibrate)");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles() {}
}
