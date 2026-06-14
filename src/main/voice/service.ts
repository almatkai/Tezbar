import { app } from 'electron'
import { execFile, spawn } from 'node:child_process'
import { createWriteStream, existsSync, mkdirSync, statSync } from 'node:fs'
import { promises as fs } from 'node:fs'
import { dirname, join } from 'node:path'
import { once } from 'node:events'
import { promisify } from 'node:util'
import { readRawConfig, writeConfigPatch } from '../llm/configStore'
import type { VoiceModel, VoiceModelId } from '../../shared/voice'
import type { VoiceTranscribeRequest } from '../../shared/ipc'

const execFileAsync = promisify(execFile)

let activeSpeech: ReturnType<typeof spawn> | null = null

/** The PATH that the user's interactive shell sees.
 *
 *  Why: Electron inherits a very minimal PATH (often `/usr/bin:/bin`) when
 *  launched outside a terminal, which breaks two things simultaneously:
 *    - `brew`, `whisper-cli`, `pip-installed` tools can't be found, and
 *    - manually adding `/opt/homebrew/bin` first *also* shadows the
 *      system `python3` with Homebrew's Python, whose site-packages don't
 *      contain the `moonshine_voice` package the user installed via
 *      `pip install --user` against the system interpreter.
 *
 *  Querying the login shell once reproduces the exact PATH the user
 *  would see in Terminal, which has both system tools AND homebrew/pipx
 *  prefixes in the right order. */
let cachedLoginPath: string | null = null
async function getLoginPath(): Promise<string> {
  if (cachedLoginPath !== null) return cachedLoginPath
  try {
    const { stdout } = await execFileAsync('bash', ['-lc', 'echo -n "$PATH"'])
    const fromShell = stdout.trim()
    cachedLoginPath = fromShell || process.env['PATH'] || ''
  } catch {
    cachedLoginPath = process.env['PATH'] || ''
  }
  // Defensive: make sure common dirs are present even if the user has a
  // minimal shell rc. We APPEND them so the user's own ordering wins.
  const extras = ['/opt/homebrew/bin', '/usr/local/bin', '/usr/bin', '/bin']
  const existing = new Set(cachedLoginPath.split(':').filter(Boolean))
  for (const e of extras) {
    if (!existing.has(e)) cachedLoginPath += `:${e}`
  }
  return cachedLoginPath
}

async function execWithUserPath(
  file: string,
  args: string[],
  options: { maxBuffer?: number } = {},
): Promise<{ stdout: string; stderr: string }> {
  const path = await getLoginPath()
  return execFileAsync(file, args, {
    maxBuffer: options.maxBuffer ?? 16 * 1024 * 1024,
    env: { ...process.env, PATH: path },
  })
}

type ModelAsset = {
  fileName: string
  url: string
}

/** Which binary/package a model needs at runtime. We use this to (a)
 *  gate "Download" until the runtime is present and (b) auto-install
 *  the runtime on click when we have a well-known install command. */
type RuntimeKind = 'whisper-cpp' | 'moonshine-python'

type VoiceModelCatalogEntry = {
  id: VoiceModelId
  name: string
  family: 'moonshine' | 'whisper'
  description: string
  homepageUrl: string
  estimatedSizeMb: number
  runtime: RuntimeKind
  assets: ModelAsset[]
}

type DownloadState = {
  status: 'downloading' | 'error'
  stage: 'installing-runtime' | 'downloading-weights'
  downloadedBytes: number
  totalBytes: number | null
  progress: number | null
  errorMessage?: string
}

/** Whisper.cpp ggml files come from the official ggerganov repo on HF —
 *  these are the *actual* files whisper-cli / whisper-cpp consume.
 *  The old catalog pointed at OpenAI's `.safetensors`, which whisper.cpp
 *  cannot read, so downloads were effectively useless. */
