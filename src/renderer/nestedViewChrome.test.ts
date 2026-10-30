// @vitest-environment jsdom
import { createElement, type ReactElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import ClipboardView from './ClipboardView'
import ExtensionsView from './ExtensionsView'
import PermissionsView from './PermissionsView'
import ExtensionRuntimeView from './ExtensionRuntimeView'
import { ListRuntime } from './src/raycast-api/list-runtime'
import { GridRuntime } from './src/raycast-api/grid-runtime'
import { DetailRuntime } from './src/raycast-api/detail-runtime'
import { FormRuntime } from './src/raycast-api/form-runtime'

const noop = (): void => {}
const runtimeProps = {
  title: 'Open Ports',
  commandName: 'open-ports',
  actions: [],
  onBack: noop,
  onRunPrimaryAction: noop,
  onOpenActions: noop,
  onSearchTextChanged: noop,
  onLoadMore: noop,
}

function render(element: ReactElement): Document {
  return new DOMParser().parseFromString(renderToStaticMarkup(element), 'text/html')
}

function expectChrome(doc: Document): Element {
  const header = doc.querySelector('[data-command-surface-header]')!
  expect(header).not.toBeNull()
  expect(header.hasAttribute('data-window-search-drag')).toBe(true)
  expect(header.querySelector('button[aria-label="Back"].no-drag')).not.toBeNull()
  expect(header.querySelector('svg path')?.getAttribute('d')).toBe('m15 18-6-6 6-6')
  expect(doc.querySelectorAll('[data-command-surface-header]')).toHaveLength(1)
  expect(doc.querySelectorAll('[data-command-surface-footer]')).toHaveLength(1)
  expect(doc.querySelector('.view-header, .glass-card')).toBeNull()
  return header
}

describe('nested views share launcher chrome', () => {
  it('renders Clipboard search and filters in one shared header with title/count in the footer', () => {
    const doc = render(createElement(ClipboardView, { onBack: noop }))
    const header = expectChrome(doc)
    expect(header.querySelector('input')?.placeholder).toBe('Search clipboard history…')
    expect(header.querySelector('input')?.classList.contains('command-surface-input')).toBe(true)
    expect(header.querySelectorAll('.launcher-mode-chip')).toHaveLength(5)
    expect(header.querySelector('[aria-pressed="true"]')?.textContent).toBe('All')
    expect(doc.querySelector('[data-command-surface-status]')?.textContent).toBe(
      'Clipboard History · 0 items'
    )
  })

  it('renders extension list search/accessory in the same header and keeps its count', () => {
    const doc = render(
      createElement(ListRuntime, {
        ...runtimeProps,
        root: {
          type: 'List',
          props: {
            searchBarPlaceholder: 'Search ports…',
            searchBarAccessory: {
              type: 'List.Dropdown',
              children: [
                { type: 'List.Dropdown.Item', props: { title: 'TCP', value: 'tcp' } },
                { type: 'List.Dropdown.Item', props: { title: 'UDP', value: 'udp' } },
              ],
            },
          },
          children: [{ type: 'List.Item', props: { id: 'port-1', title: '8080' } }],
        },
      })
    )
    const header = expectChrome(doc)
    expect(header.querySelector('input')?.placeholder).toBe('Search ports…')
    expect(header.querySelectorAll('.launcher-mode-chip')).toHaveLength(2)
    expect(header.querySelector('[aria-pressed="true"]')?.textContent).toBe('TCP')
    expect(doc.querySelector('[data-command-surface-status]')?.textContent).toBe(
      'Open Ports · 1 item'
    )
  })

  it('gives GIF grids the same flat search header, functional select and footer', () => {
    const doc = render(
      createElement(GridRuntime, {
        ...runtimeProps,
        title: 'Browse Recent GIFs',
        root: {
          type: 'Grid',
          props: {
            searchBarPlaceholder: 'Search GIFs…',
            searchBarAccessory: {
              type: 'List.Dropdown',
              children: [{ type: 'List.Dropdown.Item', props: { title: 'Giphy', value: 'giphy' } }],
            },
          },
        },
      })
    )
    const header = expectChrome(doc)
    expect(header.querySelector('input')?.placeholder).toBe('Search GIFs…')
    expect(header.querySelector<HTMLSelectElement>('select.no-drag')?.value).toBe('giphy')
    expect(doc.querySelector('[data-command-surface-status]')?.textContent).toBe(
      'Browse Recent GIFs · 0 items'
    )
  })

  it.each(['List', 'Grid'])('uses a plain title when %s has no search', (type) => {
    const props = {
      ...runtimeProps,
      root: { type, props: { navigationTitle: 'System Overview' } },
    }
    const doc = render(
      type === 'List' ? createElement(ListRuntime, props) : createElement(GridRuntime, props)
    )
    const header = expectChrome(doc)
    expect(header.querySelector('input')).toBeNull()
    expect(header.textContent).toContain('System Overview')
  })

  it.each(['List', 'Grid', 'Tezbar.PreferenceSetup'])(
    'keeps %s chrome inside its own sheet through the extension host wrapper',
    (type) => {
      const doc = render(
        createElement(ExtensionRuntimeView, {
          onBack: noop,
          initial: {
            ok: true,
            mode: 'view',
            sessionId: 'chrome-test',
            extensionId: 'fixture',
            commandName: 'fixture',
            title: 'Fixture',
            actions: [],
            root: { type, props: { title: 'Fixture' } },
          },
        })
      )
      expectChrome(doc)
      expect(
        doc.querySelector('.extension-runtime-host .view-sheet > [data-command-surface-header]')
      ).not.toBeNull()
      expect(
        doc.querySelector('.extension-runtime-host .view-sheet > [data-command-surface-footer]')
      ).not.toBeNull()
    }
  )

  it('gives detail and form runtimes title headers and floating footer slots', () => {
    const detail = render(
      createElement(DetailRuntime, {
        ...runtimeProps,
        root: { type: 'Detail', props: { markdown: 'Port details' } },
      })
    )
    expect(expectChrome(detail).textContent).toContain('Open Ports')
    const form = render(
      createElement(FormRuntime, {
        ...runtimeProps,
        root: { type: 'Form' },
        onSubmitForm: noop,
        onChangeField: noop,
        onInvokeAction: noop,
      })
    )
    expect(expectChrome(form).textContent).toContain('Open Ports')
    expect(form.querySelector('form.view-sheet > [data-command-surface-footer]')).not.toBeNull()
    expect(form.querySelector('button[type="submit"][aria-label="Submit form"]')).not.toBeNull()
  })

  it.each([false, true])('uses shared Permissions chrome (nativeWindow=%s)', (nativeWindow) => {
    const doc = render(createElement(PermissionsView, { onBack: noop, nativeWindow }))
    expect(expectChrome(doc).textContent).toBe('Permissions')
  })

  it('changes standalone Extensions chrome while retaining embedded Settings cards', () => {
    expectChrome(render(createElement(ExtensionsView, { onBack: noop })))
    const embedded = render(createElement(ExtensionsView, { onBack: noop, embedded: true }))
    expect(
      embedded.querySelector('[data-command-surface-header], [data-command-surface-footer]')
    ).toBeNull()
    expect(embedded.querySelector('.extensions-store-body.glass-card')).not.toBeNull()
  })

  it('keeps textual modifier/Enter keycaps out of every in-scope view', () => {
    const files = [
      'ClipboardView',
      'NotesView',
      'SnippetsView',
      'EmojiPickerView',
      'IndexingView',
      'SystemMonitorView',
      'PermissionsView',
      'ReleaseNotesView',
      'ExtensionsView',
      'ExtensionRuntimeView',
      'src/raycast-api/list-runtime',
      'src/raycast-api/grid-runtime',
      'src/raycast-api/detail-runtime',
      'src/raycast-api/form-runtime',
    ]
    for (const file of files) {
      const source = readFileSync(resolve('src/renderer', `${file}.tsx`), 'utf8')
      expect(source, file).not.toMatch(/<Kbd>\s*(?:Cmd|Ctrl|Shift|Enter)\s*<\/Kbd>/)
    }
  })
})
