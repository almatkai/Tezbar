// @vitest-environment jsdom
import { act, createElement, useState } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { VaultIconPicker } from './VaultIconPicker'

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })

describe('Vault icon picker', () => {
  let host: HTMLDivElement
  let root: Root
  let changed: ReturnType<typeof vi.fn>
  let parentKey: ReturnType<typeof vi.fn>

  const chip = (): HTMLButtonElement => host.querySelector('button')!
  const dialog = (): HTMLDivElement | null => document.querySelector('[role="dialog"]')
  const cell = (id: string): HTMLButtonElement => document.querySelector(`[data-icon-id="${id}"]`)!
  const input = (): HTMLInputElement => document.querySelector('input')!
  const click = (element: HTMLElement): void => act(() => element.click())
  const key = (element: Element, value: string): void =>
    act(() => {
      element.dispatchEvent(
        new KeyboardEvent('keydown', { key: value, bubbles: true, cancelable: true })
      )
    })
  const filter = (value: string): void =>
    act(() => {
      // Use the native setter so React sees the input event as a user change.
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(
        input(),
        value
      )
      input().dispatchEvent(new Event('input', { bubbles: true }))
    })

  beforeEach(() => {
    changed = vi.fn()
    parentKey = vi.fn()
    host = document.createElement('div')
    document.body.append(host)
    root = createRoot(host)
    function Editor(): JSX.Element {
      const [icon, setIcon] = useState<string | undefined>()
      return createElement(
        'div',
        { onKeyDown: parentKey },
        createElement(VaultIconPicker, {
          kind: 'password',
          icon,
          onChange: (next) => {
            changed(next)
            setIcon(next)
          },
        })
      )
    }
    act(() => root.render(createElement(Editor)))
  })

  afterEach(() => {
    act(() => root.unmount())
    host.remove()
    vi.restoreAllMocks()
  })

  it('opens with the kind default selected and focuses the filter', () => {
    click(chip())
    expect(dialog()).not.toBeNull()
    expect(document.activeElement).toBe(input())
    expect(cell('password').getAttribute('aria-pressed')).toBe('true')
    expect(chip().getAttribute('aria-expanded')).toBe('true')
  })

  it('navigates the grid with arrows, selects with Enter, and returns focus', () => {
    click(chip())
    key(input(), 'ArrowDown')
    expect(document.activeElement).toBe(cell('password'))
    key(cell('password'), 'ArrowRight')
    expect(document.activeElement).toBe(cell('note'))
    key(cell('note'), 'ArrowDown')
    expect(document.activeElement).toBe(cell('wifi'))
    key(cell('wifi'), 'ArrowLeft')
    expect(document.activeElement).toBe(cell('wallet'))
    key(cell('wallet'), 'ArrowUp')
    expect(document.activeElement).toBe(cell('password'))
    key(cell('password'), 'ArrowLeft')
    expect(document.activeElement).toBe(cell('server'))
    key(cell('server'), 'Enter')
    expect(changed).toHaveBeenCalledWith('server')
    expect(dialog()).toBeNull()
    expect(document.activeElement).toBe(chip())
    expect(parentKey).not.toHaveBeenCalled()
    click(chip())
    expect(cell('server').getAttribute('aria-pressed')).toBe('true')
  })

  it('filters by labels and keywords, and handles an empty grid', () => {
    click(chip())
    filter('cloud')
    expect(document.querySelectorAll('[data-icon-id]')).toHaveLength(1)
    expect(cell('cloud')).not.toBeNull()
    filter('biometric')
    expect(document.querySelectorAll('[data-icon-id]')).toHaveLength(1)
    key(input(), 'ArrowDown')
    expect(document.activeElement).toBe(cell('fingerprint'))
    key(cell('fingerprint'), 'ArrowUp')
    expect(document.activeElement).toBe(input())
    filter('no matching subject')
    expect(document.querySelectorAll('[data-icon-id]')).toHaveLength(0)
    key(input(), 'ArrowDown')
    expect(document.activeElement).toBe(input())
  })

  it('consumes Escape before the editor and App, then restores focus to the chip', () => {
    const appEscape = vi.fn()
    window.addEventListener('keydown', appEscape)
    try {
      click(chip())
      key(input(), 'Escape')
      expect(dialog()).toBeNull()
      expect(document.activeElement).toBe(chip())
      expect(changed).not.toHaveBeenCalled()
      expect(parentKey).not.toHaveBeenCalled()
      expect(appEscape).not.toHaveBeenCalled()
      key(chip(), 'Escape')
      expect(parentKey).toHaveBeenCalledTimes(1)
      expect(appEscape).toHaveBeenCalledTimes(1)
    } finally {
      window.removeEventListener('keydown', appEscape)
    }
  })

  it('clears a custom selection with Use default and closes on an outside click', () => {
    click(chip())
    click(cell('cloud'))
    click(chip())
    const reset = [...dialog()!.querySelectorAll('button')].find(
      (button) => button.textContent === 'Use default'
    )!
    click(reset)
    expect(changed).toHaveBeenLastCalledWith(undefined)
    expect(document.activeElement).toBe(chip())
    click(chip())
    expect(cell('password').getAttribute('aria-pressed')).toBe('true')
    act(() => document.body.dispatchEvent(new Event('pointerdown', { bubbles: true })))
    expect(dialog()).toBeNull()
    expect(document.activeElement).toBe(chip())
  })
})