const MODEL_CATALOG: VoiceModelCatalogEntry[] = [
  {
    id: 'moonshine-base-en',
    name: 'Moonshine Base (English)',
    family: 'moonshine',
    description: 'Low-latency Moonshine STT model from Moonshine AI.',
    homepageUrl: 'https://github.com/moonshine-ai/moonshine',
    estimatedSizeMb: 140,
    runtime: 'moonshine-python',
    assets: [
      {
        fileName: 'encoder_model.ort',
        url: 'https://download.moonshine.ai/model/base-en/quantized/base-en/encoder_model.ort',
      },
      {
        fileName: 'decoder_model_merged.ort',
        url: 'https://download.moonshine.ai/model/base-en/quantized/base-en/decoder_model_merged.ort',
      },
      {
        fileName: 'tokenizer.bin',
        url: 'https://download.moonshine.ai/model/base-en/quantized/base-en/tokenizer.bin',
      },
    ],
  },
  {
    id: 'whisper-base',
    name: 'Whisper Base (English, whisper.cpp)',
    family: 'whisper',
    description: 'Fast whisper.cpp ggml model — good for quick dictation.',
    homepageUrl: 'https://huggingface.co/ggerganov/whisper.cpp',
    estimatedSizeMb: 150,
    runtime: 'whisper-cpp',
    assets: [
      {
        fileName: 'ggml-base.en.bin',
        url: 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin',
      },
    ],
  },
  {
    id: 'whisper-small',
    name: 'Whisper Small (English, whisper.cpp)',
    family: 'whisper',
    description: 'Higher-accuracy whisper.cpp ggml model — a bit slower, noticeably better.',
    homepageUrl: 'https://huggingface.co/ggerganov/whisper.cpp',
    estimatedSizeMb: 490,
    runtime: 'whisper-cpp',
    assets: [
      {
        fileName: 'ggml-small.en.bin',
        url: 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin',
      },
    ],
  },
]

const activeDownloads = new Map<VoiceModelId, DownloadState>()
const VOICE_MODEL_CONFIG_KEY = 'voiceSttModelId'

function voiceModelsRootDir(): string {
  const dir = join(app.getPath('userData'), 'voice-models')
  mkdirSync(dir, { recursive: true })
  return dir
}

function modelDir(modelId: VoiceModelId): string {
  return join(voiceModelsRootDir(), modelId)
}

function modelAssetPath(modelId: VoiceModelId, fileName: string): string {
  return join(modelDir(modelId), fileName)
}

function findModel(modelId: VoiceModelId): VoiceModelCatalogEntry {
  const model = MODEL_CATALOG.find((entry) => entry.id === modelId)
  if (!model) {
    throw new Error(`Unknown voice model: ${modelId}`)
  }
  return model
}

function readSelectedModelId(): VoiceModelId {
  const config = readRawConfig()
  const raw = config[VOICE_MODEL_CONFIG_KEY]
  if (raw === 'moonshine-base-en' || raw === 'whisper-base' || raw === 'whisper-small') {
    return raw
  }
  return 'moonshine-base-en'
}

function fileSizeOrZero(path: string): number {
  try {
    return statSync(path).size
  } catch {
    return 0
  }
}

function modelDownloadedBytes(model: VoiceModelCatalogEntry): number {
  return model.assets.reduce((acc, asset) => acc + fileSizeOrZero(modelAssetPath(model.id, asset.fileName)), 0)
}

function isModelFullyDownloaded(model: VoiceModelCatalogEntry): boolean {
  return model.assets.every((asset) => {
    const path = modelAssetPath(model.id, asset.fileName)
    return existsSync(path) && fileSizeOrZero(path) > 0
  })
}

// ---------- Runtime installation (binary/package the model needs) ----------
//
// The "Download" button in Settings used to download only the *weights*
// for a model. For whisper-cpp that's a ggml file which is useless
// without the `whisper-cli` binary; for Moonshine the `.ort` files are
// useless without the `moonshine-voice` Python package. We now install
// the runtime as part of the same operation so clicking Download yields
// a working configuration instead of a confusing "engine not available"
// error the moment the user tries to speak.

type RuntimeProbe = {
  ready: boolean
  label: string
  installCommand: string
  message?: string
}

async function probeRuntime(kind: RuntimeKind): Promise<RuntimeProbe> {
  if (kind === 'whisper-cpp') {
    const ready = (await hasBinary('whisper-cli')) || (await hasBinary('whisper-cpp'))
    return {
      ready,
      label: 'whisper.cpp',
      installCommand: 'brew install whisper-cpp',
    }
  }

  const python = await hasBinary('python3')
  if (!python) {
    return {
      ready: false,
      label: 'Moonshine (Python)',
      installCommand: 'brew install python && python3 -m pip install --user moonshine-voice',
      message: 'python3 was not found on your PATH.',
    }
  }
  const ready = await hasMoonshinePython()
  return {
    ready,
    label: 'Moonshine (Python)',
    installCommand: 'python3 -m pip install --user moonshine-voice onnxruntime',
  }
}

/** Run a shell command with the user's login PATH so brew / pip / python
 *  are actually reachable from inside Electron (whose process PATH is
 *  typically stripped back to `/usr/bin:/bin`). */
async function runLoginShell(command: string): Promise<{ stdout: string; stderr: string }> {
  const path = await getLoginPath()
  const { stdout, stderr } = await execFileAsync('bash', ['-lc', command], {
    maxBuffer: 32 * 1024 * 1024,
    env: {
      ...process.env,
      PATH: path,
      HOMEBREW_NO_AUTO_UPDATE: '1',
      HOMEBREW_NO_ANALYTICS: '1',
      HOMEBREW_NO_INSTALL_CLEANUP: '1',
      PIP_DISABLE_PIP_VERSION_CHECK: '1',
    },
  })
  return { stdout, stderr }
}

