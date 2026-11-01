// @vitest-environment jsdom
import { act, createElement } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  FormRuntime,
  reconcileFormValues,
  reconcileFormWheelDraft,
  shouldSubmitFormOnKey,
} from './form-runtime'
import type { ExtensionRuntimeNode } from '../../../shared/extensionRuntime'

type Fields = Parameters<typeof reconcileFormValues>[0]
const timerFields: Fields = [
  { key: 'seconds:Form.TextField', id: 'seconds', type: 'Form.TextField', title: 'Seconds', value: '' },
  { key: 'name:Form.TextField', id: 'name', type: 'Form.TextField', title: 'Name', value: '' },
]

function field(id: string, value?: unknown, type = 'Form.TextField') {
  return { key: `${id}:${type}`, id, title: id, type, value }
}

describe('form draft reconciliation', () => {
  it('keeps typed timer fields across repeated unchanged server refreshes', () => {
    let draft: Record<string, unknown> = { seconds: '5', name: 'Native V2 QA' }
    for (let refresh = 0; refresh < 3; refresh += 1) {
      draft = reconcileFormValues(structuredClone(timerFields), draft, timerFields)
    }
    expect(draft).toEqual({ seconds: '5', name: 'Native V2 QA' })
  })

  it('applies changed server values while retaining other local edits', () => {
    const updated = structuredClone(timerFields)
    updated[0]!.value = '10'
    expect(reconcileFormValues(updated, { seconds: '5', name: 'Draft' }, timerFields))
      .toEqual({ seconds: '10', name: 'Draft' })
  })

  it('keeps controlled field updates authoritative', () => {
    const fields = [{ ...timerFields[0]!, actionId: 'change-seconds' }]
    expect(reconcileFormValues(fields, { seconds: '5' }, fields)).toEqual({ seconds: '' })
  })

  it('initializes new fields and drops removed fields', () => {
    const fields = [{ ...timerFields[0]!, id: 'minutes', key: 'minutes:Form.TextField', value: '2' }]
    expect(reconcileFormValues(fields, { seconds: '5' }, timerFields)).toEqual({ minutes: '2' })
  })

  it('applies an acknowledgement and a later server reset', () => {
    const acknowledged = [field('seconds', '5')]
    const values = reconcileFormValues(acknowledged, { seconds: '5' }, timerFields)
    expect(values).toEqual({ seconds: '5' })
    expect(reconcileFormValues([field('seconds', '')], values, acknowledged))
      .toEqual({ seconds: '' })
  })

  it('preserves uncontrolled drafts but initializes changed field types and missing values', () => {
    const previous = [field('keep'), field('changed', '')]
    expect(reconcileFormValues([
      field('keep'), field('changed', undefined, 'Form.Checkbox'),
      field('tags', undefined, 'Form.TagPicker'), field('files', undefined, 'Form.FilePicker'),
      field('text'),
    ], { keep: 'draft', changed: 'old text' }, previous)).toEqual({
      keep: 'draft', changed: false, tags: [], files: [], text: '',
    })
  })

  it('compares rebuilt server arrays by value and accepts changed selections', () => {
    const previous = [field('tags', ['server'], 'Form.TagPicker')]
    const values = { tags: ['draft'] }
    expect(reconcileFormValues([field('tags', ['server'], 'Form.TagPicker')], values, previous))
      .toEqual(values)
    expect(reconcileFormValues([field('tags', [], 'Form.TagPicker')], values, previous))
      .toEqual({ tags: [] })
  })

  it('keeps pending wheel gestures until all coordinates are acknowledged, then accepts server changes', () => {
    const local = { hue: 120, saturation: 75, value: 60, alpha: 85 }
    const coordinates = (values: typeof local): Fields => Object.entries(values)
      .map(([id, value]) => field(id, String(value)))
    const stale = coordinates({ ...local, alpha: 80 })
    const acknowledged = coordinates(local)

    expect(reconcileFormWheelDraft(stale, local)).toBe(local)
    expect(reconcileFormWheelDraft(acknowledged, local)).toBeNull()
    expect(reconcileFormWheelDraft(stale, null)).toBeNull()
    expect(reconcileFormValues(coordinates({ ...local, hue: 180 }), local, acknowledged))
      .toEqual({ hue: '180', saturation: 75, value: 60, alpha: 85 })
  })
})

