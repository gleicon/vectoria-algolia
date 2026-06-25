#!/usr/bin/env python3
"""
WANDS (Wayfair Annotated Dataset) loader for vectoria-algolia.

Downloads ~42K real furniture/home products and 480 annotated queries from
Wayfair's public dataset, loads them into the running search server, then
runs a quality evaluation using the real relevance labels.

Dataset: https://github.com/wayfair/WANDS  (Apache-2.0)
Stdlib only — no extra dependencies.

Usage:
    python3 scripts/wands_setup.py
    python3 scripts/wands_setup.py --max-products 5000
    python3 scripts/wands_setup.py --skip-load --eval
    python3 scripts/wands_setup.py --server http://localhost:8108 --index products
"""

import argparse
import csv
import io
import json
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Dict, List, Optional, Tuple
from metrics import dcg, ndcg, mrr, precision_at, batch_load

BASE_URL = "https://raw.githubusercontent.com/wayfair/WANDS/main/dataset"
PRODUCTS_URL = f"{BASE_URL}/product.csv"
QUERIES_URL  = f"{BASE_URL}/query.csv"
LABELS_URL   = f"{BASE_URL}/label.csv"

# Synthetic price bands per product class prefix — WANDS has no price field.
_PRICE_MAP = {
    "sofa": (499, 3499), "sectional": (799, 4999), "bed ": (299, 2999),
    "mattress": (199, 2499), "dresser": (199, 999), "nightstand": (79, 499),
    "desk": (99, 799), "chair": (79, 1299), "table": (99, 1999),
    "bookcase": (79, 599), "storage": (49, 499), "lamp": (29, 299),
    "rug": (49, 999), "curtain": (29, 199), "mirror": (49, 599),
    "outdoor": (99, 1999), "patio": (199, 2499), "grill": (99, 999),
}

_BRANDS = [
    "Allmodern", "Birch Lane", "Joss & Main", "Perigold", "Mercury Row",
    "Corrigan Studio", "Laurel Foundry", "Sand & Stable", "Three Posts",
    "Beachcrest Home", "Highland Dunes", "Andover Mills", "Brayden Studio",
    "George Oliver", "Wade Logan", "Zipcode Design", "Foundstone",
]

def _price_for(product_class: str) -> float:
    lc = product_class.lower()
    for key, (lo, hi) in _PRICE_MAP.items():
        if key in lc:
            h = abs(hash(lc)) % 100
            return round(lo + (hi - lo) * h / 100, 2)
    h = abs(hash(lc)) % 100
    return round(49 + 1450 * h / 100, 2)

def _brand_for(product_id: str) -> str:
    return _BRANDS[abs(hash(product_id)) % len(_BRANDS)]

def _in_stock(product_id: str) -> bool:
    return abs(hash(product_id + "stock")) % 10 > 1  # ~80% in stock


def download_cached(url: str, cache_dir: Optional[str]) -> str:
    if cache_dir:
        name = url.rsplit("/", 1)[-1]
        path = Path(cache_dir) / name
        if path.exists():
            print(f"  cached: {path}")
            return path.read_text(encoding="utf-8")
        Path(cache_dir).mkdir(parents=True, exist_ok=True)

    print(f"  downloading {url} ...", flush=True)
    req = urllib.request.Request(url, headers={"User-Agent": "vectoria-wands-setup/1.0"})
    with urllib.request.urlopen(req, timeout=60) as r:
        text = r.read().decode("utf-8")

    if cache_dir:
        path.write_text(text, encoding="utf-8")
    return text


def load_products(text: str) -> List[dict]:
    reader = csv.DictReader(io.StringIO(text))
    products = []
    for row in reader:
        pid = row.get("id", "").strip()
        title = row.get("product_name", "").strip()
        desc  = row.get("product_description", "").strip()
        cat   = row.get("product_class", "").strip()
        if not pid or not title:
            continue
        products.append({
            "objectID":    f"wands-{pid}",
            "title":       title,
            "description": desc[:500] if desc else "",
            "category":    cat,
            "brand":       _brand_for(pid),
            "price":       _price_for(cat),
            "in_stock":    _in_stock(pid),
        })
    return products


def load_queries(text: str) -> Dict[str, str]:
    """Returns {query_id: query_string}."""
    reader = csv.DictReader(io.StringIO(text))
    return {r["id"].strip(): r["query"].strip() for r in reader if r.get("query")}


