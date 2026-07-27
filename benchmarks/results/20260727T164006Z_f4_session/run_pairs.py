#!/usr/bin/env python3
"""Sprint F4 Phase 5 — small real paired sample of cold vs resumed
plan->implement cost, using the real `claude` CLI (subscription auth, no
API key). NOT the full 30-run T01-T10 corpus the brief's own Phase 5 asks
for — see NEXT_SESSION_PROMPT.md/CHANGELOG.md for why that corpus run is
still outstanding. This is a smaller, real (not synthetic) mechanism-level
measurement of the same lever: does resuming actually move cost/cache share
in the plan->implement transition."""
import json
import os
import subprocess
import sys
import time
import uuid

REPO = os.path.join(os.path.dirname(__file__), "wt_bench")
DENIED = ["Write", "Edit", "MultiEdit", "NotebookEdit", "Bash", "Task"]

PLAN_PROMPT = (
    "Read notes.md and src_lib.rs in this repo. Identify the bug in "
    "safe_div and outline a one-paragraph plan to fix it. Do not edit "
    "any files — describe the plan only."
)
IMPLEMENT_PROMPT_COLD = (
    "Following this plan, describe the exact code change to fix "
    "safe_div, as a short diff-like text block. Do not edit any files.\n\n"
    "## Plan\n{plan}"
)
IMPLEMENT_PROMPT_RESUMED = (
    "Following the plan you just wrote, describe the exact code change "
    "to fix safe_div, as a short diff-like text block. Do not edit any "
    "files. You already read the files in this session — do not read "
    "them again."
)


def clean_env():
    env = os.environ.copy()
    for k in [
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_REMOTE_SESSION_ID",
        "ANTHROPIC_API_KEY",
    ]:
        env.pop(k, None)
    return env


def call(prompt, session_id=None, resume=None, model=None):
    args = [
        "claude",
        "-p",
        prompt,
        "--output-format",
        "json",
        "--permission-mode",
        "acceptEdits",
        "--disallowedTools",
        *DENIED,
    ]
    if session_id:
        args += ["--session-id", session_id]
    if resume:
        args += ["--resume", resume]
    if model:
        args += ["--model", model]
    out = subprocess.run(
        args, cwd=REPO, env=clean_env(), capture_output=True, text=True, timeout=180
    )
    try:
        data = json.loads(out.stdout)
    except json.JSONDecodeError:
        data = {"is_error": True, "raw": out.stdout, "stderr": out.stderr}
    return data


def usage_of(data):
    u = data.get("usage") or {}
    mu = data.get("modelUsage") or {}
    if mu:
        inp = sum(v.get("inputTokens", 0) for v in mu.values())
        cread = sum(v.get("cacheReadInputTokens", 0) for v in mu.values())
        ccreate = sum(v.get("cacheCreationInputTokens", 0) for v in mu.values())
    else:
        inp = u.get("input_tokens", 0)
        cread = u.get("cache_read_input_tokens", 0)
        ccreate = u.get("cache_creation_input_tokens", 0)
    cost = data.get("total_cost_usd", data.get("cost_usd", 0.0)) or 0.0
    return {"input": inp, "cache_read": cread, "cache_create": ccreate, "cost": cost}


def run_pair(i):
    result = {"pair": i}

    # --- cold condition: two independent, unresumed spawns ---
    t0 = time.time()
    plan_cold = call(PLAN_PROMPT)
    plan_text = plan_cold.get("result", "") or ""
    impl_cold = call(IMPLEMENT_PROMPT_COLD.format(plan=plan_text[:800]))
    cold_a = usage_of(plan_cold)
    cold_b = usage_of(impl_cold)
    result["cold_cost"] = round(cold_a["cost"] + cold_b["cost"], 6)
    result["cold_cache_read"] = cold_a["cache_read"] + cold_b["cache_read"]
    result["cold_cache_create"] = cold_a["cache_create"] + cold_b["cache_create"]
    result["cold_seconds"] = round(time.time() - t0, 1)

    # --- resumed condition: New session, then Resume it ---
    t1 = time.time()
    sid = str(uuid.uuid4())
    plan_new = call(PLAN_PROMPT, session_id=sid)
    impl_resumed = call(IMPLEMENT_PROMPT_RESUMED, resume=sid)
    res_a = usage_of(plan_new)
    res_b = usage_of(impl_resumed)
    result["resumed_cost"] = round(res_a["cost"] + res_b["cost"], 6)
    result["resumed_cache_read"] = res_a["cache_read"] + res_b["cache_read"]
    result["resumed_cache_create"] = res_a["cache_create"] + res_b["cache_create"]
    result["resumed_seconds"] = round(time.time() - t1, 1)
    result["resumed_pass_rate_ok"] = not impl_resumed.get("is_error", False)
    result["cold_pass_rate_ok"] = not impl_cold.get("is_error", False)

    return result


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 8
    results = []
    for i in range(n):
        r = run_pair(i)
        results.append(r)
        print(json.dumps(r), flush=True)
    with open(os.path.join(os.path.dirname(__file__), "results.jsonl"), "w") as f:
        for r in results:
            f.write(json.dumps(r) + "\n")


if __name__ == "__main__":
    main()
