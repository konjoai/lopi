use anyhow::Result;
use lopi_core::ScheduleEntry;
use lopi_orchestrator::next_run_times;

pub async fn list(schedules: Vec<ScheduleEntry>) -> Result<()> {
    if schedules.is_empty() {
        println!("⏰ lopi schedules — none configured");
        println!();
        println!("  Add [[schedules]] entries to lopi.toml:");
        println!();
        println!("  [[schedules]]");
        println!("  name = \"nightly-lint\"");
        println!("  repo = \"/path/to/repo\"");
        println!("  goal = \"Fix all clippy warnings\"");
        println!("  cron = \"0 2 * * *\"");
        return Ok(());
    }

    println!("⏰ lopi schedules — {} configured\n", schedules.len());
    let w = 30usize;
    println!("  {:<20}  {:<w$}  {:<14}  Next run (UTC)", "Name", "Goal", "Cron");
    println!("  {}", "─".repeat(20 + 2 + w + 2 + 14 + 2 + 26));
    for s in &schedules {
        let goal = if s.goal.len() > w {
            format!("{}…", &s.goal[..w - 1])
        } else {
            s.goal.clone()
        };
        let next = next_run_times(&s.cron, 1)
            .into_iter()
            .next()
            .map_or_else(|| "invalid cron".to_string(), |t| t.format("%Y-%m-%d %H:%M UTC").to_string());
        println!("  {:<20}  {:<w$}  {:<14}  {}", s.name, goal, s.cron, next);
    }
    Ok(())
}
