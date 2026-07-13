import { describe, expect, it } from 'vitest'
import { buildContentFtsQuery, buildFtsQuery, fuzzySimilarityScore } from './textMatch'

describe('buildFtsQuery', () => {
  it('uses only FTS-safe prefix tokens', () => {
    expect(buildFtsQuery('port-manager')).toBe('port* OR manager*')
    expect(buildFtsQuery('raycast.port_manager')).toBe('raycast* OR port* OR manager*')
  })

  it('drops punctuation-only queries', () => {
    expect(buildFtsQuery('*')).toBe('')
    expect(buildFtsQuery('--- ... ___')).toBe('')
  })

  it('keeps Unicode words', () => {
    expect(buildFtsQuery('Поиск документов')).toBe('поиск* OR документов*')
  })
})

describe('buildContentFtsQuery', () => {
  it('drops dangerously broad partial tokens', () => {
    expect(buildContentFtsQuery('i')).toBe('')
    expect(buildContentFtsQuery('in')).toBe('')
    expect(buildContentFtsQuery('inv')).toBe('inv')
  })

  it('requires all content terms and only prefixes specific tokens', () => {
    expect(buildContentFtsQuery('invoice report')).toBe('invoice* AND report*')
    expect(buildContentFtsQuery('Поиск документов')).toBe('поиск* AND документов*')
  })
})

describe('fuzzySimilarityScore', () => {
  it('matches small typos by token similarity', () => {
    expect(fuzzySimilarityScore('Quick Notes', 'quik notes')).toBeGreaterThan(0.75)
  })

  it('matches Unicode query tokens', () => {
    expect(
      fuzzySimilarityScore('Быстрый поиск документов', 'Поиск документов')
    ).toBeGreaterThan(0.9)
  })

  it('does not match unrelated text', () => {
    expect(fuzzySimilarityScore('Quick Notes', 'battery')).toBe(0)
  })

  it('does not connect gemini to unrelated descriptions through the word in', () => {
    expect(
      fuzzySimilarityScore(
        'Open ~/Library native-command Reveal the hidden Library folder in Finder',
        'gemini'
      )
    ).toBe(0)
    expect(fuzzySimilarityScore('Open Ports in Menu Bar extensions Port Manager', 'gemini')).toBe(0)
    expect(fuzzySimilarityScore('customer-portal.runtime.min.js', 'gemini')).toBe(0)
  })

  it('still discovers Visual Studio Code for the initial vs code query', () => {
    expect(
      fuzzySimilarityScore('Visual Studio Code application', 'vs code')
    ).toBeGreaterThanOrEqual(0.45)
  })
})
