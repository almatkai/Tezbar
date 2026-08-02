import { describe, expect, it } from 'vitest'
import type { Stage } from '../../shared/agent'
import { appendTimelineText, upsertTimelineStage } from './timeline'

function stage(index: number, status: Stage['status'] = 'running'): Stage {
  return { index, label: `bash: step ${index}`, status }
}

describe('appendTimelineText', () => {
  it('appends to the trailing text item and starts a new one after a stage', () => {
    let items = appendTimelineText([], 'hello')
    items = appendTimelineText(items, ' world')
    expect(items).toEqual([{ type: 'text', text: 'hello world' }])

    items = upsertTimelineStage(items, stage(0))
    items = appendTimelineText(items, 'next paragraph')
    expect(items).toEqual([
      { type: 'text', text: 'hello world' },
      { type: 'stage', stage: stage(0) },
      { type: 'text', text: 'next paragraph' },
    ])
  })
})

describe('upsertTimelineStage', () => {
  it('updates an existing stage in place, never moving it above later text', () => {
    // tool A starts, tool B starts, text streams, then tool A finishes.
    let items = upsertTimelineStage([], stage(0, 'running'))
    items = upsertTimelineStage(items, stage(1, 'running'))
    items = appendTimelineText(items, 'interleaved prose')
    items = upsertTimelineStage(items, stage(0, 'done'))

    expect(items.map((item) => item.type)).toEqual(['stage', 'stage', 'text'])
    const updated = items[0]!
    expect(updated.type === 'stage' && updated.stage.status).toBe('done')
    const prose = items[2]!
    expect(prose.type === 'text' && prose.text).toBe('interleaved prose')
  })

  it('keeps real execution order: stage → text → stage lands after the text', () => {
    // Logical order: tool call, some prose, then another tool call.
    let items = upsertTimelineStage([], stage(0, 'running'))
    items = appendTimelineText(items, 'Let me check that.')
    items = upsertTimelineStage(items, stage(1, 'running'))
    items = appendTimelineText(items, 'All set.')

    expect(
      items.map((item) => {
        if (item.type === 'stage') return `s${item.stage.index}`
        return 'text'
      })
    ).toEqual(['s0', 'text', 's1', 'text'])
  })
})
