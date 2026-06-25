#!/usr/bin/env python3
"""
Search quality evaluator for vectoria-algolia.

Fires a curated query set against the running server, grades each hit with a
relevance function, then computes NDCG@10, MRR, and Precision@5 per query
and as a macro-average.

Usage:
    python3 scripts/quality_eval.py [--server http://localhost:8108] [--index products] [--top-k 10]
"""

import argparse
import json
import sys
import urllib.request
import urllib.error
from typing import Callable, List, Optional
from metrics import dcg, ndcg, mrr, precision_at

# ── Relevance functions ────────────────────────────────────────────────────────
# Each returns 0 (irrelevant), 1 (partial), 2 (relevant), or 3 (perfect).

def _cat(cat: str) -> Callable:
    """Hit is relevant when its category matches."""
    def grade(h: dict) -> int:
        return 2 if h.get("category", "").lower() == cat.lower() else 0
    return grade

def _cat_kw(cat: str, *keywords: str) -> Callable:
    """Relevant = category match; perfect = category + keyword in title/description."""
    kws = [k.lower() for k in keywords]
    def grade(h: dict) -> int:
        text = (h.get("title", "") + " " + h.get("description", "")).lower()
        if h.get("category", "").lower() != cat.lower():
            return 0
        return 3 if any(k in text for k in kws) else 1
    return grade

def _brand_cat(brand: str, cat: str) -> Callable:
    def grade(h: dict) -> int:
        b_match = h.get("brand", "").lower() == brand.lower()
        c_match = h.get("category", "").lower() == cat.lower()
        if b_match and c_match: return 3
        if b_match: return 2
        if c_match: return 1
        return 0
    return grade

def _kw(*keywords: str, cats: Optional[List[str]] = None) -> Callable:
    """Any hit whose title/description contains a keyword is relevant."""
    kws = [k.lower() for k in keywords]
    def grade(h: dict) -> int:
        text = (h.get("title", "") + " " + h.get("description", "")).lower()
        cat_ok = (cats is None) or (h.get("category", "") in cats)
        if any(k in text for k in kws) and cat_ok:
            return 3
        if cat_ok and cats is not None:
            return 1
        return 0
    return grade

def _price_cat(cat: str, max_price: float) -> Callable:
    def grade(h: dict) -> int:
        if h.get("category", "").lower() != cat.lower():
            return 0
        return 3 if h.get("price", 9999) <= max_price else 1
    return grade

# ── Query set ─────────────────────────────────────────────────────────────────
# Each entry: (query_string, hits_per_page, optional_facet_filters, grade_fn, description)

QUERIES: List[tuple] = [
    # ── Category navigation ──────────────────────────────────────────────────
    ("running shoe",        10, [],                         _cat_kw("Footwear", "running"),           "footwear / running shoes"),
    ("hiking boot",         10, [],                         _cat_kw("Footwear", "hiking", "boot"),    "footwear / hiking boots"),
    ("yoga mat",            10, [],                         _cat_kw("Fitness", "yoga"),               "fitness / yoga mats"),
    ("wireless headphones", 10, [],                         _cat_kw("Electronics", "headphones"),     "electronics / headphones"),
    ("espresso machine",    10, [],                         _cat_kw("Kitchen & Home", "espresso"),    "kitchen / espresso machines"),
    ("down jacket",         10, [],                         _cat_kw("Clothing", "down", "jacket"),    "clothing / down jackets"),
    ("backpack",            10, [],                         _cat_kw("Outdoor & Garden", "backpack"),  "outdoor / backpacks"),
    ("kubernetes book",     10, [],                         _cat_kw("Books", "kubernetes"),           "books / kubernetes"),

    # ── Brand searches ───────────────────────────────────────────────────────
    ("Sony headphones",     10, [],                         _brand_cat("Sony", "Electronics"),        "brand: Sony electronics"),
    ("Nike running",        10, [],                         _brand_cat("Nike", "Footwear"),           "brand: Nike footwear"),
    ("Garmin watch",        10, [],                         _brand_cat("Garmin", "Fitness"),          "brand: Garmin fitness"),
    ("Weber grill",         10, [],                         _brand_cat("Weber", "Outdoor & Garden"),  "brand: Weber outdoor"),
    ("KitchenAid mixer",    10, [],                         _brand_cat("KitchenAid", "Kitchen & Home"),"brand: KitchenAid kitchen"),
    ("Patagonia jacket",    10, [],                         _brand_cat("Patagonia", "Clothing"),      "brand: Patagonia clothing"),

    # ── Attribute / feature searches ─────────────────────────────────────────
    ("noise cancelling",    10, [],                         _kw("noise cancelling", "anc",            cats=["Electronics"]), "feature: ANC"),
    ("waterproof",          10, [],                         _kw("waterproof", "ipx7", "gore-tex",     cats=["Footwear","Clothing","Outdoor & Garden"]), "feature: waterproof"),
    ("carbon fibre",        10, [],                         _kw("carbon", "carbon fibre", "carbon-fibre"), "feature: carbon fibre"),
    ("adjustable",          10, [],                         _kw("adjustable",                         cats=["Fitness"]), "feature: adjustable (fitness)"),
    ("bluetooth",           10, [],                         _kw("bluetooth",                          cats=["Electronics","Fitness"]), "feature: bluetooth"),
    ("4K",                  10, [],                         _kw("4k",                                 cats=["Electronics"]), "feature: 4K display"),

    # ── Facet-filtered searches ───────────────────────────────────────────────
    ("",                    10, [["category:Electronics"]], _cat("Electronics"),                      "facet: all Electronics"),
    ("",                    10, [["category:Footwear"]],    _cat("Footwear"),                         "facet: all Footwear"),
    ("shoe",                10, [["category:Footwear"]],    _cat_kw("Footwear", "shoe"),              "facet+query: Footwear shoe"),
    ("",                    10, [["category:Books"]],       _cat("Books"),                            "facet: all Books"),

    # ── Semantic / long-tail ─────────────────────────────────────────────────
    ("gift for runner",     10, [],                         _cat("Footwear"),                         "semantic: gift for runner"),
    ("home office setup",   10, [],                         _kw("monitor", "keyboard", "webcam",      cats=["Electronics"]), "semantic: home office"),
    ("camping gear",        10, [],                         _cat("Outdoor & Garden"),                 "semantic: camping gear"),
    ("healthy cooking",     10, [],                         _cat("Kitchen & Home"),                   "semantic: healthy cooking"),
    ("winter workout",      10, [],                         _kw("insulated", "thermal", "fleece",     cats=["Clothing","Fitness"]), "semantic: winter workout"),
    ("distributed systems", 10, [],                         _cat_kw("Books", "distributed"),          "semantic: distributed systems book"),
]