def load_labels(text: str) -> Dict[str, Dict[str, int]]:
    """Returns {query_id: {objectID: grade}}."""
    grade_map = {"Exact": 3, "Partial": 1, "Irrelevant": 0}
    labels: Dict[str, Dict[str, int]] = {}
    reader = csv.DictReader(io.StringIO(text))
    for row in reader:
        qid  = row.get("query_id", "").strip()
        pid  = row.get("product_id", "").strip()
        lbl  = row.get("label", "").strip()
        if qid and pid and lbl in grade_map:
            labels.setdefault(qid, {})[f"wands-{pid}"] = grade_map[lbl]
    return labels




def search(server: str, index: str, query: str, k: int) -> List[dict]:
    url = f"{server}/1/indexes/{index}/query"
    body = json.dumps({"query": query, "hitsPerPage": k}).encode()
    req = urllib.request.Request(
        url, data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return json.loads(r.read())["hits"]
    except urllib.error.URLError as e:
        print(f"  ERROR: {e}", file=sys.stderr)
        return []


def evaluate(
    server: str,
    index: str,
    queries: Dict[str, str],
    labels: Dict[str, Dict[str, int]],
    k: int = 10,
    max_queries: int = 100,
    verbose: bool = False,
) -> None:
    # Only evaluate queries that have at least one relevant label.
    eval_queries = [
        (qid, qtext)
        for qid, qtext in queries.items()
        if qid in labels and any(g > 0 for g in labels[qid].values())
    ][:max_queries]

    print(f"\nEvaluating {len(eval_queries)} queries  (NDCG@{k}, MRR, P@5)\n")
    print(f"{'Query':<40} {'NDCG@10':>7} {'MRR':>7} {'P@5':>7}")
    print("-" * 65)

    results = []
    for qid, qtext in eval_queries:
        hits  = search(server, index, qtext, k)
        qgrades = labels[qid]
        grades = [qgrades.get(h["objectID"], 0) for h in hits]
        while len(grades) < k:
            grades.append(0)
        n = ndcg(grades, k)
        m = mrr(grades)
        p = precision_at(grades, 5)
        results.append((n, m, p))
        print(f"{qtext[:39]:<40} {n:>7.3f} {m:>7.3f} {p:>7.3f}")
        if verbose and hits:
            for i, (h, g) in enumerate(zip(hits[:5], grades[:5])):
                print(f"    {i+1}. [{g}] {str(h.get('title',''))[:60]}")

    if results:
        print("-" * 65)
        print(f"{'MACRO AVERAGE':<40} "
              f"{sum(r[0] for r in results)/len(results):>7.3f} "
              f"{sum(r[1] for r in results)/len(results):>7.3f} "
              f"{sum(r[2] for r in results)/len(results):>7.3f}")


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--server",       default="http://localhost:8108")
    ap.add_argument("--index",        default="products")
    ap.add_argument("--max-products", type=int, default=0,
                    help="Limit products loaded (0 = all ~42K)")
    ap.add_argument("--max-queries",  type=int, default=100,
                    help="Queries to evaluate (default 100)")
    ap.add_argument("--cache-dir",    default="",
                    help="Directory to cache downloaded CSV files (default: no cache)")
    ap.add_argument("--skip-load",    action="store_true",
                    help="Skip loading — only run quality eval")
    ap.add_argument("--eval",         action="store_true",
                    help="Run quality eval after loading (requires labels download)")
    ap.add_argument("--verbose",      action="store_true")
    args = ap.parse_args()

    cache = args.cache_dir or None

    if not args.skip_load:
        print("Downloading WANDS products ...")
        products = load_products(download_cached(PRODUCTS_URL, cache))
        if args.max_products:
            products = products[:args.max_products]
        print(f"  {len(products)} products ready")
        print(f"Loading into {args.server}/{args.index} ...")
        batch_load(args.server, args.index, products)
        print("  done")
        time.sleep(1)  # let engine settle

    if args.eval or args.skip_load:
        print("Downloading WANDS queries and labels ...")
        queries = load_queries(download_cached(QUERIES_URL, cache))
        labels  = load_labels(download_cached(LABELS_URL, cache))
        print(f"  {len(queries)} queries, "
              f"{sum(len(v) for v in labels.values())} label pairs")
        evaluate(
            args.server, args.index, queries, labels,
            max_queries=args.max_queries,
            verbose=args.verbose,
        )


if __name__ == "__main__":
    main()
