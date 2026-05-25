import { useMemo, useState } from 'react'
import type { ExtensionRuntimeNode } from '../../../shared/extensionRuntime'

type GridItem = {
  id: string
  title: string
  subtitle: string
  image?: string
  actionIds: string[]
}

function imageSourceFromContent(content: unknown): string | undefined {
  if (!content || typeof content !== 'object') return undefined
  const value = (content as { value?: unknown }).value
  const source =
    value && typeof value === 'object'
      ? (value as { source?: unknown }).source
      : (content as { source?: unknown }).source

  if (typeof source === 'string') return source
  if (source && typeof source === 'object') {
    const themed = source as { dark?: unknown; light?: unknown }
    if (typeof themed.dark === 'string') return themed.dark
    if (typeof themed.light === 'string') return themed.light
  }
  return undefined
}

function collectGridItems(root: ExtensionRuntimeNode): GridItem[] {
  const out: GridItem[] = []

  const walk = (node: ExtensionRuntimeNode): void => {
    if (node.type === 'Grid.Item') {
      const id = typeof node.props?.id === 'string' ? node.props.id : `grid:${out.length}`
      const title = typeof node.props?.title === 'string' ? node.props.title : 'Untitled'
      const subtitle = typeof node.props?.subtitle === 'string' ? node.props.subtitle : ''
      const image = imageSourceFromContent(node.props?.content)
      const actionIds = Array.isArray(node.props?.actionIds)
        ? node.props.actionIds.filter((value): value is string => typeof value === 'string')
        : []
      out.push({ id, title, subtitle, image, actionIds })
      return
    }

    for (const child of node.children ?? []) {
      walk(child)
    }
  }

  walk(root)
  return out
}

export function GridRuntime({
  root,
  title,
  onBack,
  onRunPrimaryAction,
  onOpenActions,
}: {
  root: ExtensionRuntimeNode
  title: string
  onBack: () => void
  onRunPrimaryAction: (actionId?: string) => void
  onOpenActions: (actionIds?: string[]) => void
}): JSX.Element {
  const items = useMemo(() => collectGridItems(root), [root])
  const [selected, setSelected] = useState(0)
  const columnCount = typeof root.props?.columns === 'number'
    ? Math.max(2, Math.min(8, Math.round(root.props.columns)))
    : 4

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="glass-card mb-2 shrink-0 px-3 py-2">
        <div className="flex items-center gap-2">
          <button type="button" className="btn btn-ghost" onClick={onBack}>
            Back
          </button>
          <div className="text-[12px] font-semibold text-ink-2">{title}</div>
          <div className="ml-auto flex items-center gap-1">
            <button type="button" className="btn btn-ghost" onClick={() => onRunPrimaryAction()}>
              Enter
            </button>
            <button type="button" className="btn btn-ghost" onClick={() => onOpenActions()}>
              Cmd+K
            </button>
          </div>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1 py-2">
        {items.length === 0 ? (
          <div className="flex h-full items-center justify-center text-[12px] text-ink-3">
            No grid items
          </div>
        ) : (
          <div
            className="grid gap-3"
            style={{ gridTemplateColumns: `repeat(${columnCount}, minmax(0, 1fr))` }}
          >
            {items.map((item, index) => (
              <button
                key={item.id}
                type="button"
                onMouseEnter={() => setSelected(index)}
                onClick={() => onRunPrimaryAction(item.actionIds[0])}
                onContextMenu={(event) => {
                  event.preventDefault()
                  setSelected(index)
                  onOpenActions(item.actionIds)
                }}
                className={`group min-h-[132px] rounded-[10px] border p-3 text-left transition ${
                  selected === index
                    ? 'border-accent-1/55 bg-white/[0.075] text-ink-1 shadow-[0_0_0_1px_rgba(139,116,255,0.18)]'
                    : 'border-white/[0.075] bg-transparent text-ink-2 hover:border-white/16 hover:bg-white/[0.045]'
                }`}
              >
                <div className="mx-auto mb-3 grid aspect-square w-full max-w-[88px] place-items-center rounded-[8px] bg-[#242936] p-4 transition group-hover:bg-[#2b3040]">
                  {item.image ? (
                    <img
                      src={item.image}
                      alt=""
                      className="h-full w-full object-contain"
                      loading="lazy"
                    />
                  ) : (
                    <span className="text-[20px] font-semibold text-ink-4">
                      {item.title.slice(0, 1).toUpperCase()}
                    </span>
                  )}
                </div>
                <p className="truncate text-[12px] font-semibold">{item.title}</p>
                {item.subtitle ? <p className="truncate text-[11px] text-ink-4">{item.subtitle}</p> : null}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
