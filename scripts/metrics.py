#!/usr/bin/env python3
"""Shared ranking metrics and HTTP helpers used by all dataset scripts."""

import json
import math
import sys
import urllib.error
import urllib.request
from typing import List


def dcg(grades: List[int], k: int) -> float:
    return sum(g / math.log2(i + 2) for i, g in enumerate(grades[:k]))


def ndcg(grades: List[int], k: int) -> float:
    ideal = sorted(grades, reverse=True)
    d = dcg(ideal, k)
    return dcg(grades, k) / d if d > 0 else 0.0


def mrr(grades: List[int]) -> float:
    for i, g in enumerate(grades):
        if g > 0:
            return 1.0 / (i + 1)
    return 0.0


def precision_at(grades: List[int], k: int) -> float:
    return sum(1 for g in grades[:k] if g > 0) / k


def batch_load(server: str, index: str, products: List[dict], batch_size: int = 200) -> None:
    url = f"{server}/1/indexes/{index}/batch"
    total = len(products)
    for i in range(0, total, batch_size):
        chunk = products[i:i + batch_size]
        requests = [{"action": "addObject", "body": p} for p in chunk]
        body = json.dumps({"requests": requests}).encode()
        req = urllib.request.Request(
            url, data=body,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=30):
                pass
        except urllib.error.URLError as e:
            print(f"  ERROR at batch {i}: {e}", file=sys.stderr)
            sys.exit(1)
        print(f"  loaded {min(i + batch_size, total)}/{total}", end="\r", flush=True)
    print()
