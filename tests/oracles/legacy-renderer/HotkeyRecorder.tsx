import { useCallback, useEffect, useRef, useState } from 'react'
import { cx } from './ui/primitives'

/* =========================================================================
   Electron accelerator → display symbols
   ========================================================================= */

export function formatShortcutForDisplay(shortcut: string): string {
  const parts = shortcut.split('+').map((token) => {
    const value = token.trim()
    if (/^(command|cmd)$/i.test(value)) return '⌘'
    if (/^(control|ctrl)$/i.test(value)) return '⌃'
    if (/^(alt|option)$/i.test(value)) return '⌥'
    if (/^shift$/i.test(value)) return '⇧'
    if (/^(backspace|delete)$/i.test(value)) return '⌫'
    return value.length === 1 ? value.toUpperCase() : value
  })

  const modifierSymbols = new Set(['⌘', '⌃', '⌥', '⇧'])
  const modifiers: string[] = []
  const keys: string[] = []
  for (const part of parts) {
    if (modifierSymbols.has(part)) modifiers.push(part)
    else if (part) keys.push(part)
  }

  const modifierStr = modifiers.join('')
  const keyStr = keys.join('+')
  if (modifierStr && keyStr) return modifierStr + keyStr
  return modifierStr || keyStr
}

/* =========================================================================
   Key mapping helpers
   ========================================================================= */

function mapCodeToAcceleratorToken(code: string): string | null {
  if (!code) return null
  if (code.startsWith('Key') && code.length === 4) return code.slice(3).toUpperCase()
  if (code.startsWith('Digit') && code.length === 6) return code.slice(5)
  if (code.startsWith('Numpad') && code.length > 6) return code
  if (/^F\d{1,2}$/i.test(code)) return code.toUpperCase()

  const codeMap: Record<string, string> = {
    Space: 'Space',
    Enter: 'Return',
    Tab: 'Tab',
    Escape: 'Escape',
    Backspace: 'Backspace',
    Delete: 'Delete',
    ArrowUp: 'Up',
    ArrowDown: 'Down',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    Minus: '-',
    Equal: '=',
    BracketLeft: '[',
    BracketRight: ']',
    Backslash: '\\',
    Semicolon: ';',
    Quote: "'",
    Backquote: '`',
    Comma: ',',
    Period: '.',
    Slash: '/',
  }
  return codeMap[code] || null
}

export function keyEventToAccelerator(e: KeyboardEvent): string | null {
  const parts: string[] = []
  if (e.metaKey) parts.push('Command')
  if (e.ctrlKey) parts.push('Control')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')

  const key = e.key
  if (['Meta', 'Control', 'Alt', 'Shift'].includes(key)) return null

  const keyMap: Record<string, string> = {
    ArrowUp: 'Up',
    ArrowDown: 'Down',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    ' ': 'Space',
    Enter: 'Return',
    Backspace: 'Backspace',
    Delete: 'Delete',
    Tab: 'Tab',
    Escape: 'Escape',
    F1: 'F1',
    F2: 'F2',
    F3: 'F3',
    F4: 'F4',
    F5: 'F5',
    F6: 'F6',
    F7: 'F7',
    F8: 'F8',
    F9: 'F9',
    F10: 'F10',
    F11: 'F11',
    F12: 'F12',
  }

  const mappedKey =
    mapCodeToAcceleratorToken(e.code) ||
    keyMap[key] ||
    (key.length === 1 ? key.toUpperCase() : key)

  const allowWithoutModifier = /^F\d{1,2}$/i.test(mappedKey)
  if (parts.length === 0 && !allowWithoutModifier) return null

  parts.push(mappedKey)
  return parts.join('+')
}

/* =========================================================================
   HotkeyRecorder component
   ========================================================================= */

interface HotkeyRecorderProps {
  value: string
  onChange: (hotkey: string) => void
  compact?: boolean
}

