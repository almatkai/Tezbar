import { describe, expect, it } from 'vitest'
import { reconcileFormValues, reconcileFormWheelDraft } from './form-runtime'

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
