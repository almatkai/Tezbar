import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import type { ExtensionRuntimeNode } from '../../../shared/extensionRuntime'
import { parseNativeColorWheel, wheelCoordinatesFromPointer } from './detail-runtime'
import { FormRuntime } from './form-runtime'
import { GridRuntime, gridEmptyState } from './grid-runtime'
import { backActionId } from './index'

const wheel = {
  hue: 90,
  saturation: 75,
  value: 60,
  alpha: 85,
  preview: '#779922',
  formats: [{ key: 'hex', title: 'HEX', value: '#779922', actionId: 'color-picker:wheel-copy:hex' }],
  changeActionId: 'color-picker:wheel-change',
  saveActionId: 'color-picker:wheel-submit',
}

describe('native V2 visuals', () => {
  it('renders Rust supplied color formats with the existing wheel and numeric controls', () => {
    const root: ExtensionRuntimeNode = {
      type: 'Form',
      props: { nativeColorWheel: wheel, backActionId: 'color-picker:back' },
      children: [
        { type: 'Form.TextField', props: { id: 'hue', title: 'Hue (degrees)', value: '90' } },
        { type: 'Form.TextField', props: { id: 'saturation', title: 'Saturation (%)', value: '75' } },
      ],
    }
    const markup = renderToStaticMarkup(React.createElement(FormRuntime, {
      root,
      title: 'Color Wheel',
      onBack: () => undefined,
      onSubmitForm: () => undefined,
      onChangeField: () => undefined,
      onInvokeAction: () => undefined,
      onOpenActions: () => undefined,
    }))

    expect(parseNativeColorWheel(root.props?.nativeColorWheel)).toEqual(wheel)
    expect(markup).toContain('aria-label="Color wheel"')
    expect(markup).toContain('conic-gradient')
    expect(markup).toContain('#779922')
    expect(markup).toContain('Hue (degrees)')
    expect(backActionId(root)).toBe('color-picker:back')
  })

  it('preserves brightness and opacity when picking hue and saturation', () => {
    expect(wheelCoordinatesFromPointer(0, -50, 100, wheel)).toMatchObject({
      hue: 0, saturation: 100, value: 60, alpha: 85,
    })
    expect(wheelCoordinatesFromPointer(60, 0, 100, wheel)).toBeNull()
  })

  it('shows the GIF adapter loading, empty and error titles', () => {
    const root: ExtensionRuntimeNode = {
      type: 'Grid',
      props: { isLoading: true, filtering: false },
      children: [{ type: 'Grid.EmptyView', props: { title: 'Loading GIFs…' } }],
    }
    const markup = renderToStaticMarkup(React.createElement(GridRuntime, {
      root,
      title: 'Search for GIFs',
      actions: [],
      onBack: () => undefined,
      onRunPrimaryAction: () => undefined,
      onOpenActions: () => undefined,
      onSearchTextChanged: () => undefined,
    }))

    expect(gridEmptyState(root, '')).toEqual({ title: 'Loading GIFs…', description: undefined, loading: true })
    expect(markup).toContain('Loading GIFs…')
    expect(markup).toContain('aria-busy="true"')
    root.props = { isLoading: false }
    root.children = [{ type: 'Grid.EmptyView', props: { title: 'Provider request failed' } }]
    expect(gridEmptyState(root, 'cat').title).toBe('Provider request failed')
    expect(backActionId({ type: 'Detail', props: { backActionId: 'gif-search:back' } })).toBe('gif-search:back')
    expect(backActionId({ type: 'Detail', props: {} })).toBeNull()
  })
})
