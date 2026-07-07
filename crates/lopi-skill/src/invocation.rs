//! Skill-invocation prefix parsing — `:<skill-name> <args>` (Skill Arguments,
//! Sprint 2). This module only does the pure string split; looking the named
//! skill up in a [`crate::SkillRegistry`] and rendering it is the caller's
//! concern at the goal-ingestion boundary.

/// Parse a `:<skill-name> <rest>` invocation prefix out of `input`.
///
/// Returns `Some((name, args))` when `input` starts with `:` followed by a
/// non-empty skill name; `args` is the remainder, trimmed, or `""` when none
/// was given. Returns `None` for anything that isn't a colon-prefixed
/// invocation — an ordinary goal string is not this function's concern, and
/// the caller passes it through unchanged.
#[must_use]
pub fn parse_invocation(input: &str) -> Option<(&str, &str)> {
    let rest = input.strip_prefix(':')?;
    let (name, args) = rest.split_once(' ').unwrap_or((rest, ""));
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some((name, args.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_and_args() {
        assert_eq!(parse_invocation(":kcqf vectro"), Some(("kcqf", "vectro")));
    }

    #[test]
    fn plain_goal_is_not_an_invocation() {
        assert_eq!(parse_invocation("no colon here"), None);
    }

    #[test]
    fn name_with_no_args_yields_empty_args() {
        assert_eq!(parse_invocation(":kcqf"), Some(("kcqf", "")));
    }

    #[test]
    fn bare_colon_is_not_an_invocation() {
        assert_eq!(parse_invocation(":"), None);
    }

    #[test]
    fn colon_space_with_empty_name_is_not_an_invocation() {
        assert_eq!(parse_invocation(": vectro"), None);
    }
}
