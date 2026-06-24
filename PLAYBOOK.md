# Playbook — vectoria-algolia

Complete runbook for building, running, and loading data.

---

## Requirements

| Tool | Version | Check |
|------|---------|-------|
| Docker | 24+ | `docker --version` |
| Docker Compose | v2 | `docker compose version` |
| Rust | 1.88+ | `rustc --version` (local dev only) |
| Node.js | 22+ | `node --version` (local dev only) |
| jq | any | `jq --version` (load script only) |

---

## Quick start (Docker — recommended)

```sh
# 1. Clone
git clone https://github.com/gleicon/vectoria-algolia
cd vectoria-algolia

# 2. Build and start (downloads embedding model ~40 MB on first run)
docker compose up --build

# 3. Wait for healthy (~60s first run while model downloads), then load products
docker compose run --rm loader

# 4. Open the demo
open http://localhost:8108
```

The single container serves both the Algolia-compatible API and the React demo.

---

## Local development

### Start the search server

```sh
# Build and start (uses in-memory engine, no persistence)
cargo run

# With edgestore persistence
VECTORIA_STORAGE_PATH=./data cargo run
```

Server starts on `http://localhost:8108`.

> **Single-index model.** The server creates exactly one index at startup, named
> by `VECTORIA_INDEX` (default: `products`). Requests to any other index name
> return 404. All loaders and the demo must use the same index name. To switch
> datasets, set `VECTORIA_INDEX=wands` (or any name) before starting the server
> and pass `--index wands` to the loader script.

### Load products

```sh
# Load sample dataset (50 products across 5 categories)
./scripts/load_products.sh

# Custom server / index
./scripts/load_products.sh http://localhost:8108 products
```

Verify:
```sh
curl -s -X POST http://localhost:8108/1/indexes/products/query \
  -H 'Content-Type: application/json' \
  -d '{"query": "running shoe", "hitsPerPage": 3}' | jq '.hits[].objectID'
```

### Start the React demo

```sh
cd demo
npm install
npm run dev          # starts at http://localhost:3000
```

Vite proxies `/1/*` → `localhost:8108`, so no CORS config needed in dev.

---

## API reference (Algolia-compatible)

### Index a product

```sh
curl -X PUT http://localhost:8108/1/indexes/products/objects/my-sku-001 \
  -H 'Content-Type: application/json' \
  -d '{
    "title": "My Product",
    "brand": "Acme",
    "category": "Electronics",
    "description": "Great product with many features.",
    "price": 99.99,
    "in_stock": true
  }'
```

Response:
```json
{"objectID": "my-sku-001", "taskID": 1}
```

### Batch index

```sh
curl -X POST http://localhost:8108/1/indexes/products/batch \
  -H 'Content-Type: application/json' \
  -d '{
    "requests": [
      {"action": "addObject", "body": {"objectID": "p1", "title": "...", "price": 50}},
      {"action": "addObject", "body": {"objectID": "p2", "title": "...", "price": 80}}
    ]
  }'
```

### Search

```sh
curl -s -X POST http://localhost:8108/1/indexes/products/query \
  -H 'Content-Type: application/json' \
  -d '{
    "query": "wireless headphones",
    "hitsPerPage": 10,
    "facets": ["brand", "category"],
    "filters": "category:Electronics AND price <= 300"
  }' | jq '.'
```

### Multi-search (used by InstantSearch internally)

```sh
curl -s -X POST 'http://localhost:8108/1/indexes/_/queries' \
  -H 'Content-Type: application/json' \
  -d '{
    "requests": [
      {"indexName": "products", "query": "shoe", "hitsPerPage": 5},
      {"indexName": "products", "query": "yoga",  "hitsPerPage": 5}
    ]
  }' | jq '.results | length'
```

---

## Filter syntax

