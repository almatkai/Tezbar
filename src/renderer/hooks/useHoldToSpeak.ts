import { useCallback, useEffect, useRef, useState } from 'react'

/** Hold-to-speak pipeline for the launcher.
 *
 *  Why this exists:
 *    The previous implementation used Chromium's `webkitSpeechRecognition`,
 *    which in Electron silently tries to upload audio to Google's cloud
 *    endpoint (`speech.googleapis.com`). Electron ships without a Google
 *    API key, so every session failed with
 *      `chunked_data_pipe_upload_data_stream.cc(217) OnSizeReceived failed
 *       with Error: -2`
 *    and the mic button flipped on/off as the fail-and-retry loop fired.
 *
 *  What this does instead:
 *    1. Acquires a single `MediaStream` from the default input device.
 *    2. On press, starts a `MediaRecorder` (WebM/Opus on Chromium).
 *    3. On release, stops the recorder, decodes the audio through the
 *       Web Audio API, resamples to 16 kHz mono, and encodes a 16-bit PCM
 *       RIFF/WAV buffer entirely in-renderer.
 *    4. Sends the WAV bytes to the main process for local transcription.
 *
 *  The WAV-in-renderer step means the main process does not need ffmpeg
 *  installed just to convert WebM → 16 kHz WAV for whisper/moonshine.
 *
 *  Every step is wrapped in try/catch and logs to the console with a
 *  `[stt]` prefix so users can copy-paste logs when something breaks. */

export type HoldToSpeakState =
  | { kind: 'unsupported'; reason: string }
  | { kind: 'idle' }
  | { kind: 'recording' }
  | { kind: 'transcribing' }
  | { kind: 'error'; message: string; hint?: string }

export type HoldToSpeakApi = {
  state: HoldToSpeakState
  /** Returns true if the platform can capture audio at all. */
  supported: boolean
  press: () => void
  release: () => void
}

export type HoldToSpeakOptions = {
  /** Called with the final transcribed text. */
  onTranscript: (text: string) => void
  /** Called whenever a user-visible message should be shown. */
  onMessage?: (message: string) => void
  /** Optional BCP-47 language hint passed to the engine. */
  language?: string
  /** Minimum recording duration (ms) — anything shorter is discarded as
   *  an accidental tap. */
  minDurationMs?: number
}

function pickRecorderMimeType(): string | null {
  const candidates = [
    'audio/webm;codecs=opus',
    'audio/webm',
    'audio/ogg;codecs=opus',
    'audio/mp4',
  ]
  for (const mime of candidates) {
    if (typeof MediaRecorder !== 'undefined' && MediaRecorder.isTypeSupported(mime)) {
      return mime
    }
  }
  return null
}

/** Resample a possibly-multichannel AudioBuffer down to mono 16 kHz
 *  using an OfflineAudioContext. */
async function toMono16kAudioBuffer(source: AudioBuffer): Promise<AudioBuffer> {
  const targetRate = 16_000
  const frames = Math.ceil((source.duration * targetRate))
  const offline = new OfflineAudioContext(1, Math.max(frames, 1), targetRate)
  const node = offline.createBufferSource()
  node.buffer = source
  node.connect(offline.destination)
  node.start(0)
  return offline.startRendering()
}

/** Encode an AudioBuffer (assumed mono, any sample rate) as a
 *  16-bit PCM RIFF/WAV blob. */
function encodeWav(audio: AudioBuffer): Uint8Array {
  const channels = 1
  const sampleRate = audio.sampleRate
  const samples = audio.getChannelData(0)
  const byteLength = 44 + samples.length * 2
  const buffer = new ArrayBuffer(byteLength)
  const view = new DataView(buffer)

  const writeString = (offset: number, text: string): void => {
    for (let i = 0; i < text.length; i += 1) {
      view.setUint8(offset + i, text.charCodeAt(i))
    }
  }

  writeString(0, 'RIFF')
  view.setUint32(4, 36 + samples.length * 2, true)
  writeString(8, 'WAVE')
  writeString(12, 'fmt ')
  view.setUint32(16, 16, true) // PCM chunk size
  view.setUint16(20, 1, true) // PCM format
  view.setUint16(22, channels, true)
  view.setUint32(24, sampleRate, true)
  view.setUint32(28, sampleRate * channels * 2, true) // byte rate
  view.setUint16(32, channels * 2, true) // block align
  view.setUint16(34, 16, true) // bits per sample
  writeString(36, 'data')
  view.setUint32(40, samples.length * 2, true)

  let offset = 44
  for (let i = 0; i < samples.length; i += 1) {
    const clamped = Math.max(-1, Math.min(1, samples[i] ?? 0))
    view.setInt16(offset, clamped < 0 ? clamped * 0x8000 : clamped * 0x7fff, true)
    offset += 2
  }

  return new Uint8Array(buffer)
}

