# vectoria-algolia

Algolia-compatible HTTP adapter for [Vectoria](https://github.com/gleicon/vectoria). Lets you use any Algolia client or InstantSearch widget against a Vectoria search engine — no Algolia account required.

## What it implements

The minimal surface that InstantSearch needs:

| Endpoint | Purpose |
|----------|---------|
| `POST /1/indexes/{name}/query` | Single-index search — used by most InstantSearch widgets |
| `POST /1/indexes/*/queries` | Multi-search batch — used when multiple widgets share a search client |

Response shape matches what `algoliasearch` and `@algolia/client-search` expect: `hits`, `nbHits`, `page`, `nbPages`, `hitsPerPage`, `facets`, `processingTimeMS`, `queryID`.

## What it does NOT implement

- Highlights / snippets (`_highlightResult`, `_snippetResult`)
- Rules, Synonyms, A/B testing
- Analytics API (`POST /1/events`)
- Index management (`PUT /1/indexes/{name}/settings`)
- `OR` / `NOT` in filter strings — only `AND` chains

## Getting started

```sh
git clone https://github.com/gleicon/vectoria-algolia
cd vectoria-algolia
cargo build --release
./target/release/vectoria-algolia
```

Default port: **8108**. Override with `PORT=...`.

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `8108` | Listen port |
| `VECTORIA_INDEX` | `products` | Default index name — must match your frontend's index name |

## Wiring up InstantSearch

### Option A — `algoliasearch` v5 with custom host (recommended)

```ts
import { liteClient } from 'algoliasearch/lite';

const searchClient = liteClient('unused-app-id', 'unused-api-key', {
  hosts: [{ url: 'localhost:8108', protocol: 'http' }],
});
```

Drop this `searchClient` into any InstantSearch app. The app ID and key are never validated.

### Option B — custom `searchClient` adapter (zero dependencies)

```ts
const searchClient = {
  search(requests) {
    return fetch('http://localhost:8108/1/indexes/*/queries', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ requests }),
    }).then(r => r.json());
  },
};
```

Pass this to `<InstantSearch searchClient={searchClient} indexName="products" />`.

## Connecting to the product-search-react-typescript quickstart

Reference: <https://github.com/algolia/quickstarts/tree/main/product-search-react-typescript>

1. Clone the quickstart.
2. In `.env.local`, replace the Algolia credentials with dummy values:

```env
VITE_ALGOLIA_APP_ID=local
VITE_ALGOLIA_SEARCH_KEY=local
VITE_ALGOLIA_INDEX_NAME=products
```

3. In `src/main.tsx` (or wherever `algoliasearch` is instantiated), add a custom hosts override:

```ts
import { liteClient } from 'algoliasearch/lite';

const searchClient = liteClient(
  import.meta.env.VITE_ALGOLIA_APP_ID,
  import.meta.env.VITE_ALGOLIA_SEARCH_KEY,
  { hosts: [{ url: 'localhost:8108', protocol: 'http' }] }
);
```

4. Import your product data into Vectoria, then start `vectoria-algolia`.

## Next.js + InstantSearch starter

Reference: <https://www.algolia.com/developers/code-exchange/instantsearch-and-next-js-starter>

In `src/lib/algolia.ts` (or equivalent):

```ts
import { liteClient } from 'algoliasearch/lite';

export const searchClient = liteClient('local', 'local', {
  hosts: [{ url: process.env.NEXT_PUBLIC_SEARCH_URL ?? 'localhost:8108', protocol: 'http' }],
});
```

Set `NEXT_PUBLIC_SEARCH_URL=localhost:8108` in `.env.local`.

## Filter syntax

Vectoria-algolia understands the subset of Algolia's filter syntax used by `RefinementList`, `NumericMenu`, and `RangeSlider`:

```
brand:Nike                         → {"brand": "Nike"}
in_stock:true                      → {"in_stock": true}
price >= 100                       → {"price_min": 100}
price < 200                        → {"price_max": 199}
brand:Nike AND price >= 50         → {"brand": "Nike", "price_min": 50}
```

`OR` and `NOT` are not parsed — filtered out silently.

## Non-standard extension: `searchMode`

Pass `"searchMode": "semantic"` or `"searchMode": "bm25"` in the query body to override Vectoria's default hybrid mode:

```json
{ "query": "running shoes", "hitsPerPage": 10, "searchMode": "semantic" }
```

## Switching to crates.io vectoria-core

While iterating locally the `Cargo.toml` uses a path dependency:

```toml
vectoria-core = { path = "../vectoria/vectoria-core" }
```

Switch to the published crate before deploying:

```toml
vectoria-core = "1.0.7"
```

## License

Apache-2.0