const defaultKey = {
  key: 'Enter',
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  repeat: false,
  isComposing: false,
  inTextArea: false,
  isMac: true,
}

describe('form submit keyboard decision', () => {
  it.each([
    ['plain input Enter', {}, true],
    ['input Shift Enter', { shiftKey: true }, true],
    ['textarea Enter', { inTextArea: true }, false],
    ['textarea Shift Enter', { inTextArea: true, shiftKey: true }, false],
    ['Mac input Command Enter', { metaKey: true }, true],
    ['Mac textarea Command Enter', { metaKey: true, inTextArea: true }, true],
    ['Mac textarea Command Shift Enter', { metaKey: true, shiftKey: true, inTextArea: true }, true],
    ['Mac Control Enter', { ctrlKey: true }, false],
    ['Mac textarea Control Enter', { ctrlKey: true, inTextArea: true }, false],
    ['off-Mac input Enter', { isMac: false }, true],
    ['off-Mac textarea Enter', { isMac: false, inTextArea: true }, false],
    ['off-Mac textarea Shift Enter', { isMac: false, inTextArea: true, shiftKey: true }, false],
    ['off-Mac Control Enter', { isMac: false, ctrlKey: true }, true],
    ['off-Mac textarea Control Enter', { isMac: false, ctrlKey: true, inTextArea: true }, true],
    [
      'off-Mac textarea Control Shift Enter',
      { isMac: false, ctrlKey: true, shiftKey: true, inTextArea: true },
      true,
    ],
    ['off-Mac Command Enter', { isMac: false, metaKey: true }, false],
    ['off-Mac textarea Command Enter', { isMac: false, metaKey: true, inTextArea: true }, false],
    ['Alt Enter', { altKey: true }, false],
    ['Mac Command Alt Enter', { metaKey: true, altKey: true, inTextArea: true }, false],
    [
      'off-Mac Control Alt Enter',
      { isMac: false, ctrlKey: true, altKey: true, inTextArea: true },
      false,
    ],
    ['Mac both modifiers', { metaKey: true, ctrlKey: true }, false],
    ['off-Mac both modifiers', { isMac: false, metaKey: true, ctrlKey: true }, false],
    ['Command K', { key: 'k', metaKey: true }, false],
    ['other key', { key: 'Escape' }, false],
  ] as const)('%s', (_name, event, expected) => {
    expect(shouldSubmitFormOnKey({ ...defaultKey, ...event })).toBe(expected)
  })

  it('ignores repeat and IME for every platform, target and modifier combination', () => {
    for (const isMac of [true, false]) {
      for (const inTextArea of [true, false]) {
        for (let modifiers = 0; modifiers < 16; modifiers += 1) {
          const event = {
            ...defaultKey,
            isMac,
            inTextArea,
            metaKey: Boolean(modifiers & 1),
            ctrlKey: Boolean(modifiers & 2),
            altKey: Boolean(modifiers & 4),
            shiftKey: Boolean(modifiers & 8),
          }
          expect(shouldSubmitFormOnKey({ ...event, repeat: true })).toBe(false)
          expect(shouldSubmitFormOnKey({ ...event, isComposing: true })).toBe(false)
        }
      }
    }
  })
})

const promptRoot: ExtensionRuntimeNode = {
  type: 'Form',
  children: [
    {
      type: 'Form.TextArea',
      props: { id: 'prompt', title: 'Ask Anything', value: 'Existing draft' },
    },
    { type: 'Form.Separator' },
    { type: 'Form.Description', props: { title: 'Context', text: 'Extension description' } },
  ],
}

