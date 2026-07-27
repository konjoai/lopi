#!/usr/bin/env python3
"""Pure-python paired Wilcoxon signed-rank test (no scipy available in this
sandbox). Normal approximation with continuity correction, standard for
n>=10; for smaller n this also prints the exact sign pattern so the reader
can judge significance without trusting the approximation alone."""
import json
import math
import sys


def wilcoxon(diffs):
    diffs = [d for d in diffs if d != 0]
    n = len(diffs)
    if n == 0:
        return {"n": 0, "W": None, "z": None, "p": None}
    ranks = sorted(range(n), key=lambda i: abs(diffs[i]))
    rank_of = [0] * n
    i = 0
    r = 1
    # average ranks for ties
    abs_sorted = sorted(abs(d) for d in diffs)
    while i < n:
        j = i
        while j < n and abs_sorted[j] == abs_sorted[i]:
            j += 1
        avg_rank = (r + (r + (j - i) - 1)) / 2
        for k in range(i, j):
            idx = ranks[k]
            rank_of[idx] = avg_rank
        r += j - i
        i = j
    w_pos = sum(rank_of[i] for i in range(n) if diffs[i] > 0)
    w_neg = sum(rank_of[i] for i in range(n) if diffs[i] < 0)
    W = min(w_pos, w_neg)
    mean_w = n * (n + 1) / 4
    sd_w = math.sqrt(n * (n + 1) * (2 * n + 1) / 24)
    if sd_w == 0:
        z = 0.0
    else:
        z = (W - mean_w + 0.5) / sd_w  # continuity correction
    # two-sided p from standard normal
    p = 2 * (1 - 0.5 * (1 + math.erf(abs(z) / math.sqrt(2))))
    return {
        "n": n,
        "W_pos": w_pos,
        "W_neg": w_neg,
        "W": W,
        "z": z,
        "p": p,
    }


def effect_size_r(z, n):
    if n == 0:
        return None
    return abs(z) / math.sqrt(n)


def summarize(field_a, field_b, rows, label):
    diffs = [row[field_b] - row[field_a] for row in rows]
    res = wilcoxon(diffs)
    r = effect_size_r(res["z"], res["n"]) if res["z"] is not None else None
    print(f"--- {label} ---")
    print(f"  n pairs (nonzero diff): {res['n']}")
    print(f"  median {field_a}: {sorted(row[field_a] for row in rows)[len(rows)//2]:.6f}")
    print(f"  median {field_b}: {sorted(row[field_b] for row in rows)[len(rows)//2]:.6f}")
    all_negative = all(d < 0 for d in diffs) if diffs else False
    all_positive = all(d > 0 for d in diffs) if diffs else False
    print(f"  W_pos={res['W_pos']}, W_neg={res['W_neg']}, W={res['W']}")
    print(f"  z={res['z']:.4f}" if res["z"] is not None else "  z=n/a")
    print(f"  p={res['p']:.6g}" if res["p"] is not None else "  p=n/a")
    print(f"  effect size r={r:.4f}" if r is not None else "  r=n/a")
    print(f"  all pairs same direction: {'yes (all lower)' if all_negative else ('yes (all higher)' if all_positive else 'no')}")
    return res, r


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "results.jsonl"
    rows = [json.loads(l) for l in open(path) if l.strip()]
    print(f"Loaded {len(rows)} pairs from {path}\n")
    summarize("cold_cost", "resumed_cost", rows, "cost_usd (resumed - cold)")
    print()
    cache_rows = []
    for row in rows:
        cold_total = row["cold_cache_read"] + row["cold_cache_create"]
        res_total = row["resumed_cache_read"] + row["resumed_cache_create"]
        cache_rows.append({
            "cold_ratio": row["cold_cache_read"] / cold_total if cold_total else 0,
            "resumed_ratio": row["resumed_cache_read"] / res_total if res_total else 0,
        })
    summarize("cold_ratio", "resumed_ratio", cache_rows, "cache_read/(cache_read+cache_create) ratio")
    print()
    pass_ok = sum(1 for row in rows if row.get("cold_pass_rate_ok") and row.get("resumed_pass_rate_ok"))
    print(f"Both conditions completed without CLI error in {pass_ok}/{len(rows)} pairs")


if __name__ == "__main__":
    main()
