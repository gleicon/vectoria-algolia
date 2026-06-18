interface HitDoc {
  objectID: string
  _score: number
  title?: string
  name?: string
  brand?: string
  category?: string
  price?: number
  in_stock?: boolean
  rating?: number
  description?: string
  [key: string]: unknown
}

interface HitProps {
  hit: HitDoc
}

const stars = (r: number) => {
  const full = Math.floor(r)
  const half = r - full >= 0.5
  return '★'.repeat(full) + (half ? '½' : '') + '☆'.repeat(5 - full - (half ? 1 : 0))
}

export function Hit({ hit }: HitProps) {
  const title = hit.title ?? hit.name ?? hit.objectID
  const inStock = hit.in_stock !== false

  return (
    <article className="bg-white border border-zinc-200 rounded-lg p-4 flex flex-col gap-2 hover:border-zinc-400 transition-colors">
      <div className="flex items-start justify-between gap-2">
        <h3 className="text-sm font-semibold text-zinc-900 leading-snug">{title}</h3>
        {!inStock && (
          <span className="shrink-0 text-xs px-2 py-0.5 rounded-full bg-red-50 text-red-600 border border-red-200">
            Out of stock
          </span>
        )}
      </div>

      <div className="flex items-center gap-2 text-xs text-zinc-500">
        {hit.brand && (
          <span className="bg-zinc-100 px-2 py-0.5 rounded font-medium text-zinc-700">
            {hit.brand}
          </span>
        )}
        {hit.category && (
          <span className="text-zinc-400">{hit.category}</span>
        )}
      </div>

      {hit.description && (
        <p className="text-xs text-zinc-500 leading-relaxed line-clamp-2">
          {hit.description}
        </p>
      )}

      <div className="flex items-center justify-between mt-auto pt-1">
        <span className="text-base font-bold text-zinc-900">
          {hit.price != null ? `$${hit.price.toLocaleString()}` : ''}
        </span>
        {hit.rating != null && (
          <span className="text-xs text-amber-500 font-mono" title={`${hit.rating}/5`}>
            {stars(hit.rating)} <span className="text-zinc-400">{hit.rating}</span>
          </span>
        )}
      </div>
    </article>
  )
}
