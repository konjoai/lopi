//! lopi-side prompt templates — literal strings with named `{hole}` markers
//! that lopi resolves against a variable map **at enqueue time**. Claude never
//! sees an unresolved template; it only ever receives the final literal
//! string. This is distinct from a skill: a skill injects markdown context by
//! trigger match, whereas a template is a caller-supplied string filled in
//! before a [`crate::Task`] is even constructed.
//!
//! Escaping follows the same rule as Rust's `format!` macro: `{{` and `}}`
//! produce a literal `{` / `}`, so a template that must contain a literal
//! brace can still be written unambiguously.

use std::collections::BTreeMap;
use thiserror::Error;

/// Errors returned by [`resolve`] when a template cannot be fully resolved.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TemplateError {
    /// A `{name}` hole in the template had no matching entry in `vars`.
    ///
    /// Resolution fails loudly rather than passing the hole through
    /// unresolved — an unfilled hole reaching Claude verbatim would silently
    /// change the meaning of the prompt.
    #[error("template variable `{{{name}}}` is not defined (template: {template:?})")]
    UnresolvedVariable {
        /// The unresolved hole's name.
        name: String,
        /// The original template string, for diagnostic context.
        template: String,
    },
}

/// Resolve `template`'s `{name}` holes against `vars`, returning the literal
/// result string.
///
/// A single left-to-right scan (no regex, no dependency beyond the standard
/// library): `{name}` is replaced with `vars[name]`, `{{` / `}}` produce a
/// literal `{` / `}`, and any other character is copied through unchanged.
/// Pure — no I/O; equal inputs always produce equal outputs.
///
/// # Errors
/// Returns [`TemplateError::UnresolvedVariable`] when `template` contains a
/// `{name}` hole absent from `vars`. Extra entries in `vars` that no hole
/// references are ignored, not an error.
pub fn resolve(template: &str, vars: &BTreeMap<String, String>) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let name: String = chars.by_ref().take_while(|&nc| nc != '}').collect();
                match vars.get(&name) {
                    Some(value) => out.push_str(value),
                    None => {
                        return Err(TemplateError::UnresolvedVariable {
                            name,
                            template: template.to_string(),
                        })
                    }
                }
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn resolves_named_holes() {
        let v = vars(&[("repo", "vectro"), ("cmd", "cargo test")]);
        assert_eq!(
            resolve("test {repo} until {cmd}", &v).unwrap(),
            "test vectro until cargo test"
        );
    }

    #[test]
    fn unfilled_hole_errors_naming_the_missing_variable() {
        let v = vars(&[]);
        let err = resolve("test {missing}", &v).unwrap_err();
        let TemplateError::UnresolvedVariable { name, .. } = err;
        assert_eq!(name, "missing");
    }

    #[test]
    fn no_holes_returns_input_unchanged() {
        let v = vars(&[]);
        assert_eq!(resolve("no holes here", &v).unwrap(), "no holes here");
    }

    #[test]
    fn unused_extra_var_is_not_an_error() {
        let v = vars(&[("repo", "vectro"), ("unused", "ignored")]);
        assert_eq!(resolve("build {repo}", &v).unwrap(), "build vectro");
    }

    #[test]
    fn escaped_braces_produce_literal_braces() {
        let v = vars(&[]);
        assert_eq!(
            resolve("literal {{brace}} test", &v).unwrap(),
            "literal {brace} test"
        );
    }

    #[test]
    fn resolve_is_pure() {
        let v = vars(&[("repo", "vectro")]);
        let a = resolve("build {repo}", &v).unwrap();
        let b = resolve("build {repo}", &v).unwrap();
        assert_eq!(a, b);
    }
}
