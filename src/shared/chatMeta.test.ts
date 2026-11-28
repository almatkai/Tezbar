import { describe, expect, it } from 'vitest'
import { chatIconToSquircleKind, heuristicChatMeta, isEmoji } from './chatMeta'

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
    expect(meta.icon).toBe('cpu')
    expect(meta.title).toBe('CPU Usage & Compiler Memory')
  })

  it('generates sports meta for Messi queries', () => {
    const meta = heuristicChatMeta('who is messi?')
    expect(meta.icon).toBe('trophy')
    expect(meta.title).toBe('Lionel Messi')
  })

  it('generates sports meta for Ronaldo queries', () => {
    const meta = heuristicChatMeta('кто такой роналду')
    expect(meta.icon).toBe('trophy')
    expect(meta.title).toBe('Cristiano Ronaldo')
  })

  it('generates coding meta for coding queries', () => {
    const meta = heuristicChatMeta('how to build a react hook in typescript?')
    expect(meta.icon).toBe('snippets')
    expect(meta.title).toBe('Build a react hook in typescript')
  })

  it('cleans boilerplate question phrases in English and Russian', () => {
    const en = heuristicChatMeta('Can you please tell me what is the speed of light?')
    expect(en.title).toBe('The speed of light')

    const ru = heuristicChatMeta('Подскажи пожалуйста что такое квантовая запутанность?')
    expect(ru.title).toBe('Квантовая запутанность')

    const ruPorts = heuristicChatMeta('Скажи Порты открыты t3 code')
    expect(ruPorts.title).toBe('Порты открыты t3 code')
    expect(ruPorts.icon).toBe('cpu')

    const ruComma = heuristicChatMeta('Скажи, какие порты открыты?')
    expect(ruComma.title).toBe('Порты открыты')
    expect(ruComma.icon).toBe('cpu')
  })

  it('generates meta for MCP and API key queries from user prompts', () => {
    const mcp = heuristicChatMeta('MCP version 2 when it came out?')
    expect(mcp.icon).toBe('snippets')
    expect(mcp.title).toBe('MCP Version 2')

    const apiKey = heuristicChatMeta('API Key как вообще генерить')
    expect(apiKey.icon).toBe('key')
    expect(apiKey.title).toBe('API Key')
  })

  it('maps both legacy emojis and kind strings to solid squircle launcher kinds', () => {
    expect(chatIconToSquircleKind('⚡')).toBe('snippets')
    expect(chatIconToSquircleKind('💻')).toBe('cpu')
    expect(chatIconToSquircleKind('🔑')).toBe('key')
    expect(chatIconToSquircleKind('⚽')).toBe('trophy')
    expect(chatIconToSquircleKind('📝')).toBe('edit')
    expect(chatIconToSquircleKind('🤖')).toBe('ai')
    expect(chatIconToSquircleKind('💬')).toBe('chat')
    expect(chatIconToSquircleKind('code')).toBe('snippets')
    expect(chatIconToSquircleKind('key')).toBe('key')
    expect(chatIconToSquircleKind('cpu')).toBe('cpu')
    expect(chatIconToSquircleKind(undefined)).toBe('chat')
  })
})