| Filter | Example | Notes |
|--------|---------|-------|
| Attribute equals | `brand:Nike` | String or boolean |
| AND chain | `brand:Nike AND in_stock:true` | Multiple attributes |
| Price ≥ | `price >= 100` | Numeric range |
| Price range | `price >= 100 AND price <= 200` | Maps to price_min/price_max |
| Boolean | `in_stock:true` | Exact match |

OR and NOT are not supported.

---

## Connecting an existing InstantSearch app

### algoliasearch v5 (liteClient)

```ts
import { liteClient } from 'algoliasearch/lite'

const searchClient = liteClient('unused', 'unused', {
  hosts: [{ url: 'localhost:8108', protocol: 'http', accept: 'readWrite' }],
})
```

### Custom searchClient (no algoliasearch dependency)

```ts
const searchClient = {
  search(requests: unknown[]) {
    return fetch('http://localhost:8108/1/indexes/_/queries', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ requests }),
    }).then(r => r.json())
  },
}
```

### Quickstart repos

**product-search-react-typescript**
<https://github.com/algolia/quickstarts/tree/main/product-search-react-typescript>

In `src/main.tsx`:
```ts
import { liteClient } from 'algoliasearch/lite'
const searchClient = liteClient('local', 'local', {
  hosts: [{ url: 'localhost:8108', protocol: 'http', accept: 'readWrite' }],
})
```

**InstantSearch + Next.js starter**
<https://www.algolia.com/developers/code-exchange/instantsearch-and-next-js-starter>

In `.env.local`:
```
NEXT_PUBLIC_ALGOLIA_APP_ID=local
NEXT_PUBLIC_ALGOLIA_SEARCH_KEY=local
```

Override the client initialization with the custom host above.

---

## Docker image details

Single multi-stage image:
1. `node:22-alpine` — builds the React demo (`demo/dist/`)
2. `rust:1.80-slim` — builds the Rust binary
3. `debian:bookworm-slim` — final image with binary + static files

The binary serves:
- `POST /1/indexes/{name}/query` — search
- `POST /1/indexes/{*}/queries` — multi-search  
- `PUT  /1/indexes/{name}/objects/{id}` — index object
- `POST /1/indexes/{name}/batch` — batch index/delete
- `GET  /*` — React demo static files (via `STATIC_DIR`)

Volumes:
- `/data` — fastembed model cache + optional persistent index

First container start downloads `multilingual-e5-small` (~40 MB) from HuggingFace Hub.

---

## Running tests

```sh
cargo test
```

29 tests:
- 7 filter_parser unit tests
- 3 ingest unit tests
- 11 route integration tests (axum + stub embedding, no model download)
- 8 doc tests

---

## Search quality evaluation

`scripts/quality_eval.py` measures ranking quality against a curated 30-query test set.
No extra dependencies — stdlib only (Python 3.7+).

### Metrics

| Metric | What it measures |
|--------|-----------------|
| **NDCG@10** | Ranking quality — high-relevance results ranked near the top score higher |
| **MRR** | Mean Reciprocal Rank — how high the first relevant result appears |
| **P@5** | Precision at 5 — fraction of the top 5 hits that are relevant |

Each hit is graded 0–3 by a relevance function (category match, keyword match, brand match).

### Running

```sh
# Server must be running and products loaded first
python3 scripts/quality_eval.py

# Custom server / index
python3 scripts/quality_eval.py --server http://localhost:8108 --index products

# Show top-5 hits with grades per query
python3 scripts/quality_eval.py --verbose
```

### Query set

30 queries across five types:

| Type | Count | Example |
|------|-------|---------|
| Category navigation | 8 | `running shoe`, `espresso machine` |
| Brand search | 6 | `Sony headphones`, `Patagonia jacket` |
| Attribute / feature | 6 | `noise cancelling`, `waterproof`, `4K` |
| Facet-filtered | 4 | empty query + `category:Electronics` |
| Semantic / long-tail | 6 | `gift for runner`, `home office setup` |

