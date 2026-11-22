# Changelog

All notable changes. Follows [Keep a Changelog](https://keepachangelog.com/).

---

## [0.2.0] — 2026-11-22

### Added
- **`VECTORIA_STORAGE_PATH` env var**: set it to a directory path and the adapter uses `EdgeStoreStorage` instead of in-memory. Data survives container restarts without reimporting.
- **Migration path**: with `VECTORIA_STORAGE_PATH`, vectoria-algolia can be used as a persistent drop-in for Algolia, then later the same data directory can be opened read-only by a vectoria server for a zero-reimport migration to the full vectoria API.

### Fixed
- `SearchRequest` struct updated for vectoria-core compatibility: added missing `candidate_pool`, `cluster`, and `snippets` fields.

---

## [0.1.0] — 2026-07-01

Initial release. Algolia-compatible HTTP adapter backed by an embedded in-memory vectoria engine.
