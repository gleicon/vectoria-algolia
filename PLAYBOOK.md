# Playbook — vectoria-algolia

Complete runbook for building, running, and loading data.

---

## Requirements

| Tool | Version | Check |
|------|---------|-------|
| Docker | 24+ | `docker --version` |
| Docker Compose | v2 | `docker compose version` |
| Rust | 1.85+ | `rustc --version` (local dev only) |
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
