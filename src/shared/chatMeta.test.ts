import { describe, expect, it } from 'vitest'
import { heuristicChatMeta, isEmoji } from './chatMeta'

describe('chatMeta', () => {
  it('correctly detects emoji strings', () => {
    expect(isEmoji('💻')).toBe(true)
    expect(isEmoji('⚽')).toBe(true)
    expect(isEmoji('⚡')).toBe(true)
    expect(isEmoji('🤖')).toBe(true)
    expect(isEmoji('chat')).toBe(false)
    expect(isEmoji('')).toBe(false)
    expect(isEmoji(null)).toBe(false)
    expect(isEmoji(undefined)).toBe(false)
  })

  it('generates CPU and compiler diagnostic meta for CPU queries', () => {
    const meta = heuristicChatMeta(
      'What is using the most CPU? compiler 2-4gb? wtf it just takes 2-4gb for idle or just once build the app?'
    )
    expect(meta.icon).toBe('💻')
    expect(meta.title).toBe('CPU Usage & Compiler Memory')
  })

  it('generates sports meta for Messi queries', () => {
    const meta = heuristicChatMeta('who is messi?')
    expect(meta.icon).toBe('⚽')
    expect(meta.title).toBe('Lionel Messi')
  })

  it('generates sports meta for Ronaldo queries', () => {
    const meta = heuristicChatMeta('кто такой роналду')
    expect(meta.icon).toBe('⚽')
    expect(meta.title).toBe('Cristiano Ronaldo')
  })

  it('generates coding meta for coding queries', () => {
    const meta = heuristicChatMeta('how to build a react hook in typescript?')
    expect(meta.icon).toBe('⚡')
    expect(meta.title).toBe('Build a react hook in typescript')
  })

  it('cleans boilerplate question phrases in English and Russian', () => {
    const en = heuristicChatMeta('Can you please tell me what is the speed of light?')
    expect(en.title).toBe('The speed of light')

    const ru = heuristicChatMeta('Подскажи пожалуйста что такое квантовая запутанность?')
    expect(ru.title).toBe('Квантовая запутанность')
  })
})
