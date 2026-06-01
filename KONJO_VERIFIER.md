# Konjo Verifier

> *The only agent orchestrator that grades its own work before opening a PR.*

## What it is

The Konjo Verifier is a second-score pass that runs after an agent's tests pass. It sends the plan,
diff, and test output to Opus with a developer-supplied rubric, and receives a structured verdict
before the commit is made.

A passing heuristic score (tests green, lint clean) is necessary but not sufficient. The Verifier
asks the higher-order question: **does this diff actually accomplish what was asked, and does it meet
the team's quality criteria?**

## How it works

```
Plan → Implement → Test → [Score: heuristic pass?] → [Verifier: rubric pass?] → Commit → PR
                                                            ↓ fail
                                                   append fix_hints to constraints
                                                            ↓
                                                         Retry
```

The verifier calls Opus with:

- **Goal** — the original task description
- **Plan excerpt** — first 1500 characters of the agent's plan
- **Diff excerpt** — first 6000 characters of the git diff
- **Test output** — heuristic scorer errors/output
- **Rubric** — ordered list of criteria to check

Opus returns a structured JSON verdict:

```json
{
  "passed": false,
  "gaps": ["No test covers the new error branch"],
  "fix_hints": ["Add a test asserting Err(TokenExpired) is returned when the token is stale"],
  "confidence": 0.87
}
```

On failure, `fix_hints` are injected into `Task::constraints` and the agent retries with them as
hard requirements in the next planning prompt.

## Enabling the Verifier

```rust
let runner = AgentRunner::new(task, repo_path, bus, store, cancel_rx, counter)
    .with_api(client.clone(), limiter.clone(), breaker.clone())
    .with_verifier();  // Sprint S — enable rubric-guided second-score pass
```

## Rubrics

Three canonical rubrics ship with lopi at `.lopi/rubrics/`:

| File | When to use |
|------|-------------|
| `feature_completeness.toml` | New feature implementation tasks (default fallback) |
| `refactor_safety.toml` | Refactoring tasks where no public API should change |
| `security_audit.toml` | Security hardening tasks and webhook/auth changes |

### Rubric format

```toml
name = "my_rubric"

criteria = [
  "All existing tests still pass",
  "The stated goal is fully implemented",
  "No debugging artefacts remain in the diff",
]
```

### Task-level override

Attach a rubric directly to a task to override the default:

```rust
let mut task = Task::new("harden the webhook handler");
task.rubric = Some(Rubric {
    name: "security_audit".into(),
    criteria: vec![
        "HMAC signature is verified before processing".into(),
        "No stack traces leaked in error responses".into(),
    ],
});
```

## Verdict persistence

Every verdict is written to the `verifier_verdicts` SQLite table:

```
id, task_id, attempt, passed, gaps_json, fix_hints_json, confidence, model_used, ts
```

Queryable via `MemoryStore::load_verifier_verdicts(task_id)`.

## Cost

One Opus call per successful heuristic-score pass, typically $0.01–$0.05 per attempt depending on
diff size. The call is skipped when:

- `verifier_enabled = false` (opt-in)
- No `AnthropicClient` is configured
- The heuristic score failed (verifier only runs after tests pass)

## The Konjo brand position

Competitors produce code that passes tests. lopi produces code that passes tests *and* satisfies
your explicit quality criteria, with a receipts trail in SQLite. That's the difference between
"automated" and "provably correct by your standards."