# ── HTTP helper ───────────────────────────────────────────────────────────────

def search(server: str, index: str, query: str, hpg: int, facet_filters: list) -> List[dict]:
    url = f"{server}/1/indexes/{index}/query"
    body = {"query": query, "hitsPerPage": hpg}
    if facet_filters:
        body["facetFilters"] = facet_filters
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        url,
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            return json.loads(resp.read())["hits"]
    except urllib.error.URLError as e:
        print(f"  ERROR: {e}", file=sys.stderr)
        return []


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--server",  default="http://localhost:8108")
    ap.add_argument("--index",   default="products")
    ap.add_argument("--top-k",   type=int, default=10)
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()

    K = args.top_k
    results = []

    print(f"Evaluating {len(QUERIES)} queries against {args.server} / {args.index}  (NDCG@{K}, MRR, P@5)\n")
    print(f"{'Query':<32} {'NDCG@10':>7} {'MRR':>7} {'P@5':>7}  Description")
    print("-" * 80)

    for q_str, hpg, ff, grade_fn, desc in QUERIES:
        hits = search(args.server, args.index, q_str, hpg, ff)
        grades = [grade_fn(h) for h in hits]

        # Pad to at least K so metrics are comparable
        while len(grades) < K:
            grades.append(0)

        n = ndcg(grades, K)
        m = mrr(grades)
        p = precision_at(grades, 5)
        results.append((n, m, p))

        label = (q_str if q_str else f"[{ff[0][0] if ff else ''}]")[:31]
        print(f"{label:<32} {n:>7.3f} {m:>7.3f} {p:>7.3f}  {desc}")

        if args.verbose and hits:
            for i, (h, g) in enumerate(zip(hits[:5], grades[:5])):
                title = str(h.get("title",""))[:50]
                score = h.get("_score", 0)
                print(f"    {i+1}. [{g}] {title}  (score={score:.4f})")

    avg_ndcg = sum(r[0] for r in results) / len(results)
    avg_mrr  = sum(r[1] for r in results) / len(results)
    avg_p5   = sum(r[2] for r in results) / len(results)

    print("-" * 80)
    print(f"{'MACRO AVERAGE':<32} {avg_ndcg:>7.3f} {avg_mrr:>7.3f} {avg_p5:>7.3f}")
    print()

    # Worst queries — useful for iterating on ranking
    ranked = sorted(zip([q[4] for q in QUERIES], results), key=lambda x: x[1][0])
    print("Bottom 5 by NDCG@10:")
    for desc, (n, m, p) in ranked[:5]:
        print(f"  {n:.3f}  {desc}")


if __name__ == "__main__":
    main()
