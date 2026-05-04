CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    goal TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    source TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS attempts (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    attempt_num INTEGER NOT NULL,
    branch TEXT NOT NULL,
    score_test_pass_rate REAL,
    score_lint_errors INTEGER,
    score_diff_lines INTEGER,
    outcome TEXT NOT NULL,
    errors TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS patterns (
    id TEXT PRIMARY KEY,
    goal_keywords TEXT NOT NULL,
    successful_constraints TEXT,
    avg_attempts REAL,
    success_rate REAL,
    last_seen TEXT NOT NULL
);
