# vectoria-algolia

HTTP search adapter for [Vectoria](https://github.com/gleicon/vectoria) that speaks the same search protocol used by [Algolia Search](https://www.algolia.com/), [Typesense](https://typesense.org/), and other engines that support the [InstantSearch](https://www.algolia.com/doc/guides/building-search-ui/what-is-instantsearch/js/) widget ecosystem.

Point an existing InstantSearch, autocomplete, or `algoliasearch` client at `localhost:8108` instead of `algolia.net` and it works without code changes beyond the host override.

---

## Compatibility

This project implements the HTTP search protocol independently, based on the publicly documented [Algolia Search REST API](https://www.algolia.com/doc/rest-api/search/) and the open-source [InstantSearch](https://github.com/algolia/instantsearch) libraries (Apache 2.0). No Algolia account or credentials are required.

The same protocol is also supported, to varying degrees, by:
- [Typesense](https://typesense.org/docs/guide/algolia-migration.html) — open-source search engine with an Algolia-compatible adapter
- [Meilisearch](https://www.meilisearch.com/) — open-source search engine with a similar query model
- [OpenSearch](https://opensearch.org/) — via community plugins

_Algolia_ is a registered trademark of Algolia, Inc. _InstantSearch_ libraries are open-source and maintained by Algolia under the Apache 2.0 license.

---

## What is implemented

| Feature | Notes |
|---------|-------|
| `POST /1/indexes/{name}/query` | Single-index search |
| `POST /1/indexes/*/queries` | Multi-search batch — used by InstantSearch internally |
| `PUT /1/indexes/{name}/objects/{id}` | Index a single object |
| `POST /1/indexes/{name}/batch` | Batch index / delete |
| `hits`, `nbHits`, `page`, `nbPages`, `hitsPerPage` | Standard response envelope |
| `facets` | Aggregated counts per field |
| `facetFilters` | `[["attr:val"]]` nested-array syntax from `RefinementList` |
| `filters` | `brand:Nike AND price >= 100` string syntax |
| `numericFilters` | Array of numeric range strings |
| `_highlightResult` | Per-field `value` / `matchLevel` / `matchedWords` / `fullyHighlighted` |
| Custom `highlightPreTag` / `highlightPostTag` | Defaults to AIS tags used by `<Highlight>` |
| `processingTimeMS`, `queryID`, `exhaustiveNbHits` | Present on every response |
| `searchMode` | Non-standard extension: `hybrid` (default) / `semantic` / `bm25` |

## What is NOT implemented

| Feature | Notes |
|---------|-------|
| `_snippetResult` | Not yet |
| Rules, Synonyms, A/B testing | Out of scope |
| Analytics (`POST /1/events`) | Accepted but ignored |
| Index settings (`PUT /1/indexes/{name}/settings`) | Out of scope |
| `OR` / `NOT` in filter strings | Only `AND` chains |

---

## Quick start

```sh
git clone https://github.com/gleicon/vectoria-algolia
cd vectoria-algolia
docker compose up --build   # builds, starts, and loads 550 sample products
open http://localhost:8108
```

See **[PLAYBOOK.md](PLAYBOOK.md)** for the full runbook: local dev, API reference, filter syntax, quality evaluation.

---

## Wiring up an existing InstantSearch app

### algoliasearch v5 (recommended)

```ts
import { liteClient } from 'algoliasearch/lite'

const searchClient = liteClient('local', 'local', {
  hosts: [{ url: 'localhost:8108', protocol: 'http', accept: 'readWrite' }],
})
```

Drop this `searchClient` into any `<InstantSearch>` tree. The app ID and key fields are accepted but never validated.

### Custom adapter (zero dependencies)

```ts
const searchClient = {
  search(requests) {
    return fetch('http://localhost:8108/1/indexes/*/queries', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ requests }),
    }).then(r => r.json())
  },
}
```

### Quickstart repos

**product-search-react-typescript** — `src/main.tsx`:
```ts
const searchClient = liteClient(
  import.meta.env.VITE_ALGOLIA_APP_ID,
  import.meta.env.VITE_ALGOLIA_SEARCH_KEY,
  { hosts: [{ url: 'localhost:8108', protocol: 'http' }] }
)
```

**Next.js + InstantSearch starter** — `src/lib/algolia.ts`:
```ts
export const searchClient = liteClient('local', 'local', {
  hosts: [{ url: process.env.NEXT_PUBLIC_SEARCH_URL ?? 'localhost:8108', protocol: 'http' }],
})
```

Set `NEXT_PUBLIC_SEARCH_URL=localhost:8108` in `.env.local`.

---

## Tests

Three test layers ship with the project.

### Rust unit + integration tests

Uses a stub embedding provider — no model download, runs in milliseconds.

```sh
cargo test
```

**40 tests, 0 failed:**

| Suite | Count | Covers |
|-------|------:|--------|
| `filter_parser` unit | 10 | filter-string parsing, NaN/Infinity guards |
| `translate` unit | 9 | highlight matching, unicode, edge cases |
| `ingest` unit | 3 | object ingestion, missing-objectID handling |
| route integration | 18 | full HTTP round-trips via axum test client |

Route integration tests (the 18):

| Category | Test |
|----------|------|
| Indexing | PUT single object returns 200 + objectID |
| Indexing | PUT to unknown index returns 404 |
| Indexing | batch indexes multiple objects |
| Search | query returns hits and nbHits |
| Search | unknown index returns 404 |
| Search | pagination fields present |
| Filters | `filters` string restricts by category |
| Filters | price range filter |
| Filters | `numericFilters` array |
| Facets | `facets` param returns aggregated counts |
| Facets | `facetFilters` restricts results |
| `_highlightResult` | present on every hit |
| `_highlightResult` | AIS tags wrap matched token |
| `_highlightResult` | empty query → `matchLevel: none` |
| Multi-search | two requests → two independent results |
| Multi-search | unknown index returns 404 |
| Multi-search | disjunctive: filtered ≤ unfiltered nbHits |
| Multi-search | URL-encoded params forwarded correctly |

### Client compatibility tests (Node.js)

Uses the real `algoliasearch` v5 `liteClient` against a live server. Verifies the wire format as a client application would see it.

**Single command (Docker):**
```sh
docker compose --profile test up --build --exit-code-from test
# Docker prints ERRO[N] N at the end — that's the exit code, not an error.
# ERRO[N] 0 means all tests passed.
```

Or locally, with a server already running:
```sh
cd tests/client && SERVER_URL=http://localhost:8108 npx vitest run
```

**Latest results (20 tests, exit code 0):**

| Category | Test | |
|----------|------|-|
| Search | returns `hits` array and `nbHits` | ✓ |
| Search | every hit has `objectID` | ✓ |
| Search | empty query returns all documents | ✓ |
| Search | `hitsPerPage` is respected | ✓ |
| Search | `page` / `nbPages` fields present | ✓ |
| `_highlightResult` | present on every hit | ✓ |
| `_highlightResult` | per-field `value` / `matchLevel` / `matchedWords` | ✓ |
| `_highlightResult` | AIS tags wrap matched token in field value | ✓ |
| `_highlightResult` | empty query → `matchLevel: none` | ✓ |
| `_highlightResult` | custom `highlightPreTag` / `highlightPostTag` | ✓ |
| Filters | string filter restricts by category | ✓ |
| Filters | price range filter | ✓ |
| `facetFilters` | nested-array `[["attr:val"]]` syntax | ✓ |
| Facets | counts returned when `facets` param is set | ✓ |
| Facets | counts are positive integers | ✓ |
| Multi-search | one result per request | ✓ |
| Multi-search | each result has `hits` and `nbHits` | ✓ |
| Multi-search | disjunctive: filtered ≤ unfiltered `nbHits` | ✓ |
| Pagination | page 1 returns different hits than page 0 | ✓ |
| Clear refinements | unfiltered `nbHits` > filtered `nbHits` | ✓ |

### Search quality evaluation

Measures ranking quality against a 30-query benchmark set using NDCG@10, MRR, and Precision@5. Requires a running server with products loaded.

```sh
python3 scripts/quality_eval.py
```

**Baseline (550 products, hybrid search):**
```
MACRO AVERAGE   NDCG@10=0.930   MRR=0.910   P@5=0.867
```

See [PLAYBOOK.md § Search quality evaluation](PLAYBOOK.md#search-quality-evaluation) for the full query set and per-query breakdown.

---

## Non-standard extension: `searchMode`

Pass `"searchMode"` in any query body to override the default hybrid ranking:

```json
{ "query": "running shoes", "hitsPerPage": 10, "searchMode": "semantic" }
```

| Value | Behaviour |
|-------|-----------|
| `hybrid` | BM25 + vector, re-ranked (default) |
| `semantic` | Vector-only |
| `bm25` | Keyword-only, no embeddings |

---

## Troubleshooting

### `GET /` returns 404, but `POST /query` returns 200

A local `vectoria-algolia` process is bound to port 8108 and intercepting requests before Docker's port mapping. This happens when you've previously run `cargo run` in the same shell session.

```sh
# Find and kill the local process
lsof -i :8108
kill <PID>

# Restart the Docker container so its port binding takes effect
docker compose restart search
```

### Site shows 404 in the browser after `docker compose up`

The `demo/dist/` directory may not have been built. The Docker image includes the React demo in its final stage. If you're running locally (not Docker), build the demo first:

```sh
cd demo && npm install && npm run build
cd ..
cargo run  # STATIC_DIR auto-detected from ./demo/dist
```

### Products missing after container restart

The index is in-memory by default and is cleared on restart. Reload products:

```sh
docker compose run --rm loader
# or locally:
./scripts/load_products.sh
```

To persist the index across restarts, mount a volume and set `VECTORIA_STORAGE_PATH`:

```sh
VECTORIA_STORAGE_PATH=./data cargo run
```

---

## License

Apache-2.0