async function installRuntime(kind: RuntimeKind): Promise<void> {
  if (kind === 'whisper-cpp') {
    if (!(await hasBinary('brew'))) {
      throw new Error(
        'Homebrew is required to install whisper.cpp automatically.\n' +
        'Install Homebrew from https://brew.sh and try again, or install whisper.cpp manually:\n' +
        '  brew install whisper-cpp',
      )
    }
    console.info('[stt][main] installing whisper-cpp via Homebrew — this can take a few minutes')
    try {
      const { stdout, stderr } = await runLoginShell('brew install whisper-cpp')
      console.info('[stt][main] brew stdout:\n' + stdout.trim())
      if (stderr.trim()) console.info('[stt][main] brew stderr:\n' + stderr.trim())
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      throw new Error(`brew install whisper-cpp failed: ${msg}`)
    }
    return
  }

  if (!(await hasBinary('python3'))) {
    throw new Error(
      'python3 was not found. Install Python first (e.g. `brew install python`) and try again.',
    )
  }
  console.info('[stt][main] installing moonshine-voice via pip (user site) — this can take a minute')
  try {
    const { stdout, stderr } = await runLoginShell(
      'python3 -m pip install --user --upgrade moonshine-voice onnxruntime',
    )
    console.info('[stt][main] pip stdout:\n' + stdout.trim())
    if (stderr.trim()) console.info('[stt][main] pip stderr:\n' + stderr.trim())
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    throw new Error(`pip install moonshine-voice failed: ${msg}`)
  }
}

function buildEmptyRuntimeView(kind: RuntimeKind): VoiceModel['runtime'] {
  return {
    label: kind === 'whisper-cpp' ? 'whisper.cpp' : 'Moonshine (Python)',
    ready: false,
    installCommand:
      kind === 'whisper-cpp'
        ? 'brew install whisper-cpp'
        : 'python3 -m pip install --user moonshine-voice onnxruntime',
  }
}

// Cached per-kind so the Settings list doesn't spawn a shell on every
// render; the download pipeline refreshes this explicitly after
// attempting to install.
const runtimeCache = new Map<RuntimeKind, RuntimeProbe>()
async function cachedProbeRuntime(kind: RuntimeKind): Promise<RuntimeProbe> {
  const cached = runtimeCache.get(kind)
  if (cached) return cached
  const probe = await probeRuntime(kind)
  runtimeCache.set(kind, probe)
  return probe
}
function invalidateRuntimeCache(kind: RuntimeKind): void {
  runtimeCache.delete(kind)
}

async function toVoiceModelView(
  model: VoiceModelCatalogEntry,
  selectedId: VoiceModelId,
): Promise<VoiceModel> {
  const active = activeDownloads.get(model.id)
  const weightsDownloaded = isModelFullyDownloaded(model)
  const diskBytes = modelDownloadedBytes(model)
  const runtime = await cachedProbeRuntime(model.runtime)

  const runtimeView: VoiceModel['runtime'] = {
    label: runtime.label,
    ready: runtime.ready,
    installCommand: runtime.installCommand,
    message: runtime.message,
  }

  const selected = model.id === selectedId

  // "Downloaded" is the composite of both runtime + weights — that is
  // what actually matters for Hold-to-Speak to work.
  if (weightsDownloaded && runtime.ready) {
    return {
      id: model.id,
      name: model.name,
      family: model.family,
      description: model.description,
      homepageUrl: model.homepageUrl,
      estimatedSizeMb: model.estimatedSizeMb,
      status: 'downloaded',
      stage: 'idle',
      progress: 1,
      downloadedBytes: diskBytes,
      totalBytes: diskBytes,
      selected,
      runtime: runtimeView,
    }
  }

  if (active) {
    return {
      id: model.id,
      name: model.name,
      family: model.family,
      description: model.description,
      homepageUrl: model.homepageUrl,
      estimatedSizeMb: model.estimatedSizeMb,
      status: active.status,
      stage: active.stage,
      progress: active.progress,
      downloadedBytes: active.downloadedBytes,
      totalBytes: active.totalBytes,
      selected,
      errorMessage: active.errorMessage,
      runtime: runtimeView,
    }
  }

  return {
    id: model.id,
    name: model.name,
    family: model.family,
    description: model.description,
    homepageUrl: model.homepageUrl,
    estimatedSizeMb: model.estimatedSizeMb,
    status: 'not-downloaded',
    stage: 'idle',
    progress: 0,
    downloadedBytes: diskBytes,
    totalBytes: null,
    selected,
    runtime: runtimeView,
  }
}

