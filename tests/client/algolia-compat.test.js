/**
 * Algolia-compatibility tests using the real algoliasearch v5 liteClient.
 *
 * Verifies that vectoria-algolia responds correctly to every request shape
 * InstantSearch generates — so any developer who just changes hosts will get
 * the same experience they'd expect from Algolia.
 *
 * Run against a live server:
 *   SERVER_URL=http://localhost:8108 npx vitest run
 */

import { describe, it, expect, beforeAll } from 'vitest'
import { liteClient } from 'algoliasearch/lite'

const SERVER = process.env.SERVER_URL ?? 'http://localhost:8108'
const INDEX  = 'products'

const url  = new URL(SERVER)
const host = url.hostname + (url.port ? `:${url.port}` : '')

const client = liteClient('local', 'local', {
  hosts: [{ url: host, protocol: url.protocol.replace(':', ''), accept: 'readWrite' }],
})

/** Single-index search via the liteClient multi-search endpoint. */
async function search(params) {
  const res = await client.search([{ indexName: INDEX, params }])
  return res.results[0]
}

// ── Seed data ─────────────────────────────────────────────────────────────────

const SEED = [
  { objectID: 'c-shoe1', title: 'Nike Air Max Running Shoe', brand: 'Nike',     category: 'Footwear',    price: 120, in_stock: true,  description: 'Lightweight running shoe with Air cushioning' },
  { objectID: 'c-shoe2', title: 'Adidas Ultraboost Trail',   brand: 'Adidas',   category: 'Footwear',    price: 180, in_stock: true,  description: 'Trail running shoe with Boost midsole' },
  { objectID: 'c-mat1',  title: 'Yoga Mat Premium Non-Slip', brand: 'Manduka',  category: 'Fitness',     price: 78,  in_stock: true,  description: 'Non-slip yoga mat 6mm thick' },
  { objectID: 'c-hdp1',  title: 'Sony WH-1000XM5 Wireless', brand: 'Sony',     category: 'Electronics', price: 350, in_stock: false, description: 'Wireless noise-cancelling headphones' },
  { objectID: 'c-jkt1',  title: 'Patagonia Down Jacket',     brand: 'Patagonia',category: 'Clothing',    price: 299, in_stock: true,  description: 'Recycled down insulated jacket' },
]

beforeAll(async () => {
  const requests = SEED.map(obj => ({ action: 'addObject', body: obj }))
  await fetch(`${SERVER}/1/indexes/${INDEX}/batch`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ requests }),
  })
  // Small delay so the engine flushes writes
  await new Promise(r => setTimeout(r, 200))
})

// ── Basic search ──────────────────────────────────────────────────────────────

describe('single-index search', () => {
  it('returns hits array and nbHits', async () => {
    const res = await search({ query: 'running shoe', hitsPerPage: 5 })
    expect(Array.isArray(res.hits)).toBe(true)
    expect(res.nbHits).toBeGreaterThan(0)
  })

  it('every hit has objectID', async () => {
    const res = await search({ query: 'shoe', hitsPerPage: 5 })
    for (const hit of res.hits) {
      expect(hit.objectID).toBeTruthy()
    }
  })

  it('empty query returns all indexed documents', async () => {
    const res = await search({ query: '', hitsPerPage: 20 })
    expect(res.nbHits).toBeGreaterThanOrEqual(SEED.length)
  })

  it('hitsPerPage is respected', async () => {
    const res = await search({ query: '', hitsPerPage: 2 })
    expect(res.hits.length).toBeLessThanOrEqual(2)
    expect(res.hitsPerPage).toBe(2)
  })

  it('page fields are present', async () => {
    const res = await search({ query: '', hitsPerPage: 2, page: 0 })
    expect(typeof res.page).toBe('number')
    expect(typeof res.nbPages).toBe('number')
  })
})

// ── _highlightResult ──────────────────────────────────────────────────────────

describe('_highlightResult', () => {
  it('is present on every hit', async () => {
    const res = await search({ query: 'running', hitsPerPage: 5 })
    expect(res.hits.length).toBeGreaterThan(0)
    for (const hit of res.hits) {
      expect(hit._highlightResult).toBeDefined()
    }
  })

  it('title highlight has value / matchLevel / matchedWords', async () => {
    const res = await search({ query: 'running', hitsPerPage: 5 })
    for (const hit of res.hits) {
      const hl = hit._highlightResult?.title
      expect(hl?.value).toBeDefined()
      expect(hl?.matchLevel).toMatch(/^(none|partial|full)$/)
      expect(Array.isArray(hl?.matchedWords)).toBe(true)
    }
  })

  it('matched hit wraps token in AIS highlight tags', async () => {
    const res = await search({ query: 'Nike', hitsPerPage: 10 })
    const nikeHit = res.hits.find(h => String(h.brand ?? '').toLowerCase() === 'nike')
    expect(nikeHit).toBeDefined()
    const hl = nikeHit._highlightResult?.brand?.value ?? ''
    expect(hl).toContain('<ais-highlight-0000000000>')
    expect(hl).toContain('</ais-highlight-0000000000>')
  })

  it('empty query yields matchLevel none everywhere', async () => {
    const res = await search({ query: '', hitsPerPage: 5 })
    for (const hit of res.hits) {
      expect(hit._highlightResult?.title?.matchLevel).toBe('none')
    }
  })

  it('custom highlightPreTag and highlightPostTag are respected', async () => {
    const res = await search({
      query: 'Sony',
      hitsPerPage: 5,
      highlightPreTag: '<em>',
      highlightPostTag: '</em>',
    })
    const sonyHit = res.hits.find(h => String(h.brand ?? '').toLowerCase() === 'sony')
    expect(sonyHit).toBeDefined()
    const hl = sonyHit._highlightResult?.brand?.value ?? ''
    expect(hl).toContain('<em>')
    expect(hl).toContain('</em>')
  })
})

