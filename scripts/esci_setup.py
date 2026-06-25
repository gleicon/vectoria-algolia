#!/usr/bin/env python3
"""
Amazon ESCI (Shopping Queries Dataset) loader for vectoria-algolia.

Downloads a subset of Amazon's public product search dataset (English locale),
loads products into the running search server, then evaluates ranking quality
using the real ESCI relevance labels (Exact / Substitute / Complement / Irrelevant).

Dataset: https://github.com/amazon-science/esci-data  (Apache-2.0)
Requires: pip install pandas pyarrow

The full dataset is ~200 MB of parquet files. This script downloads them once,
caches locally, and loads a configurable subset.

Usage:
    pip install pandas pyarrow
    python3 scripts/esci_setup.py
    python3 scripts/esci_setup.py --max-products 5000 --max-queries 100
    python3 scripts/esci_setup.py --skip-load --eval
    python3 scripts/esci_setup.py --server http://localhost:8108 --index products
"""

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Dict, List, Optional
from metrics import dcg, ndcg, mrr, precision_at, batch_load

try:
    import pandas as pd
except ImportError:
    print("ERROR: pandas is required.  pip install pandas pyarrow", file=sys.stderr)
    sys.exit(1)

PRODUCTS_URL = (
    "https://github.com/amazon-science/esci-data/raw/main/shopping_queries_dataset/"
    "shopping_queries_dataset_products.parquet"
)
EXAMPLES_URL = (
    "https://github.com/amazon-science/esci-data/raw/main/shopping_queries_dataset/"
    "shopping_queries_dataset_examples.parquet"
)

CACHE_DIR = Path(os.environ.get("ESCI_CACHE", ".esci_cache"))

# ESCI relevance label → NDCG grade (0–3)
ESCI_GRADE = {"E": 3, "S": 2, "C": 1, "I": 0}


def download_cached(url: str, dest: Path) -> Path:
    if dest.exists():
        print(f"  cached: {dest}")
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)
    print(f"  downloading {url} ...")
    req = urllib.request.Request(url, headers={"User-Agent": "vectoria-esci-setup/1.0"})
    try:
        with urllib.request.urlopen(req, timeout=120) as r, open(dest, "wb") as f:
            total = int(r.headers.get("Content-Length", 0))
            done = 0
            while chunk := r.read(65536):
                f.write(chunk)
                done += len(chunk)
                if total:
                    print(f"    {done//1024//1024} / {total//1024//1024} MB", end="\r", flush=True)
            print()
    except urllib.error.URLError as e:
        print(f"  ERROR downloading {url}: {e}", file=sys.stderr)
        dest.unlink(missing_ok=True)
        sys.exit(1)
    return dest


def _infer_category(row: pd.Series) -> str:
    """Best-effort category from ESCI product data (no explicit category field)."""
    bullets = str(row.get("product_bullet_point", "") or "")
    # Use first bullet point word as a rough category signal
    if bullets:
        first = bullets.split("\n")[0][:60]
        return first if first else "General"
    return "General"


def build_products(df_products: pd.DataFrame, max_products: int) -> List[dict]:
    # English locale only, drop duplicates
    df = df_products[df_products["product_locale"] == "us"].drop_duplicates("product_id")
    if max_products:
        df = df.head(max_products)

    products = []
    for _, row in df.iterrows():
        pid   = str(row["product_id"]).strip()
        title = str(row.get("product_title", "") or "").strip()
        desc  = str(row.get("product_description", "") or "").strip()
        brand = str(row.get("product_brand", "") or "").strip() or "Unknown"
        color = str(row.get("product_color", "") or "").strip()
        if not pid or not title:
            continue
        if color and color.lower() not in desc.lower():
            desc = f"{color} — {desc}" if desc else color
        products.append({
            "objectID":    f"esci-{pid}",
            "title":       title[:300],
            "description": desc[:500],
            "brand":       brand[:100],
            "category":    _infer_category(row),
            # ESCI has no price or stock — synthesize from hash
            "price":       round(9.99 + abs(hash(pid)) % 49000 / 100, 2),
            "in_stock":    abs(hash(pid + "s")) % 10 > 1,
        })
    return products


def build_labels(
    df_examples: pd.DataFrame,
    max_queries: int,
) -> tuple[Dict[str, str], Dict[str, Dict[str, int]]]:
    """Returns (queries, labels) for English queries with at least one relevant hit."""
    df = df_examples[df_examples["product_locale"] == "us"].copy()
    df["grade"] = df["esci_label"].map(ESCI_GRADE).fillna(0).astype(int)

    queries: Dict[str, str] = {}
    labels:  Dict[str, Dict[str, int]] = {}

    for qid, group in df.groupby("query_id"):
        qtext = group["query"].iloc[0]
        grade_map = {f"esci-{pid}": g for pid, g in
                     zip(group["product_id"], group["grade"])}
        if any(g > 0 for g in grade_map.values()):
            queries[str(qid)] = str(qtext)
            labels[str(qid)]  = grade_map
        if len(queries) >= max_queries:
            break

    return queries, labels


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
    verbose: bool = False,
) -> None:
    print(f"\nEvaluating {len(queries)} queries  (NDCG@{k}, MRR, P@5)\n")
    print(f"{'Query':<40} {'NDCG@10':>7} {'MRR':>7} {'P@5':>7}")
    print("-" * 65)

    results = []
    for qid, qtext in queries.items():
        hits   = search(server, index, qtext, k)
        qgrades = labels.get(qid, {})
        grades  = [qgrades.get(h["objectID"], 0) for h in hits]
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
    ap.add_argument("--max-products", type=int, default=5000,
                    help="Products to load (default 5000; 0 = all ~480K english)")
    ap.add_argument("--max-queries",  type=int, default=200,
                    help="Queries to evaluate (default 200)")
    ap.add_argument("--skip-load",    action="store_true",
                    help="Skip loading — only run quality eval")
    ap.add_argument("--eval",         action="store_true",
                    help="Run quality eval after loading")
    ap.add_argument("--cache-dir",    default=".esci_cache",
                    help="Local directory for downloaded parquet files")
    ap.add_argument("--verbose",      action="store_true")
    args = ap.parse_args()

    cache = Path(args.cache_dir)

    products_parquet = download_cached(PRODUCTS_URL, cache / "products.parquet")
    examples_parquet = download_cached(EXAMPLES_URL, cache / "examples.parquet")

    print("Reading parquet files ...")
    df_products = pd.read_parquet(products_parquet)
    df_examples = pd.read_parquet(examples_parquet)
    print(f"  {len(df_products)} total products, {df_examples['query_id'].nunique()} total queries")

    if not args.skip_load:
        print(f"Converting products (max {args.max_products or 'all'}) ...")
        products = build_products(df_products, args.max_products)
        print(f"  {len(products)} products ready")
        print(f"Loading into {args.server}/{args.index} ...")
        batch_load(args.server, args.index, products)
        print("  done")
        time.sleep(1)

    if args.eval or args.skip_load:
        print(f"Building query set (max {args.max_queries} queries) ...")
        queries, labels = build_labels(df_examples, args.max_queries)
        print(f"  {len(queries)} evaluable queries")
        evaluate(args.server, args.index, queries, labels,
                 verbose=args.verbose)


if __name__ == "__main__":
    main()