export function HotkeyRecorder({ value, onChange, compact }: HotkeyRecorderProps): JSX.Element {
  const [recording, setRecording] = useState(false)
  const [heldModifiers, setHeldModifiers] = useState<string[]>([])
  const containerRef = useRef<HTMLButtonElement>(null)

  const startRecording = useCallback(() => {
    setRecording(true)
    setHeldModifiers([])
  }, [])

  const stopRecording = useCallback(() => {
    setRecording(false)
    setHeldModifiers([])
  }, [])

  /* --- window-level key listeners while recording --- */
  useEffect(() => {
    if (!recording) return

    function handleKeyDown(e: KeyboardEvent) {
      e.preventDefault()
      e.stopPropagation()

      // Escape cancels recording
      if (e.key === 'Escape') {
        stopRecording()
        return
      }

      // Backspace / Delete with no modifiers clears the value
      if ((e.key === 'Backspace' || e.key === 'Delete') && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        onChange('')
        stopRecording()
        return
      }

      // Update held-modifier preview
      const mods: string[] = []
      if (e.metaKey) mods.push('⌘')
      if (e.ctrlKey) mods.push('⌃')
      if (e.altKey) mods.push('⌥')
      if (e.shiftKey) mods.push('⇧')
      setHeldModifiers(mods)

      // Try to produce an accelerator
      const accelerator = keyEventToAccelerator(e)
      if (accelerator) {
        onChange(accelerator)
        stopRecording()
      }
    }

    function handleKeyUp(e: KeyboardEvent) {
      e.preventDefault()
      e.stopPropagation()

      const mods: string[] = []
      if (e.metaKey) mods.push('⌘')
      if (e.ctrlKey) mods.push('⌃')
      if (e.altKey) mods.push('⌥')
      if (e.shiftKey) mods.push('⇧')
      setHeldModifiers(mods)
    }

    // Clicking outside cancels recording
    function handleMouseDown(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        stopRecording()
      }
    }

    window.addEventListener('keydown', handleKeyDown, true)
    window.addEventListener('keyup', handleKeyUp, true)
    window.addEventListener('mousedown', handleMouseDown, true)
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true)
      window.removeEventListener('keyup', handleKeyUp, true)
      window.removeEventListener('mousedown', handleMouseDown, true)
    }
  }, [recording, onChange, stopRecording])

  // Blur detection — stop recording when window loses focus
  useEffect(() => {
    if (!recording) return
    function handleBlur() {
      stopRecording()
    }
    window.addEventListener('blur', handleBlur)
    return () => window.removeEventListener('blur', handleBlur)
  }, [recording, stopRecording])

  const handleClear = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation()
      onChange('')
    },
    [onChange],
  )

  /* --- display text --- */
  const displayText = recording
    ? heldModifiers.length > 0
      ? heldModifiers.join('') + '…'
      : 'Type shortcut…'
    : value
      ? formatShortcutForDisplay(value)
      : 'None'

  const height = compact ? 'h-7' : 'h-8'
  const textSize = compact ? 'text-[11px]' : 'text-[12px]'

  return (
    <div className="relative inline-flex items-center">
      <button
        ref={containerRef}
        type="button"
        onClick={() => (recording ? stopRecording() : startRecording())}
        className={cx(
          'inline-flex items-center gap-1.5 rounded-md px-2.5 transition-all',
          height,
          textSize,
          'font-medium tracking-tight select-none',
          'focus:outline-none focus-visible:ring-1 focus-visible:ring-accent-1/60',
          recording &&
            'bg-blue-500/20 border border-blue-500/40 text-blue-400',
          !recording &&
            value &&
            'bg-white/[0.04] border border-white/10 text-ink-2 hover:border-white/20',
          !recording &&
            !value &&
            'bg-white/[0.04] border border-white/[0.06] text-ink-4 hover:text-ink-3 hover:border-white/10',
        )}
      >
        {/* Keyboard icon */}
        <svg
          width={compact ? 12 : 14}
          height={compact ? 12 : 14}
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={cx(
            'shrink-0',
            recording ? 'text-blue-400' : value ? 'text-ink-3' : 'text-ink-4',
          )}
          aria-hidden
        >
          <rect x="2" y="4" width="20" height="16" rx="2" />
          <path d="M6 8h.01M10 8h.01M14 8h.01M18 8h.01M6 12h.01M10 12h.01M14 12h.01M18 12h.01M8 16h8" />
        </svg>

        <span className={cx(recording && 'animate-pulse')}>{displayText}</span>
      </button>

      {/* Clear button — only when value is set and not recording */}
      {value && !recording && (
        <button
          type="button"
          onClick={handleClear}
          aria-label="Clear shortcut"
          className="ml-1 flex h-5 w-5 shrink-0 items-center justify-center rounded text-ink-4 transition hover:bg-white/[0.08] hover:text-ink-2"
        >
          <svg
            width="10"
            height="10"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden
          >
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      )}
    </div>
  )
}

export default HotkeyRecorder
