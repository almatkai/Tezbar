export const VOICE_MODEL_IDS = [
  'moonshine-base-en',
  'moonshine-tiny-en',
  'whisper-tiny-en',
  'whisper-tiny',
  'whisper-base',
  'whisper-small',
  'whisper-medium-en',
  'whisper-large-v3-turbo',
] as const

export type VoiceModelId = (typeof VOICE_MODEL_IDS)[number]

export type VoiceModelLanguage = 'en' | 'multilingual'

export type VoiceModelTier = 'fast' | 'balanced' | 'accurate'

export type VoiceModelStatus = 'not-downloaded' | 'downloading' | 'downloaded' | 'error'

/** What the model needs at runtime in order to actually produce text.
 *  Surfaced to the UI so users can see *why* downloading finished but
 *  transcription still fails (e.g. whisper.cpp binary missing). */
export type VoiceModelRuntime = {
  /** Human label, e.g. "whisper.cpp" or "Moonshine (Python)". */
  label: string
  /** True when the underlying binary/package is present and callable. */
  ready: boolean
  /** If not ready, the one-liner install command we tried / will try. */
  installCommand: string
  /** Free-form detail, shown in Settings if install failed. */
  message?: string
}

/** Which phase of provisioning the model is in. */
export type VoiceModelStage = 'idle' | 'installing-runtime' | 'downloading-weights'

export type VoiceModel = {
  id: VoiceModelId
  name: string
  family: 'moonshine' | 'whisper'
  description: string
  homepageUrl: string
  sizeLabel: string
  language: VoiceModelLanguage
  tier: VoiceModelTier
  estimatedSizeMb: number
  status: VoiceModelStatus
  stage: VoiceModelStage
  progress: number | null
  downloadedBytes: number
  totalBytes: number | null
  selected: boolean
  errorMessage?: string
  runtime: VoiceModelRuntime
}

export type VoiceModelDownloadRequest = {
  modelId: VoiceModelId
}

export type VoiceModelSelectionRequest = {
  modelId: VoiceModelId
}
