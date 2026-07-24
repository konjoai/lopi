//! lopi — high-performance Rust orchestrator for concurrent Claude Code agents.
#![allow(clippy::print_stdout, clippy::print_stderr)]
mod cli;
mod diag_commands;
mod gap_fill_commands;
mod learn_commands;
mod loop_commands;
mod mcp_commands;
mod onboarding_import_commands;
mod remote;
mod repl;
mod replay_commands;
mod repo_detect;
mod run_command;
mod sail_commands;
mod schedule_commands;
mod skill_commands;
mod spec_commands;
mod stability_commands;
mod task_commands;
mod toolchain_detect;
mod trust_commands;
mod util;
mod webhook_commands;
mod worktree_commands;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use anyhow::Result;
use clap::Parser;
pub(crate) use cli::{Cli, Commands, LearnCmd, LoopCmd, ScheduleCmd, StabilityCmd};
use skill_commands::SkillCmd;
use tracing_subscriber::prelude::*;
use util::load_config;
use worktree_commands::WorktreeCmd;

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<()> {
    // Parsed before the tracing subscriber is set up so `mcp-serve`'s writer
    // choice below can depend on it — `mcp-serve` speaks JSON-RPC framed
    // over stdout/stdin, so any log line landing on stdout (the `fmt` layer's
    // default writer) corrupts the frame the MCP Apps host is trying to
    // parse. Every other command keeps stdout, matching prior behavior.
    let cli = Cli::parse();
    let is_mcp_serve = matches!(cli.command, Some(Commands::McpServe { .. }));

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let writer = if is_mcp_serve {
        tracing_subscriber::fmt::writer::BoxMakeWriter::new(std::io::stderr)
    } else {
        tracing_subscriber::fmt::writer::BoxMakeWriter::new(std::io::stdout)
    };
    let fmt_layer = tracing_subscriber::fmt::layer().with_writer(writer);

    #[cfg(feature = "otel")]
    if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
        let service_name =
            std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "lopi".to_string());
        let otlp_exporter = opentelemetry_otlp::new_exporter().tonic();
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(otlp_exporter)
            .with_trace_config(opentelemetry_sdk::trace::config().with_resource(
                opentelemetry_sdk::Resource::new(vec![opentelemetry::KeyValue::new(
                    "service.name",
                    service_name,
                )]),
            ))
            .install_batch(opentelemetry_sdk::runtime::Tokio)
            .map_err(|e| anyhow::anyhow!("failed to install OTel tracer: {e}"))?;
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(otel_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    }

    #[cfg(not(feature = "otel"))]
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();

    let cfg = load_config(cli.config.as_ref());

    match cli.command {
        // ── bare `lopi` → interactive REPL ─────────────────────
        None => {
            let repo = repo_detect::detect_repo();
            let model = lopi_agent::MODEL_SONNET.to_string();
            repl::run_repl(repo, model, cfg).await?;
        }

        // ── lopi bypass <goal> ──────────────────────────────────
        Some(Commands::Bypass { goal_args }) => {
            let goal = goal_args.join(" ");
            let repo = repo_detect::detect_repo();
            repl::run_inline(goal, repo, true, cfg.as_ref()).await?;
        }

        // ── lopi run ────────────────────────────────────────────
        Some(Commands::Run {
            goal,
            repo,
            dry_run,
            speculative,
            adaptive_retry,
            stability_gate,
            budget,
            budget_preset,
            budget_tokens,
        }) => {
            run_command::run(
                goal,
                repo,
                dry_run,
                speculative,
                adaptive_retry,
                stability_gate,
                cfg.as_ref(),
                run_command::BudgetArgs {
                    budget,
                    budget_preset,
                    budget_tokens,
                },
            )
            .await?;
        }

        // ── lopi watch / tail / dock / sail / cancel / resume ───
        Some(Commands::Watch { remote, local }) => task_commands::watch(remote, local).await?,
        Some(Commands::Tail { task_id, history }) => task_commands::tail(task_id, history).await?,
        Some(Commands::Dock) => task_commands::dock().await?,
        Some(Commands::Sail {
            port,
            host,
            max_agents,
            repo,
            repos,
        }) => {
            sail_commands::run(max_agents, repo, repos, host, port, cfg.as_ref()).await?;
        }
        Some(Commands::Cancel { task_id }) => task_commands::cancel(task_id).await?,
        Some(Commands::Resume { agent_id }) => task_commands::resume(agent_id).await?,

        // ── lopi watch-gap-fill ─────────────────────────────────
        Some(Commands::WatchGapFill {
            repo,
            interval,
            sail_url,
            run_now,
        }) => {
            gap_fill_commands::watch_loop(repo, interval, &sail_url, run_now).await?;
        }

        Some(Commands::Trust) => trust_commands::show().await?,
        Some(Commands::Replay {
            task,
            from,
            dry_run,
        }) => replay_commands::run(task, from, dry_run).await?,

        Some(Commands::ServeApp { port, host }) => {
            let addr: std::net::SocketAddr = format!("{host}:{port}")
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid address: {e}"))?;
            let app_cfg = lopi_app::AppConfig::from_env();
            println!("🔐 lopi serve-app on {addr}");
            println!(
                "   GitHub OAuth: {}",
                if app_cfg.github_configured() {
                    "✅ configured"
                } else {
                    "⚠️  missing (GITHUB_CLIENT_ID, GITHUB_CLIENT_SECRET, GITHUB_REDIRECT_URI)"
                }
            );
            println!(
                "   Stripe:       {}",
                if app_cfg.stripe_configured() {
                    "✅ configured"
                } else {
                    "⚠️  missing (STRIPE_WEBHOOK_SECRET)"
                }
            );
            println!();
            let store = lopi_memory::MemoryStore::open(util::db_path()).await?;
            let state = lopi_app::AppState {
                cfg: app_cfg,
                store,
            };
            lopi_app::serve(state, addr).await?;
        }

        Some(Commands::GapFill {
            repo,
            sail_url,
            dry_run,
        }) => {
            gap_fill_commands::run(repo, &sail_url, dry_run, false).await?;
        }

        Some(Commands::Spec { repo, export, save }) => {
            spec_commands::run_spec(repo, export, save).await?;
        }
        Some(Commands::Check {
            repo,
            fail_on_violations,
        }) => {
            spec_commands::run_check(repo, fail_on_violations).await?;
        }

        Some(Commands::Learn(cmd)) => learn_commands::run(cmd, util::db_path()).await?,

        // ── lopi import ──────────────────────────────────────────
        Some(Commands::Import {
            dry_run,
            claude_dir,
        }) => {
            onboarding_import_commands::run(dry_run, claude_dir, util::db_path()).await?;
        }

        Some(Commands::ServeWebhooks {
            port,
            host,
            webhook_secret,
            github_token,
            anthropic_key,
        }) => {
            webhook_commands::run(port, host, webhook_secret, github_token, anthropic_key).await?;
        }

        Some(Commands::Schedules(ScheduleCmd::List)) => {
            let schedules = cfg
                .as_ref()
                .map(|c| c.schedules.clone())
                .unwrap_or_default();
            schedule_commands::list(schedules).await?;
        }

        Some(Commands::Loop(LoopCmd::Validate { repo })) => {
            let issues = loop_commands::check(&repo)?;
            if issues.is_empty() {
                println!("✓ loop config valid ({})", lopi_core::LoopConfig::REL_PATH);
            } else {
                eprintln!("✗ loop config has {} issue(s):", issues.len());
                for issue in &issues {
                    eprintln!("  • {issue}");
                }
                std::process::exit(1);
            }
        }
        Some(Commands::Loop(LoopCmd::Show { repo })) => {
            print!("{}", loop_commands::render(&repo)?);
        }

        Some(Commands::Worktree(WorktreeCmd::List { repo })) => {
            print!("{}", worktree_commands::list(&repo).await?);
        }
        Some(Commands::Worktree(WorktreeCmd::Gc { repo })) => {
            print!("{}", worktree_commands::gc(&repo).await?);
        }
        Some(Commands::Skill(SkillCmd::Promote { repo, min, limit })) => {
            print!(
                "{}",
                skill_commands::promote(&repo, util::db_path(), min, limit).await?
            );
        }

        // ── lopi stability ──────────────────────────────────────
        Some(Commands::Stability(StabilityCmd::List {
            limit,
            unstable_only,
        })) => stability_commands::list(limit, unstable_only).await?,

        Some(Commands::Stability(StabilityCmd::Summary)) => stability_commands::summary().await?,

        Some(Commands::McpServe { repo, max_agents }) => {
            mcp_commands::serve(repo, max_agents, cfg.as_ref()).await?;
        }

        Some(Commands::Diag {
            out,
            task_limit,
            log_limit,
            audit_limit,
        }) => {
            diag_commands::export(
                out,
                diag_commands::DiagLimits {
                    tasks: task_limit,
                    logs: log_limit,
                    audit: audit_limit,
                },
            )
            .await?;
        }
    }

    Ok(())
}