async function downloadAssetWithProgress(
  url: string,
  destinationPath: string,
  onProgress: (downloadedBytes: number, totalBytes: number | null) => void,
): Promise<void> {
  const response = await fetch(url, { method: 'GET', redirect: 'follow' })
  if (!response.ok || !response.body) {
    throw new Error(`Download failed (${response.status}): ${url}`)
  }

  await fs.mkdir(dirname(destinationPath), { recursive: true })
  const tempPath = `${destinationPath}.part`
  const total = Number(response.headers.get('content-length') ?? '')
  const totalBytes = Number.isFinite(total) && total > 0 ? total : null

  const writer = createWriteStream(tempPath)
  const reader = response.body.getReader()
  let downloaded = 0

  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      if (!value) continue

      downloaded += value.byteLength
      if (!writer.write(Buffer.from(value))) {
        await once(writer, 'drain')
      }
      onProgress(downloaded, totalBytes)
    }

    writer.end()
    await once(writer, 'finish')
    await fs.rename(tempPath, destinationPath)
  } catch (error) {
    writer.destroy()
    await fs.rm(tempPath, { force: true })
    throw error
  }
}

async function runModelDownload(modelId: VoiceModelId): Promise<void> {
  const model = findModel(modelId)
  const destinationRoot = modelDir(modelId)
  await fs.mkdir(destinationRoot, { recursive: true })

  let baselineBytes = modelDownloadedBytes(model)

  const runtimeNeeded = !(await probeRuntime(model.runtime)).ready
  const missingAssets = model.assets.filter((asset) => !existsSync(modelAssetPath(modelId, asset.fileName)))

  if (!runtimeNeeded && missingAssets.length === 0) {
    activeDownloads.delete(modelId)
    return
  }

  activeDownloads.set(modelId, {
    status: 'downloading',
    stage: runtimeNeeded ? 'installing-runtime' : 'downloading-weights',
    downloadedBytes: baselineBytes,
    totalBytes: null,
    progress: null,
  })

  try {
    // 1. Install the runtime (whisper.cpp / moonshine-voice) if missing.
    if (runtimeNeeded) {
      await installRuntime(model.runtime)
      invalidateRuntimeCache(model.runtime)

      // Re-probe to confirm the install actually put the binary/package
      // on PATH; if not, surface a precise error instead of silently
      // continuing and failing at transcription time.
      const reProbe = await probeRuntime(model.runtime)
      if (!reProbe.ready) {
        throw new Error(
          `Installed the runtime but could not detect it afterwards (${reProbe.label}). ` +
          `Try running manually: ${reProbe.installCommand}`,
        )
      }

      activeDownloads.set(modelId, {
        status: 'downloading',
        stage: 'downloading-weights',
        downloadedBytes: baselineBytes,
        totalBytes: null,
        progress: null,
      })
    }

    // 2. Download missing weight files.
    for (const asset of missingAssets) {
      const destination = modelAssetPath(modelId, asset.fileName)
      await downloadAssetWithProgress(asset.url, destination, (assetBytes, assetTotal) => {
        const state = activeDownloads.get(modelId)
        if (!state || state.status !== 'downloading') return

        const downloadedBytes = baselineBytes + assetBytes
        const progress = assetTotal && assetTotal > 0 ? Math.min(assetBytes / assetTotal, 0.999) : null

        activeDownloads.set(modelId, {
          ...state,
          stage: 'downloading-weights',
          downloadedBytes,
          totalBytes: assetTotal,
          progress,
        })
      })

      baselineBytes = modelDownloadedBytes(model)
      const state = activeDownloads.get(modelId)
      if (state && state.status === 'downloading') {
        activeDownloads.set(modelId, {
          ...state,
          stage: 'downloading-weights',
          downloadedBytes: baselineBytes,
          totalBytes: null,
          progress: null,
        })
      }
    }

    activeDownloads.delete(modelId)
    invalidateRuntimeCache(model.runtime)
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    console.error('[stt][main] model provisioning failed:', message)
    activeDownloads.set(modelId, {
      status: 'error',
      stage: runtimeNeeded ? 'installing-runtime' : 'downloading-weights',
      downloadedBytes: baselineBytes,
      totalBytes: null,
      progress: null,
      errorMessage: message,
    })
  }
}

export function getSelectedVoiceModelId(): VoiceModelId {
  return readSelectedModelId()
}

export function setSelectedVoiceModelId(modelId: VoiceModelId): VoiceModelId {
  findModel(modelId)
  writeConfigPatch({ [VOICE_MODEL_CONFIG_KEY]: modelId })
  return modelId
}