// ── Filters ───────────────────────────────────────────────────────────────────

describe('filters', () => {
  it('string filter restricts category', async () => {
    const res = await search({ query: '', hitsPerPage: 10, filters: 'category:Electronics' })
    for (const hit of res.hits) {
      expect(hit.category).toBe('Electronics')
    }
  })

  it('price range filter works', async () => {
    const res = await search({ query: '', hitsPerPage: 10, filters: 'price >= 100 AND price <= 200' })
    for (const hit of res.hits) {
      expect(Number(hit.price)).toBeGreaterThanOrEqual(100)
      expect(Number(hit.price)).toBeLessThanOrEqual(200)
    }
  })
})

// ── facetFilters (RefinementList) ─────────────────────────────────────────────

describe('facetFilters', () => {
  it('filters by category using nested array syntax', async () => {
    const res = await search({ query: '', hitsPerPage: 10, facetFilters: [['category:Footwear']] })
    expect(res.hits.length).toBeGreaterThan(0)
    for (const hit of res.hits) {
      expect(hit.category).toBe('Footwear')
    }
  })
})

// ── Facets ────────────────────────────────────────────────────────────────────

describe('facets aggregation', () => {
  it('returns facet counts when facets param is set', async () => {
    const res = await search({ query: '', hitsPerPage: 20, facets: ['category', 'brand'] })
    expect(res.facets).toBeDefined()
    expect(res.facets?.category).toBeDefined()
    expect(res.facets?.brand).toBeDefined()
  })

  it('facet counts are positive integers', async () => {
    const res = await search({ query: '', hitsPerPage: 20, facets: ['category'] })
    for (const [, count] of Object.entries(res.facets?.category ?? {})) {
      expect(Number(count)).toBeGreaterThan(0)
    }
  })
})

// ── Multi-search (POST /1/indexes/*/queries) ──────────────────────────────────

describe('multi-search', () => {
  it('returns one result per request', async () => {
    const res = await client.search([
      { indexName: INDEX, params: { query: 'shoe', hitsPerPage: 3 } },
      { indexName: INDEX, params: { query: 'yoga', hitsPerPage: 3 } },
    ])
    expect(res.results.length).toBe(2)
  })

  it('each result has hits and nbHits', async () => {
    const res = await client.search([
      { indexName: INDEX, params: { query: '', hitsPerPage: 5 } },
      { indexName: INDEX, params: { query: '', hitsPerPage: 5 } },
    ])
    for (const r of res.results) {
      expect(Array.isArray(r.hits)).toBe(true)
      expect(r.nbHits).toBeGreaterThanOrEqual(0)
    }
  })

  it('disjunctive faceting: unfiltered sub-query has ≥ hits than filtered', async () => {
    const res = await client.search([
      // main: filtered to Electronics
      { indexName: INDEX, params: { query: '', hitsPerPage: 10, facets: ['category'], facetFilters: [['category:Electronics']] } },
      // disjunctive: no filter — for sidebar counts
      { indexName: INDEX, params: { query: '', hitsPerPage: 10, facets: ['category'] } },
    ])
    expect(res.results[0].nbHits).toBeLessThanOrEqual(res.results[1].nbHits)
  })
})

// ── Pagination ────────────────────────────────────────────────────────────────

describe('pagination', () => {
  it('page 1 returns different hits than page 0', async () => {
    const p0 = await search({ query: '', hitsPerPage: 2, page: 0 })
    const p1 = await search({ query: '', hitsPerPage: 2, page: 1 })
    if (p0.nbPages >= 2) {
      const ids0 = p0.hits.map(h => h.objectID)
      const ids1 = p1.hits.map(h => h.objectID)
      expect(ids0.some(id => ids1.includes(id))).toBe(false)
    }
  })
})

// ── Clear refinements ─────────────────────────────────────────────────────────

describe('clear refinements', () => {
  it('unfiltered search returns more results than filtered search', async () => {
    const unfiltered = await search({ query: '', hitsPerPage: 50 })
    const filtered   = await search({ query: '', hitsPerPage: 50, filters: 'category:Footwear' })
    expect(unfiltered.nbHits).toBeGreaterThan(filtered.nbHits)
  })
})