describe('form controls', () => {
  let container: HTMLDivElement
  let root: Root
  const onSubmitForm = vi.fn()
  const props = {
    root: promptRoot,
    title: 'Ask Perplexity',
    onBack: vi.fn(),
    onSubmitForm,
    onChangeField: vi.fn(),
    onInvokeAction: vi.fn(),
    onOpenActions: vi.fn(),
  }

  beforeEach(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
    onSubmitForm.mockClear()
    container = document.createElement('div')
    document.body.append(container)
    root = createRoot(container)
  })

  afterEach(() => {
    act(() => root.unmount())
    container.remove()
    vi.unstubAllGlobals()
  })

  it('focuses the composer with its caret at the end and keeps focus stable on refresh', () => {
    act(() => root.render(createElement(FormRuntime, props)))
    const textarea = container.querySelector('textarea')!
    expect(document.activeElement).toBe(textarea)
    expect(textarea.selectionStart).toBe('Existing draft'.length)
    expect(textarea.selectionEnd).toBe('Existing draft'.length)
    expect(textarea.placeholder).toBe('Ask Anything')
    expect(textarea.getAttribute('aria-label')).toBe('Ask Anything')
    expect(container.querySelector('label')).toBeNull()
    expect(container.querySelector('.glass-field')).toBeNull()
    expect(container.textContent).toContain('Extension description')
    textarea.setSelectionRange(2, 2)
    act(() =>
      root.render(createElement(FormRuntime, { ...props, root: structuredClone(promptRoot) }))
    )
    expect(textarea.selectionStart).toBe(2)
  })

  it('keeps textarea newlines native, prevents shortcut newlines and ignores IME/repeat', () => {
    act(() => root.render(createElement(FormRuntime, props)))
    const textarea = container.querySelector('textarea')!
    const shortcut = /Mac/i.test(navigator.platform || navigator.userAgent)
      ? { metaKey: true }
      : { ctrlKey: true }
    for (const options of [{}, { shiftKey: true }, { ...shortcut, isComposing: true }]) {
      const event = new KeyboardEvent('keydown', {
        key: 'Enter',
        bubbles: true,
        cancelable: true,
        ...options,
      })
      act(() => textarea.dispatchEvent(event))
      expect(event.defaultPrevented).toBe(false)
    }
    expect(onSubmitForm).not.toHaveBeenCalled()
    const submit = new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
      ...shortcut,
    })
    act(() => textarea.dispatchEvent(submit))
    expect(submit.defaultPrevented).toBe(true)
    expect(onSubmitForm).toHaveBeenCalledExactlyOnceWith({ prompt: 'Existing draft' })
    const repeat = new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
      repeat: true,
      ...shortcut,
    })
    act(() => textarea.dispatchEvent(repeat))
    expect(repeat.defaultPrevented).toBe(true)
    expect(onSubmitForm).toHaveBeenCalledTimes(1)
  })

  it('submits single-line inputs, selects and checkboxes; updates the multiline hint', () => {
    act(() =>
      root.render(
        createElement(FormRuntime, {
          ...props,
          root: {
            type: 'Form',
            children: [
              { type: 'Form.TextField', props: { id: 'name', title: 'Name' } },
              { type: 'Form.Dropdown', props: { id: 'choice', title: 'Choice' } },
              { type: 'Form.Checkbox', props: { id: 'enabled', title: 'Enabled' } },
            ],
          },
        })
      )
    )
    expect(container.querySelectorAll('[aria-label="Submit form"] kbd')).toHaveLength(1)
    for (const control of container.querySelectorAll('input, select')) {
      const event = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })
      act(() => control.dispatchEvent(event))
      expect(event.defaultPrevented).toBe(true)
    }
    expect(onSubmitForm).toHaveBeenCalledTimes(3)
    act(() => root.render(createElement(FormRuntime, props)))
    expect(container.querySelectorAll('[aria-label="Submit form"] kbd')).toHaveLength(2)
  })
})