export async function listVoiceModels(): Promise<VoiceModel[]> {
  await cleanupStaleVoiceModelAssets()

  try {
    const selected = readSelectedModelId()
    return Promise.all(MODEL_CATALOG.map((model) => toVoiceModelView(model, selected)))
  } catch (err) {
    console.warn('[stt][main] listVoiceModels fallback:', err instanceof Error ? err.message : err)
    return MODEL_CATALOG.map((model, index) => ({
      id: model.id,
      name: model.name,
      family: model.family,
      description: model.description,
      homepageUrl: model.homepageUrl,
      estimatedSizeMb: model.estimatedSizeMb,
      status: 'not-downloaded' as const,
      stage: 'idle' as const,
      progress: 0,
      downloadedBytes: 0,
      totalBytes: null,
      selected: index === 0,
      runtime: buildEmptyRuntimeView(model.runtime),
    }))
  }
}

const LEGACY_WHISPER_ASSET_NAMES = [
  'model.safetensors',
  'config.json',
  'generation_config.json',
  'merges.txt',
  'preprocessor_config.json',
  'tokenizer.json',
  'tokenizer_config.json',
  'vocab.json',
]

let staleVoiceCleanupPromise: Promise<void> | null = null

async function cleanupStaleVoiceModelAssets(): Promise<void> {
  if (staleVoiceCleanupPromise) return staleVoiceCleanupPromise

  staleVoiceCleanupPromise = (async () => {
    const whisperModels = MODEL_CATALOG.filter((model) => model.family === 'whisper')
    for (const model of whisperModels) {
      const currentAssets = new Set(model.assets.map((asset) => asset.fileName))
      for (const fileName of LEGACY_WHISPER_ASSET_NAMES) {
        if (currentAssets.has(fileName)) continue
        const fullPath = modelAssetPath(model.id, fileName)
        try {
          await fs.stat(fullPath)
          await fs.rm(fullPath, { force: true })
          console.log('[stt][main] removed stale voice model asset:', fullPath)
        } catch {
          // non-fatal
        }
      }
    }
  })()

  return staleVoiceCleanupPromise
}

export async function downloadVoiceModel(modelId: VoiceModelId): Promise<VoiceModel> {
  const model = findModel(modelId)
  const active = activeDownloads.get(modelId)
  if (!active || active.status !== 'downloading') {
    activeDownloads.set(modelId, {
      status: 'downloading',
      stage: 'installing-runtime',
      downloadedBytes: modelDownloadedBytes(model),
      totalBytes: null,
      progress: null,
    })
    void runModelDownload(modelId)
  }

  const selected = readSelectedModelId()
  return toVoiceModelView(model, selected)
}

export async function speakText(text: string): Promise<void> {
  const trimmed = text.trim()
  if (!trimmed) return

  stopSpeaking()
  activeSpeech = spawn('say', [trimmed], {
    stdio: 'ignore',
  })

  activeSpeech.on('exit', () => {
    activeSpeech = null
  })
}

export function stopSpeaking(): void {
  if (!activeSpeech) return
  activeSpeech.kill('SIGTERM')
  activeSpeech = null
}

async function hasBinary(binary: string): Promise<boolean> {
  try {
    const path = await getLoginPath()
    await execFileAsync('bash', ['-lc', `command -v ${binary}`], {
      env: { ...process.env, PATH: path },
    })
    return true
  } catch {
    return false
  }
}

/** Report which transcription backends actually work on this machine.
 *  Web Speech is intentionally *not* advertised — it fails in Electron
 *  (needs a Google Cloud API key that Electron does not ship), and
 *  listing it here mislead users into thinking hold-to-speak would work
 *  out of the box. */
export async function listSttModes(): Promise<string[]> {
  const modes: string[] = []

  const models = await listVoiceModels()
  if (models.some((model) => model.status === 'downloaded')) {
    modes.push('local-model-assets')
  }

  if (await hasBinary('whisper-cli')) {
    modes.push('local-whisper-cli')
  } else if (await hasBinary('whisper-cpp')) {
    modes.push('local-whisper-cpp')
  }

  if (await hasMoonshinePython()) {
    modes.push('local-moonshine-python')
  }

  return modes
}

async function hasMoonshinePython(): Promise<boolean> {
  try {
    await execWithUserPath('python3', ['-c', 'import moonshine_voice'])
    return true
  } catch {
    return false
  }
}

function preferredMoonshineModelPath(): string {
  const selected = readSelectedModelId()
  if (selected === 'moonshine-base-en' && isModelFullyDownloaded(findModel(selected))) {
    return modelDir(selected)
  }

  const fallback = findModel('moonshine-base-en')
  if (isModelFullyDownloaded(fallback)) {
    return modelDir('moonshine-base-en')
  }

  throw new Error('Moonshine model files are not downloaded yet. Download Moonshine Base first.')
}

async function convertToWav(inputPath: string, outputPath: string): Promise<void> {
  await execFileAsync('ffmpeg', [
    '-y',
    '-i',
    inputPath,
    '-ac',
    '1',
    '-ar',
    '16000',
    '-f',
    'wav',
    outputPath,
  ])
}

