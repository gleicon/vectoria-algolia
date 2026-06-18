import { liteClient } from 'algoliasearch/lite'
import {
  InstantSearch,
  SearchBox,
  Hits,
  RefinementList,
  Pagination,
  Stats,
  Configure,
  ClearRefinements,
  useInstantSearch,
} from 'react-instantsearch'
import { Hit } from './Hit'

// Vite dev server proxies /1/* → localhost:8108 so we can use the same origin.
// In production (Docker), STATIC_DIR serves this build from the search server itself.
const searchClient = liteClient('local', 'local', {
  hosts: [{ url: window.location.host, protocol: window.location.protocol.replace(':', '') as 'http' | 'https' }],
})

function EmptyState() {
  const { results } = useInstantSearch()
  if (!results || results.nbHits > 0) return null
  return (
    <div className="col-span-full flex flex-col items-center py-20 text-zinc-400 gap-3">
      <span className="text-5xl">🔍</span>
      <p className="text-sm">No results for <strong className="text-zinc-600">"{results.query}"</strong></p>
      <p className="text-xs">Try a different search or clear the filters.</p>
    </div>
  )
}

function SidebarSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <h2 className="text-xs font-semibold uppercase tracking-widest text-zinc-400 mb-3">{title}</h2>
      {children}
    </div>
  )
}

export default function App() {
  return (
    <InstantSearch searchClient={searchClient} indexName="products" insights={false}>
      <Configure hitsPerPage={12} />

      {/* Header */}
      <header className="bg-white border-b border-zinc-200 sticky top-0 z-10">
        <div className="max-w-7xl mx-auto px-6 py-3 flex items-center gap-6">
          <span className="text-sm font-semibold text-zinc-900 whitespace-nowrap">
            Vectoria <span className="text-zinc-400 font-normal">demo</span>
          </span>
          <div className="flex-1 max-w-2xl">
            <SearchBox
              placeholder="Search products — running shoes, headphones, yoga mats…"
              autoFocus
            />
          </div>
          <Stats />
        </div>
      </header>

      {/* Layout */}
      <div className="max-w-7xl mx-auto px-6 py-6 flex gap-8">

        {/* Sidebar */}
        <aside className="w-56 shrink-0 flex flex-col gap-6">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-zinc-500 uppercase tracking-widest">Filters</span>
            <ClearRefinements
              translations={{ resetButtonText: 'Clear all' }}
            />
          </div>

          <SidebarSection title="Category">
            <RefinementList
              attribute="category"
              sortBy={['count:desc', 'name:asc']}
              limit={8}
              showMore
              showMoreLimit={20}
            />
          </SidebarSection>

          <SidebarSection title="Brand">
            <RefinementList
              attribute="brand"
              sortBy={['count:desc', 'name:asc']}
              limit={8}
              showMore
              showMoreLimit={30}
            />
          </SidebarSection>

          <SidebarSection title="Availability">
            <RefinementList attribute="in_stock" />
          </SidebarSection>
        </aside>

        {/* Results */}
        <main className="flex-1 min-w-0">
          <Hits
            hitComponent={Hit}
            classNames={{
              list: 'grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4',
              item: '',
            }}
          />
          <EmptyState />
          <div className="mt-8 flex justify-center">
            <Pagination padding={2} />
          </div>
        </main>
      </div>

      {/* Footer */}
      <footer className="mt-12 pb-8 text-center text-xs text-zinc-400 font-mono">
        powered by{' '}
        <a href="https://github.com/gleicon/vectoria" className="underline hover:text-zinc-600">
          vectoria
        </a>
        {' '}· Algolia InstantSearch compatible
      </footer>
    </InstantSearch>
  )
}