### Baseline results (550-product dataset)

```
MACRO AVERAGE   NDCG@10=0.930   MRR=0.910   P@5=0.867
```

Known weak queries and root causes:

| Query | NDCG | Cause |
|-------|------|-------|
| "home office setup" | 0.000 | conceptual query — no product contains those exact terms |
| "winter workout" | 0.301 | indirect intent: insulated/thermal/fleece items score low without exact keyword |
| "healthy cooking" | 0.600 | indirect intent: signal is weak between query and Kitchen & Home |

### Adding queries

Append to the `QUERIES` list in `quality_eval.py`. Each entry is a 5-tuple:

```python
(query_string, hits_per_page, facet_filters, grade_fn, description)
```

Built-in grade helpers:

```python
_cat("Electronics")                        # relevant if category matches
_cat_kw("Footwear", "running", "trail")    # perfect if category + keyword; partial if category only
_brand_cat("Sony", "Electronics")          # grades by brand + category match
_kw("waterproof", cats=["Clothing"])       # relevant if keyword in title/description
```

---

## Real-world datasets

The default dataset (`scripts/products.json`) is synthetic. Two scripts load
real publicly-licensed datasets for deeper experimentation.

### WANDS — Wayfair Annotated Dataset

~42K furniture/home products, 480 annotated queries, relevance grades.
No extra dependencies — stdlib only.

[https://github.com/wayfair/WANDS](https://github.com/wayfair/WANDS) (Apache-2.0)

**Docker (recommended):**
```sh
# Start server, load all WANDS products, run quality eval
docker compose up --build -d
docker compose --profile wands run --rm wands

# Custom options via COMMAND override
docker compose --profile wands run --rm wands \
  python3 /scripts/wands_setup.py --server http://search:8108 \
  --max-products 5000 --max-queries 50 --eval
```

**Local:**
```sh
# Server must be running
python3 scripts/wands_setup.py --eval
python3 scripts/wands_setup.py --max-products 5000 --max-queries 50 --eval
python3 scripts/wands_setup.py --skip-load --eval --verbose
```

### Amazon ESCI — Shopping Queries Dataset

~482K English products, real e-commerce search queries with E/S/C/I relevance
labels (Exact / Substitute / Complement / Irrelevant).
Parquet files (~200 MB) are downloaded once and cached in a Docker volume.

[https://github.com/amazon-science/esci-data](https://github.com/amazon-science/esci-data) (Apache-2.0)

**Docker (recommended — pandas/pyarrow pre-installed in image):**
```sh
# Build image (first run), download ESCI, load 5 000 products, eval 200 queries
docker compose up --build -d
docker compose --profile esci run --rm esci

# Load 20 000 products
docker compose --profile esci run --rm esci \
  python3 /scripts/esci_setup.py --server http://search:8108 \
  --max-products 20000 --eval --cache-dir /scripts/.esci_cache

# Skip loading (parquet already cached in volume), eval only
docker compose --profile esci run --rm esci \
  python3 /scripts/esci_setup.py --server http://search:8108 \
  --skip-load --eval --verbose --cache-dir /scripts/.esci_cache
```

**Local:**
```sh
pip install pandas pyarrow
python3 scripts/esci_setup.py --eval
python3 scripts/esci_setup.py --max-products 20000 --eval
python3 scripts/esci_setup.py --skip-load --eval --verbose
```

Parquet files are cached in `.esci_cache/` locally or the `esci-cache` Docker
volume between runs — subsequent runs skip the download.

### Notes

- WANDS and ESCI use separate `objectID` prefixes (`wands-*`, `esci-*`) so they
  can coexist with the default synthetic dataset in the same index.
- Both scripts accept `--server` and `--index` to target any running instance.
- Quality metrics from these datasets are directly comparable to the synthetic
  baseline because they use the same NDCG@10 / MRR / P@5 implementation.