/** Python bridge to `moonshine_voice`.
 *
 *  The previous version used `model_arch=1` and imported `load_wav_file`
 *  from the top-level module, both of which are wrong for the current
 *  PyPI package. The real API is:
 *    - `ModelArch.BASE` / `ModelArch.TINY` (enum, not int)
 *    - `load_wav_file` lives in `moonshine_voice.utils`
 *    - `transcribe_without_streaming` returns a `Transcript` object with
 *      a `.lines[].text` iterable (listeners are for streaming, not
 *      batch).
 *
 *  See https://mintlify.wiki/moonshine-ai/moonshine/guides/transcription */
async function runMoonshineTranscription(wavPath: string, language: string | undefined): Promise<string> {
  const modelPath = preferredMoonshineModelPath()
  const lang = (language || 'en').trim() || 'en'

  // Arch is inferred from the folder name — the Download UI maps the
  // catalog id (e.g. `moonshine-base-en`) to that folder, and the model
  // id is part of the on-disk path.
  const script = [
    'import json, sys, traceback',
    'try:',
    '    from moonshine_voice import Transcriber, ModelArch',
    '    try:',
    '        from moonshine_voice.utils import load_wav_file',
    '    except ImportError:',
    '        from moonshine_voice import load_wav_file',
    '',
    '    wav_path = sys.argv[1]',
    '    model_path = sys.argv[2]',
    '    language = sys.argv[3] if len(sys.argv) > 3 else "en"',
    '',
    '    lower_path = model_path.lower()',
    '    if "tiny" in lower_path and hasattr(ModelArch, "TINY"):',
    '        arch = ModelArch.TINY',
    '    elif "small" in lower_path and hasattr(ModelArch, "SMALL_STREAMING"):',
    '        arch = ModelArch.SMALL_STREAMING',
    '    elif hasattr(ModelArch, "BASE"):',
    '        arch = ModelArch.BASE',
    '    else:',
    '        # Very old versions exposed only integer model archs.',
    '        arch = 1',
    '',
    '    options = {}',
    '    if language.lower() not in ("en", "english", "es", "spanish"):',
    '        options["max_tokens_per_second"] = "13.0"',
    '',
    '    transcriber = Transcriber(',
    '        model_path=model_path,',
    '        model_arch=arch,',
    '        options=options or None,',
    '    )',
    '    try:',
    '        audio_data, sample_rate = load_wav_file(wav_path)',
    '        transcript = transcriber.transcribe_without_streaming(',
    '            audio_data=audio_data,',
    '            sample_rate=sample_rate,',
    '        )',
    '        lines = getattr(transcript, "lines", None) or []',
    '        text = " ".join((getattr(l, "text", "") or "").strip() for l in lines).strip()',
    '        sys.stdout.write(text)',
    '        sys.stdout.flush()',
    '    finally:',
    '        try:',
    '            transcriber.close()',
    '        except Exception:',
    '            pass',
    'except Exception as exc:',
    '    traceback.print_exc(file=sys.stderr)',
    '    sys.stderr.write("\\nMOONSHINE_ERROR: " + repr(exc) + "\\n")',
    '    sys.exit(1)',
    '',
  ].join('\n')

  console.info('[stt][main] invoking moonshine-voice, model=', modelPath, 'lang=', lang)

  try {
    const { stdout, stderr } = await execWithUserPath('python3', ['-c', script, wavPath, modelPath, lang])
    if (stderr.trim()) {
      console.info('[stt][main] moonshine stderr:\n' + stderr.trim())
    }
    return stdout.trim()
  } catch (err) {
    // execFile throws an Error that also has .stderr/.stdout fields when
    // the subprocess exited non-zero. Surface that so higher layers can
    // render the actual Python traceback instead of a generic failure.
    const e = err as { stderr?: string; stdout?: string; message?: string }
    const detail = (e.stderr ?? '').trim() || (e.stdout ?? '').trim() || e.message || String(err)
    throw new Error(detail)
  }
}

/** Pick a file extension that matches the uploaded mime type so tools that
 *  dispatch on extension (ffmpeg, whisper-cli) don't get confused. */
function extensionFromMime(mime: string | undefined): string {
  if (!mime) return 'bin'
  if (mime.includes('wav')) return 'wav'
  if (mime.includes('webm')) return 'webm'
  if (mime.includes('ogg')) return 'ogg'
  if (mime.includes('mp4') || mime.includes('m4a') || mime.includes('aac')) return 'm4a'
  if (mime.includes('mpeg') || mime.includes('mp3')) return 'mp3'
  if (mime.includes('flac')) return 'flac'
  return 'bin'
}

