#!/usr/bin/env python3
"""Paired Wilcoxon signed-rank test + matched-pairs rank-biserial effect size.

No scipy in this environment, so this implements the normal-approximation
Wilcoxon signed-rank test by hand (standard textbook formula, continuity
corrected), which is the accepted approach at n=30 paired samples. Reads two
equal-length JSON arrays of numbers (pre-fix, post-fix) from files given as
argv, one metric at a time.

Usage: python3 wilcoxon.py pre.json post.json ["lower_is_better"|"higher_is_better"]
"""
import sys
import json
import math


def wilcoxon_signed_rank(pre, post, better="lower_is_better"):
    diffs = [b - a for a, b in zip(pre, post)]
    # Drop exact zero differences per the standard Wilcoxon procedure.
    nonzero = [d for d in diffs if d != 0]
    n = len(nonzero)
    if n == 0:
        return {"n": 0, "W": 0, "z": 0.0, "p_two_sided": 1.0, "effect_size_r": 0.0}

    abs_diffs = sorted(range(len(nonzero)), key=lambda i: abs(nonzero[i]))
    ranks = [0.0] * len(nonzero)
    i = 0
    sorted_abs = sorted(abs(d) for d in nonzero)
    # Assign average ranks for ties.
    idx_sorted = sorted(range(len(nonzero)), key=lambda k: abs(nonzero[k]))
    pos = 0
    while pos < len(idx_sorted):
        j = pos
        while j + 1 < len(idx_sorted) and abs(nonzero[idx_sorted[j + 1]]) == abs(nonzero[idx_sorted[pos]]):
            j += 1
        avg_rank = (pos + 1 + j + 1) / 2.0
        for k in range(pos, j + 1):
            ranks[idx_sorted[k]] = avg_rank
        pos = j + 1

    w_pos = sum(r for r, d in zip(ranks, nonzero) if d > 0)
    w_neg = sum(r for r, d in zip(ranks, nonzero) if d < 0)
    w = min(w_pos, w_neg)

    mean_w = n * (n + 1) / 4.0
    # Tie correction for variance.
    from collections import Counter
    tie_counts = Counter(abs(d) for d in nonzero)
    tie_correction = sum(t ** 3 - t for t in tie_counts.values()) / 48.0
    var_w = n * (n + 1) * (2 * n + 1) / 24.0 - tie_correction
    sd_w = math.sqrt(max(var_w, 0.0))

    if sd_w == 0:
        z = 0.0
    else:
        # Continuity correction.
        if w_pos > w_neg:
            z = (w - mean_w + 0.5) / sd_w
        else:
            z = (w - mean_w - 0.5) / sd_w

    # Two-sided p-value from standard normal.
    p = 2 * (1 - _std_normal_cdf(abs(z)))
    p = min(p, 1.0)

    # Matched-pairs rank-biserial correlation as effect size:
    # r = (W_pos - W_neg) / (W_pos + W_neg)
    total = w_pos + w_neg
    r = (w_pos - w_neg) / total if total > 0 else 0.0
    # Orient r so positive means "post is better" given the metric direction.
    if better == "lower_is_better":
        r = -r

    return {
        "n": n,
        "n_ties_dropped": len(diffs) - n,
        "W_pos": w_pos,
        "W_neg": w_neg,
        "W": w,
        "z": round(z, 4),
        "p_two_sided": round(p, 6),
        "effect_size_r": round(r, 4),
        "median_pre": _median(pre),
        "median_post": _median(post),
        "median_diff": _median(diffs),
    }


def _median(xs):
    s = sorted(xs)
    n = len(s)
    if n == 0:
        return 0.0
    mid = n // 2
    if n % 2 == 1:
        return s[mid]
    return (s[mid - 1] + s[mid]) / 2.0


def _std_normal_cdf(x):
    return 0.5 * (1 + math.erf(x / math.sqrt(2)))


if __name__ == "__main__":
    pre_path, post_path = sys.argv[1], sys.argv[2]
    direction = sys.argv[3] if len(sys.argv) > 3 else "lower_is_better"
    with open(pre_path) as f:
        pre = json.load(f)
    with open(post_path) as f:
        post = json.load(f)
    assert len(pre) == len(post), "paired samples must be equal length"
    result = wilcoxon_signed_rank(pre, post, direction)
    print(json.dumps(result, indent=2))
