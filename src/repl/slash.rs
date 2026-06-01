//! Slash-command definitions, parser, and autocomplete for the lopi REPL.

/// One entry in the slash-command registry.
pub struct SlashDef {
    pub name: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
}

/// All commands available in the REPL.
pub const SLASH_COMMANDS: &[SlashDef] = &[
    SlashDef {
        name: "help",
        usage: "/help",
        description: "Show this command reference",
    },
    SlashDef {
        name: "run",
        usage: "/run <goal>",
        description: "Run an agent task with the given goal",
    },
    SlashDef {
        name: "bypass",
        usage: "/bypass <goal>",
        description: "Run with directory restrictions disabled",
    },
    SlashDef {
        name: "watch",
        usage: "/watch",
        description: "Switch to the live agent dashboard",
    },
    SlashDef {
        name: "dock",
        usage: "/dock",
        description: "List recent tasks and their status",
    },
    SlashDef {
        name: "cancel",
        usage: "/cancel <id>",
        description: "Cancel a running task by ID prefix",
    },
    SlashDef {
        name: "cost",
        usage: "/cost",
        description: "Show accumulated session token cost",
    },
    SlashDef {
        name: "model",
        usage: "/model [name]",
        description: "Show or set the Claude model",
    },
    SlashDef {
        name: "clear",
        usage: "/clear",
        description: "Clear the output history",
    },
    SlashDef {
        name: "quit",
        usage: "/quit",
        description: "Exit lopi",
    },
];

/// A parsed slash command ready for dispatch.
#[derive(Debug, Clone)]
pub enum SlashCmd {
    Help,
    Run { goal: String },
    Bypass { goal: String },
    Watch,
    Dock,
    Cancel { id: String },
    Cost,
    Model { name: Option<String> },
    Clear,
    Quit,
}

/// Parse a raw slash-command string (including the leading `/`).
///
/// Returns `Err` with a short usage hint for unknown or malformed commands.
pub fn parse_slash(input: &str) -> Result<SlashCmd, String> {
    let input = input.trim();
    let without_slash = input.strip_prefix('/').unwrap_or(input);
    let (name, rest) = without_slash
        .split_once(char::is_whitespace)
        .unwrap_or((without_slash, ""));
    let arg = rest.trim().to_string();

    match name {
        "help" => Ok(SlashCmd::Help),
        "run" => {
            if arg.is_empty() {
                Err("Usage: /run <goal>".into())
            } else {
                Ok(SlashCmd::Run { goal: arg })
            }
        }
        "bypass" => {
            if arg.is_empty() {
                Err("Usage: /bypass <goal>".into())
            } else {
                Ok(SlashCmd::Bypass { goal: arg })
            }
        }
        "watch" => Ok(SlashCmd::Watch),
        "dock" => Ok(SlashCmd::Dock),
        "cancel" => {
            if arg.is_empty() {
                Err("Usage: /cancel <task-id>".into())
            } else {
                Ok(SlashCmd::Cancel { id: arg })
            }
        }
        "cost" => Ok(SlashCmd::Cost),
        "model" => Ok(SlashCmd::Model {
            name: if arg.is_empty() { None } else { Some(arg) },
        }),
        "clear" => Ok(SlashCmd::Clear),
        "quit" | "q" | "exit" => Ok(SlashCmd::Quit),
        other => Err(format!("Unknown command: /{other}  Type /help for a list.")),
    }
}

/// Return all commands whose name starts with `prefix` (without the `/`).
pub fn autocomplete(prefix: &str) -> Vec<&'static SlashDef> {
    SLASH_COMMANDS
        .iter()
        .filter(|d| d.name.starts_with(prefix))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_run_with_goal() {
        let cmd = parse_slash("/run fix the login bug").unwrap();
        assert!(matches!(cmd, SlashCmd::Run { goal } if goal == "fix the login bug"));
    }

    #[test]
    fn parse_run_no_goal_is_err() {
        assert!(parse_slash("/run").is_err());
    }

    #[test]
    fn parse_quit_aliases() {
        assert!(matches!(parse_slash("/quit"), Ok(SlashCmd::Quit)));
        assert!(matches!(parse_slash("/exit"), Ok(SlashCmd::Quit)));
        assert!(matches!(parse_slash("/q"), Ok(SlashCmd::Quit)));
    }

    #[test]
    fn autocomplete_partial_match() {
        let results = autocomplete("ca");
        assert!(results.iter().any(|d| d.name == "cancel"));
    }

    #[test]
    fn autocomplete_empty_returns_all() {
        assert_eq!(autocomplete("").len(), SLASH_COMMANDS.len());
    }
}
