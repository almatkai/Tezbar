import { useEffect, useState } from 'react'

type RollingTextItem = {
  shortcut: string
  label: string
}

export function RollingText({ items }: { items: readonly RollingTextItem[] }): JSX.Element | null {
  const [index, setIndex] = useState(0)

  useEffect(() => {
    if (items.length < 2) return

    const timer = window.setInterval(() => {
      setIndex((current) => (current + 1) % items.length)
    }, 3200)

    return () => window.clearInterval(timer)
  }, [items.length])

  const item = items[index]
  if (!item) return null

  return (
    <span className="rolling-command-hint select-none" aria-hidden>
      <span key={`${index}:${item.shortcut}`} className="rolling-command-hint__item select-none">
        <span className="rolling-command-hint__shortcut select-none">{item.shortcut}</span>
        <span className="rolling-command-hint__label select-none">{item.label}</span>
      </span>
    </span>
  )
}