/** Locate a usable whisper.cpp ggml model file without running the
 *  transcription — so we can surface a good error message early.
 *  Priority order:
 *    1. user-specified `$RAYMES_WHISPER_MODEL`
 *    2. files named `ggml-*.bin` inside userData/voice-models
 *    3. homebrew install location on macOS */
async function findWhisperCliModel(): Promise<string | null> {
  const envPath = process.env['RAYMES_WHISPER_MODEL']
  if (envPath && existsSync(envPath)) return envPath

  // Prefer the currently-selected whisper model so downgrading from
  // `whisper-small` to `whisper-base` in Settings takes effect
  // immediately without requiring a restart.
  const selected = readSelectedModelId()
  const preferredDirs =
    selected === 'whisper-base' || selected === 'whisper-small'
      ? [modelDir(selected), modelDir(selected === 'whisper-base' ? 'whisper-small' : 'whisper-base')]
      : [modelDir('whisper-base'), modelDir('whisper-small')]

  for (const dir of preferredDirs) {
    try {
      const inner = await fs.readdir(dir)
      const match = inner.find((f) => f.startsWith('ggml-') && f.endsWith('.bin'))
      if (match) return join(dir, match)
    } catch {
      // directory may not exist yet — keep looking
    }
  }

  const candidates = [
    '/opt/homebrew/share/whisper-cpp/ggml-base.en.bin',
    '/opt/homebrew/share/whisper-cpp/ggml-base.bin',
    '/usr/local/share/whisper-cpp/ggml-base.en.bin',
    '/usr/local/share/whisper-cpp/ggml-base.bin',
  ]
  for (const c of candidates) {
    if (existsSync(c)) return c
  }
  return null
}

async function runWhisperCli(wavPath: string, language: string | undefined): Promise<string | null> {
  const binary = (await hasBinary('whisper-cli')) ? 'whisper-cli' : (await hasBinary('whisper-cpp')) ? 'whisper-cpp' : null
  if (!binary) return null

  const model = await findWhisperCliModel()
  if (!model) return null

  const args = ['-m', model, '-f', wavPath, '-l', language?.trim() || 'en', '-otxt', '-of', wavPath.replace(/\.wav$/, '')]
  console.info('[stt][main] whisper-cli:', binary, args.join(' '))
  try {
    const { stderr } = await execWithUserPath(binary, args)
    if (stderr.trim()) console.info('[stt][main] whisper-cli stderr:\n' + stderr.trim())
    const txtPath = wavPath.replace(/\.wav$/, '.txt')
    const text = await fs.readFile(txtPath, 'utf-8').catch(() => '')
    await fs.rm(txtPath, { force: true }).catch(() => undefined)
    return text.trim()
  } catch (err) {
    const e = err as { stderr?: string; message?: string }
    const detail = (e.stderr ?? '').trim() || e.message || String(err)
    console.warn('[stt][main] whisper-cli failed:', detail)
    throw new Error(detail)
  }
}

type TranscribeOutcome = {
  ok: true
  text: string
  engine: string
} | {
  ok: false
  error: string
  hint?: string
}

/** Transcribe the audio blob coming from the renderer. Strategy:
 *
 *   1. If the payload is already 16 kHz mono WAV (the renderer encodes
 *      it this way whenever Web Audio is usable), skip ffmpeg and feed
 *      the WAV straight to the engine.
 *   2. Otherwise, if ffmpeg is available, convert to 16 kHz mono WAV.
 *   3. Try engines in order: whisper-cli, moonshine-python.
 *   4. Return a structured error with an install hint if nothing works —
 *      the renderer surfaces this verbatim so the user knows what's wrong
 *      instead of silently cycling the mic forever. */
type EngineProbe = {
  python3: string
  whisper: string
  moonshine: string
  cachedAt: number
}

let cachedEngineProbe: EngineProbe | null = null
const ENGINE_PROBE_TTL_MS = 5 * 60 * 1000

async function probeEngineBinaries(): Promise<EngineProbe> {
  if (cachedEngineProbe && Date.now() - cachedEngineProbe.cachedAt < ENGINE_PROBE_TTL_MS) {
    return cachedEngineProbe
  }
  const [whichPython, whichWhisper, moonshinePath] = await Promise.all([
    execWithUserPath('bash', ['-lc', 'command -v python3 || true'])
      .then((r) => r.stdout.trim())
      .catch(() => ''),
    execWithUserPath('bash', ['-lc', 'command -v whisper-cli || command -v whisper-cpp || true'])
      .then((r) => r.stdout.trim())
      .catch(() => ''),
    execWithUserPath('python3', ['-c', 'import moonshine_voice, sys; sys.stdout.write(moonshine_voice.__file__)'])
      .then((r) => r.stdout.trim())
      .catch(() => ''),
  ])
  cachedEngineProbe = {
    python3: whichPython,
    whisper: whichWhisper,
    moonshine: moonshinePath,
    cachedAt: Date.now(),
  }
  return cachedEngineProbe
}