export function useHoldToSpeak(options: HoldToSpeakOptions): HoldToSpeakApi {
  const { onTranscript, onMessage, language, minDurationMs = 250 } = options

  const [state, setState] = useState<HoldToSpeakState>(() => {
    if (typeof window === 'undefined') {
      return { kind: 'unsupported', reason: 'No window object.' }
    }
    if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia) {
      return { kind: 'unsupported', reason: 'navigator.mediaDevices.getUserMedia is not available.' }
    }
    if (typeof MediaRecorder === 'undefined') {
      return { kind: 'unsupported', reason: 'MediaRecorder is not available.' }
    }
    if (!pickRecorderMimeType()) {
      return { kind: 'unsupported', reason: 'No supported MediaRecorder mime type.' }
    }
    return { kind: 'idle' }
  })

  const supported = state.kind !== 'unsupported'

  const streamRef = useRef<MediaStream | null>(null)
  const recorderRef = useRef<MediaRecorder | null>(null)
  const chunksRef = useRef<Blob[]>([])
  const startedAtRef = useRef<number>(0)
  const pressedRef = useRef<boolean>(false)
  const mimeRef = useRef<string | null>(null)

  const setError = useCallback(
    (message: string, hint?: string) => {
      console.warn('[stt][renderer] error:', message, hint ? `\n${hint}` : '')
      setState({ kind: 'error', message, hint })
      onMessage?.(hint ? `${message}\n${hint}` : message)
    },
    [onMessage],
  )

  const ensureStream = useCallback(async (): Promise<MediaStream | null> => {
    if (streamRef.current && streamRef.current.active) return streamRef.current
    try {
      console.info('[stt][renderer] requesting mic access')
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
        video: false,
      })
      streamRef.current = stream
      console.info(
        '[stt][renderer] mic granted; tracks:',
        stream.getAudioTracks().map((t) => t.label || t.id),
      )
      return stream
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      setError('Microphone permission was denied or unavailable.', `Details: ${msg}`)
      return null
    }
  }, [setError])

  const transcribeChunks = useCallback(
    async (chunks: Blob[], mime: string): Promise<void> => {
      if (chunks.length === 0) {
        setState({ kind: 'idle' })
        return
      }
      setState({ kind: 'transcribing' })
      const raw = new Blob(chunks, { type: mime })
      console.info('[stt][renderer] encoded blob:', raw.size, 'bytes,', mime)

      try {
        const arrayBuffer = await raw.arrayBuffer()
        let bytes: Uint8Array
        let payloadMime: string

        try {
          const ctx = new (window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)()
          const decoded = await ctx.decodeAudioData(arrayBuffer.slice(0))
          await ctx.close()
          const mono16k = await toMono16kAudioBuffer(decoded)
          bytes = encodeWav(mono16k)
          payloadMime = 'audio/wav'
          console.info('[stt][renderer] re-encoded as 16k mono WAV:', bytes.byteLength, 'bytes')
        } catch (decodeErr) {
          console.warn(
            '[stt][renderer] WAV re-encode failed, sending original blob:',
            decodeErr instanceof Error ? decodeErr.message : decodeErr,
          )
          bytes = new Uint8Array(arrayBuffer)
          payloadMime = mime
        }

        const result = await window.tezbar.voiceTranscribe({
          audioBytes: bytes.buffer as ArrayBuffer,
          mimeType: payloadMime,
          language,
        })

        if (result.ok) {
          console.info(`[stt][renderer] got ${result.text.length} chars from ${result.engine}`)
          onTranscript(result.text)
          setState({ kind: 'idle' })
        } else {
          setError(result.error, result.hint)
        }
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        setError('Could not reach the transcription service.', `Details: ${msg}`)
      }
    },
    [language, onTranscript, setError],
  )

  const press = useCallback((): void => {
    if (pressedRef.current) return
    if (state.kind === 'unsupported') return
    pressedRef.current = true

    void (async () => {
      const stream = await ensureStream()
      if (!stream || !pressedRef.current) {
        pressedRef.current = false
        if (stream) {
          for (const track of stream.getTracks()) {
            try {
              track.stop()
            } catch {
              // ignore
            }
          }
          streamRef.current = null
        }
        return
      }

      // We only suppress blur-hide inside the recording window. The OS
      // microphone sheet steals focus the first time a user grants
      // access, and without this the launcher disappears mid-record.
      try {
        await window.tezbar.setSuppressBlurHide(true)
      } catch {
        // non-fatal
      }

      const mime = pickRecorderMimeType()
      if (!mime) {
        setError('Browser does not support any audio recording format.')
        pressedRef.current = false
        if (stream) {
          for (const track of stream.getTracks()) {
            try {
              track.stop()
            } catch {
              // ignore
            }
          }
          streamRef.current = null
        }
        return
      }
      mimeRef.current = mime

      try {
        const recorder = new MediaRecorder(stream, { mimeType: mime })
        recorderRef.current = recorder
        chunksRef.current = []
        startedAtRef.current = performance.now()

        recorder.ondataavailable = (event): void => {
          if (event.data && event.data.size > 0) {
            chunksRef.current.push(event.data)
          }
        }
        recorder.onerror = (event: Event & { error?: { message?: string } }): void => {
          console.warn('[stt][renderer] recorder error:', event.error?.message ?? event)
        }
        recorder.onstop = (): void => {
          const chunks = chunksRef.current
          const usedMime = mimeRef.current ?? mime
          chunksRef.current = []
          recorderRef.current = null
          const duration = performance.now() - startedAtRef.current
          void window.tezbar.setSuppressBlurHide(false).catch(() => undefined)
          console.info('[stt][renderer] stopped after', Math.round(duration), 'ms')

          // Stop all tracks immediately so the macOS mic indicator turns off
          const activeStream = streamRef.current
          if (activeStream) {
            for (const track of activeStream.getTracks()) {
              try {
                track.stop()
              } catch {
                // ignore
              }
            }
            streamRef.current = null
          }

          if (duration < minDurationMs) {
            setState({ kind: 'idle' })
            onMessage?.('Hold the mic a bit longer to record.')
            return
          }
          void transcribeChunks(chunks, usedMime)
        }

        recorder.start(100)
        setState({ kind: 'recording' })
        console.info('[stt][renderer] recording started; mime:', mime)
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        setError('Failed to start the audio recorder.', `Details: ${msg}`)
        pressedRef.current = false
        void window.tezbar.setSuppressBlurHide(false).catch(() => undefined)
        if (stream) {
          for (const track of stream.getTracks()) {
            try {
              track.stop()
            } catch {
              // ignore
            }
          }
          streamRef.current = null
        }
      }
    })()
  }, [ensureStream, minDurationMs, onMessage, setError, state.kind, transcribeChunks])

  const release = useCallback((): void => {
    if (!pressedRef.current) return
    pressedRef.current = false
    const recorder = recorderRef.current
    if (!recorder) {
      void window.tezbar.setSuppressBlurHide(false).catch(() => undefined)
      return
    }
    if (recorder.state === 'recording') {
      try {
        recorder.stop()
      } catch (err) {
        console.warn('[stt][renderer] recorder.stop() threw:', err)
      }
    }
  }, [])

  // Stop and release everything on unmount so we don't leave the mic
  // indicator stuck on after the launcher closes.
  useEffect(() => {
    return () => {
      pressedRef.current = false
      const recorder = recorderRef.current
      if (recorder && recorder.state !== 'inactive') {
        try {
          recorder.stop()
        } catch {
          // ignore
        }
      }
      recorderRef.current = null
      const stream = streamRef.current
      if (stream) {
        for (const track of stream.getTracks()) {
          try {
            track.stop()
          } catch {
            // ignore
          }
        }
        streamRef.current = null
      }
      void window.tezbar.setSuppressBlurHide(false).catch(() => undefined)
    }
  }, [])

  return { state, supported, press, release }
}
