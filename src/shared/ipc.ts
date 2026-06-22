import type { SearchAction, SearchExecuteContext } from './search'

export const IPC_CHANNELS = {
  QUERY: 'query',
  SEARCH_ALL: 'search:all',
  PATH_COMPLETE: 'path:complete',
  DIRECTORY_VISIT_RECORD: 'directory-visit:record',
  SEARCH_EXECUTE: 'search:execute',
  SEARCH_BENCHMARK_RUN: 'search:benchmark:run',
  SEARCH_BENCHMARK_HISTORY: 'search:benchmark:history',
  AI_ACTION: 'ai:action',
  VOICE_TTS_SPEAK: 'voice:tts:speak',
  VOICE_TTS_STOP: 'voice:tts:stop',
  VOICE_STT_MODES: 'voice:stt:modes',
  VOICE_STT_TRANSCRIBE: 'voice:stt:transcribe',
  VOICE_MODELS_LIST: 'voice:models:list',
  VOICE_MODEL_DOWNLOAD: 'voice:model:download',
  VOICE_MODEL_GET_SELECTED: 'voice:model:get-selected',
  VOICE_MODEL_SET_SELECTED: 'voice:model:set-selected',
} as const

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

export type SearchExecuteRequest = {
  action: SearchAction
  context?: SearchExecuteContext
}

export function parseSearchExecuteRequest(payload: unknown): SearchExecuteRequest {
  if (!isRecord(payload) || !('action' in payload)) {
    throw new Error('Invalid search execute payload')
  }

  const action = payload.action as SearchAction
  const context = isRecord(payload.context) ? (payload.context as SearchExecuteContext) : undefined
  return { action, context }
}

export type VoiceSpeakRequest = {
  text: string
}

export function parseVoiceSpeakRequest(payload: unknown): VoiceSpeakRequest {
  if (!isRecord(payload) || typeof payload.text !== 'string') {
    throw new Error('Invalid voice speak payload')
  }
  return { text: payload.text }
}

export type VoiceModelRequest = {
  modelId: string
}

function toUint8Array(value: unknown): Uint8Array | null {
  if (value instanceof Uint8Array) {
    return value
  }
  if (value instanceof ArrayBuffer) {
    return new Uint8Array(value)
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength)
  }
  if (Array.isArray(value) && value.every((item) => typeof item === 'number')) {
    return Uint8Array.from(value)
  }
  return null
}

export type VoiceTranscribeRequest = {
  audioBytes: Uint8Array
  mimeType?: string
  language?: string
}

export function parseVoiceTranscribeRequest(payload: unknown): VoiceTranscribeRequest {
  if (!isRecord(payload)) {
    throw new Error('Invalid voice transcription payload')
  }

  const audioBytes = toUint8Array(payload.audioBytes)
  if (!audioBytes || audioBytes.byteLength === 0) {
    throw new Error('Voice transcription payload must include audio bytes')
  }

  const mimeType = typeof payload.mimeType === 'string' ? payload.mimeType : undefined
  const language = typeof payload.language === 'string' ? payload.language : undefined

  return { audioBytes, mimeType, language }
}

export function parseVoiceModelRequest(payload: unknown): VoiceModelRequest {
  if (!isRecord(payload) || typeof payload.modelId !== 'string') {
    throw new Error('Invalid voice model payload')
  }
  return { modelId: payload.modelId }
}

export type AiActionRequest = {
  instruction: string
  selectedText?: string
  appContext?: string
  allowAutomation?: boolean
  redactSensitive?: boolean
}

export function parseAiActionRequest(payload: unknown): AiActionRequest {
  if (!isRecord(payload) || typeof payload.instruction !== 'string') {
    throw new Error('Invalid AI action payload')
  }

  return {
    instruction: payload.instruction,
    selectedText: typeof payload.selectedText === 'string' ? payload.selectedText : undefined,
    appContext: typeof payload.appContext === 'string' ? payload.appContext : undefined,
    allowAutomation: payload.allowAutomation === true,
    redactSensitive: payload.redactSensitive !== false,
  }
}
