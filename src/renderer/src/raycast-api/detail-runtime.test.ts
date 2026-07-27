import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import type { ExtensionRuntimeNode } from '../../../shared/extensionRuntime'
import { DetailRuntime } from './detail-runtime'

describe('DetailRuntime color wheel', () => {
  it('renders the wheel pixels without depending on an extension image asset', () => {
    const root: ExtensionRuntimeNode = {
      type: 'Detail',
      props: {
        markdown: '![RGB Color Wheel](rgb-color-wheel.webp?&raycast-height=350)',
        initialColor: { red: 255, green: 235, blue: 170 },
      },
    }

    const markup = renderToStaticMarkup(
      React.createElement(DetailRuntime, {
        root,
        title: 'Color Wheel',
        onBack: () => undefined,
        onRunPrimaryAction: () => undefined,
        onOpenActions: () => undefined,
      })
    )

    expect(markup).toMatch(/background-image:[^;]*conic-gradient/)
    expect(markup).toMatch(/background-image:[^;]*radial-gradient/)
  })
})