export async function transcribeAudio(req: VoiceTranscribeRequest): Promise<TranscribeOutcome> {
  const tempRoot = join(app.getPath('temp'), 'tezbar-voice')
  await fs.mkdir(tempRoot, { recursive: true })

  const token = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
  const ext = extensionFromMime(req.mimeType)
  const sourcePath = join(tempRoot, `input-${token}.${ext}`)
  const wavPath = ext === 'wav' ? sourcePath : join(tempRoot, `input-${token}.wav`)

  try {
    await fs.writeFile(sourcePath, Buffer.from(req.audioBytes))
    console.info(
      '[stt][main] received audio',
      JSON.stringify({
        bytes: req.audioBytes.byteLength,
        mime: req.mimeType ?? 'unknown',
        language: req.language ?? 'auto',
      }),
    )

    // Diagnostic probe — cached for 5 minutes so repeated dictation
    // requests don't shell out every time.
    try {
      const probe = await probeEngineBinaries()
      console.info('[stt][main] engine probe', JSON.stringify(probe))
    } catch (err) {
      console.info('[stt][main] engine probe skipped:', err instanceof Error ? err.message : err)
    }

    if (ext !== 'wav') {
      if (!(await hasBinary('ffmpeg'))) {
        return {
          ok: false,
          error: 'Audio arrived in a compressed format but ffmpeg is not installed.',
          hint: 'Install ffmpeg (`brew install ffmpeg`) or enable Web Audio encoding in the renderer.',
        }
      }
      await convertToWav(sourcePath, wavPath)
    }

    // Track the first engine failure so we can surface the actual cause
    // (Python traceback, whisper-cli stderr) when every engine fails.
    let firstFailure: { engine: string; message: string } | null = null

    // 1. whisper-cli (cheapest to set up on macOS: `brew install whisper-cpp`)
    const whisperAvailable = (await hasBinary('whisper-cli')) || (await hasBinary('whisper-cpp'))
    if (whisperAvailable) {
      try {
        const whisperText = await runWhisperCli(wavPath, req.language)
        if (whisperText && whisperText.length > 0) {
          console.info('[stt][main] whisper-cli produced', whisperText.length, 'chars')
          return { ok: true, text: whisperText, engine: 'whisper-cli' }
        }
        if (whisperText !== null && !firstFailure) {
          firstFailure = { engine: 'whisper.cpp', message: 'whisper-cli returned no text.' }
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        console.warn('[stt][main] whisper-cli threw:', message)
        if (!firstFailure) firstFailure = { engine: 'whisper.cpp', message }
      }
    }

    // 2. Moonshine via Python
    const moonshineAvailable = (await hasBinary('python3')) && (await hasMoonshinePython())
    if (moonshineAvailable) {
      try {
        const text = await runMoonshineTranscription(wavPath, req.language)
        if (text.length > 0) {
          console.info('[stt][main] moonshine produced', text.length, 'chars')
          return { ok: true, text, engine: 'moonshine-python' }
        }
        if (!firstFailure) {
          firstFailure = { engine: 'Moonshine', message: 'Moonshine returned no text.' }
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        console.warn('[stt][main] moonshine failed:', message)
        if (!firstFailure) firstFailure = { engine: 'Moonshine', message }
      }
    }

    if (firstFailure) {
      // An engine was installed and callable but still failed. Showing
      // the underlying error is far more useful than pretending nothing
      // was configured — the user can paste it back and we can fix the
      // actual issue.
      return {
        ok: false,
        error: `${firstFailure.engine} failed to transcribe the recording.`,
        hint: firstFailure.message,
      }
    }

    return {
      ok: false,
      error: 'No local speech-to-text engine is available.',
      hint:
        'Open Settings → Voice models and click "Install & download" on a model. ' +
        'TezBar will install the required runtime (whisper.cpp via Homebrew or Moonshine via pip) and the weights in one step.',
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    console.error('[stt][main] transcription pipeline error:', message)
    return { ok: false, error: message }
  } finally {
    await fs.rm(sourcePath, { force: true }).catch(() => undefined)
    if (wavPath !== sourcePath) {
      await fs.rm(wavPath, { force: true }).catch(() => undefined)
    }
  }
}

/** Back-compat alias so any lingering callers keep working. */
export async function transcribeWithMoonshine(req: VoiceTranscribeRequest): Promise<{
  text: string
  engine: string
}> {
  const result = await transcribeAudio(req)
  if (!result.ok) {
    throw new Error(result.hint ? `${result.error}\n${result.hint}` : result.error)
  }
  return { text: result.text, engine: result.engine }
}
