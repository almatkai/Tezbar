import { useEffect, useState, type KeyboardEvent, type ReactNode } from 'react'
import { Button, TextField } from '../ui/primitives'
import { extensionRequest as request } from './api'
import type { ComposerItem } from '../../shared/aiExtensions'
export function useComposerExtensions(
  draft: string,
  setDraft: (text: string) => void,
  cwd: string | undefined,
  onProfile: (id: string) => void
): {
  menu: ReactNode
  onKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => boolean
  activeDescendant?: string
} {
  const [items, setItems] = useState<ComposerItem[]>([])
  const [index, setIndex] = useState(0)
  const [dismissed, setDismissed] = useState(false)
  const [prompt, setPrompt] = useState<ComposerItem | null>(null)
  const [arguments_, setArguments] = useState<Record<string, string>>({})
  const [error, setError] = useState('')
  useEffect(() => {
    if (typeof window.tezbar.aiExtensions === 'function')
      void request<ComposerItem[]>('composer-items', cwd)
        .then(setItems)
        .catch(() => {})
  }, [cwd])
  useEffect(() => {
    setIndex(0)
    setDismissed(false)
  }, [draft])
  const filtered =
    !dismissed && /^\/[\w-]*$/.test(draft)
      ? items.filter((i) => (i.name || '').toLowerCase().startsWith(draft.slice(1).toLowerCase()))
      : []
  const resolve = async (item: ComposerItem, args: Record<string, string>): Promise<void> => {
    try {
      const result = await request<{ text: string }>(
        item.kind === 'resources' ? 'read-resource' : 'prompt-resolve',
        item,
        args
      )
      setDraft(result.text)
      setPrompt(null)
      setError('')
      document.getElementById('ai-chat-input')?.focus()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }
  const choose = (item: ComposerItem): void => {
    if (item.kind === 'agents') {
      onProfile(item.id || item.name)
      setDraft('')
      return
    }
    if (item.arguments?.length) {
      setPrompt(item)
      setArguments({})
      setDismissed(true)
      return
    }
    void resolve(item, {})
  }
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!filtered.length) return false
    if (event.key === 'Escape') {
      event.preventDefault()
      setDismissed(true)
      return true
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      setIndex((i) => (i + (event.key === 'ArrowDown' ? 1 : filtered.length - 1)) % filtered.length)
      return true
    }
    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault()
      choose(filtered[index] || filtered[0]!)
      return true
    }
    return false
  }
  return {
    onKeyDown,
    activeDescendant: filtered.length ? `ai-command-${index}` : undefined,
    menu: (
      <>
        {!!filtered.length && (
          <div
            role="listbox"
            aria-label="AI commands"
            className="mb-2 max-h-60 overflow-auto rounded-lg border border-ink-4/20 py-1 text-[12px]"
          >
            {filtered.map((item, i) => (
              <button
                type="button"
                role="option"
                id={`ai-command-${i}`}
                key={`${item.extensionId}:${item.connectionId}:${item.kind}:${item.name}`}
                aria-selected={i === index}
                className={`block w-full px-3 py-1.5 text-left ${i === index ? 'bg-white/10 text-ink-1' : 'text-ink-2'}`}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => choose(item)}
              >
                <span className="mr-3 text-ink-3">{item.extensionName}</span>/{item.name}{' '}
                {item.title}
              </button>
            ))}
          </div>
        )}
        {prompt && (
          <div className="mb-2 space-y-2 text-[12px]">
            {prompt.arguments?.map((arg, i, all) => (
              <label className="block" key={arg.name}>
                {arg.name}
                {arg.required ? ' *' : ''}
                <TextField
                  autoFocus={i === 0}
                  value={arguments_[arg.name] || ''}
                  onChange={(e) => setArguments((a) => ({ ...a, [arg.name]: e.target.value }))}
                  onKeyDown={(e) => {
                    if (e.key !== 'Enter') return
                    e.preventDefault()
                    const next = e.currentTarget
                      .closest('label')
                      ?.nextElementSibling?.querySelector('input')
                    if (i + 1 < all.length && next instanceof HTMLInputElement) next.focus()
                    else void resolve(prompt, arguments_)
                  }}
                />
              </label>
            ))}
            <Button onClick={() => void resolve(prompt, arguments_)}>Insert in draft</Button>
            <Button variant="quiet" onClick={() => setPrompt(null)}>
              Cancel
            </Button>
            {error && <p role="alert">{error}</p>}
          </div>
        )}
      </>
    ),
  }
}
