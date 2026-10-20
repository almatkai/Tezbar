import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { resolve, join } from 'node:path'
import { FrameReader } from '../src/main/extensionWorker/framing'

// An actual compatibility-process smoke test. It uses a temporary extension
// and an in-memory host; it never reads the system clipboard or user profile.
async function main(): Promise<void> {
  const root = await mkdtemp(join(tmpdir(), 'tezbar-extension-worker-test-'))
  const extension = join(root, 'extensions', 'protocol-fixture')
  await mkdir(join(extension, '.sc-build'), { recursive: true })
  await writeFile(
    join(extension, 'package.json'),
    JSON.stringify({
      name: 'protocol-fixture',
      title: 'Protocol Fixture',
      platforms: ['macOS', 'Windows', 'Linux'],
      commands: ['run', 'paste', 'image', 'html', 'failure', 'paste-action'].map((name) => ({
        name,
        title: name,
        mode: name === 'paste-action' ? 'view' : 'no-view',
      })),
    })
  )
  await writeFile(
    join(extension, '.sc-build', 'run.js'),
    `
    const { Clipboard, AI } = require('@raycast/api');
    exports.default = async () => {
      console.log('extension log must not corrupt stdout');
      const text = await Clipboard.readText();
      const answer = await AI.ask(text);
      await Clipboard.copy(text + answer);
    };
  `
  )
  const pngPath = join(root, 'horse.png')
  await writeFile(
    pngPath,
    Buffer.from(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aRZkAAAAASUVORK5CYII=',
      'base64'
    )
  )
  for (const [name, code] of Object.entries({
    paste: "await Clipboard.paste('pasted Қаза🐎')",
    image: `await Clipboard.copy({ file: ${JSON.stringify(pngPath)} })`,
    html: "await Clipboard.copy({ text: 'hello', html: '<b>hello</b>' })",
    failure: "await Clipboard.copy('reject this write')",
  })) {
    await writeFile(
      join(extension, '.sc-build', `${name}.js`),
      `const { Clipboard } = require('@raycast/api'); exports.default = async () => { ${code} };`
    )
  }
  await writeFile(
    join(extension, '.sc-build', 'paste-action.js'),
    `
    const React = require('react');
    const { List, ActionPanel, Action } = require('@raycast/api');
    exports.default = () => React.createElement(List, null,
      React.createElement(List.Item, { title: 'Paste', actions:
        React.createElement(ActionPanel, null, React.createElement(Action.Paste, { content: 'pasted Қаза🐎' }))
      }));
  `
  )
  const child = spawn(
    process.env.TEZBAR_TEST_BUN || 'bun',
    [resolve('dist-backend/extension-worker.js')],
    {
      env: {
        ...process.env,
        APPDATA_DIR: root,
        TEZBAR_CONFIG_DIR: join(root, 'config'),
        TEZBAR_EXTENSION_WORKER: '1',
      },
      stdio: ['pipe', 'pipe', 'pipe'],
    }
  )
  let stderr = ''
  child.stderr.on('data', (chunk) => {
    stderr = (stderr + String(chunk)).slice(-8192)
  })
  const replies = new Map<
    string,
    { resolve: (value: unknown) => void; reject: (reason: Error) => void }
  >()
  let clipboard = ''
  let original = ''
  let readyResolve!: () => void
  let readyReject!: (error: Error) => void
  const ready = new Promise<void>((resolve, reject) => {
    readyResolve = resolve
    readyReject = reject
  })
  const send = (value: unknown): void => {
    child.stdin.write(`${JSON.stringify(value)}\n`)
  }
  const requests: string[] = []
  let protocolFailure: Error | undefined
  const fail = (error: Error): void => {
    protocolFailure = error
    readyReject(error)
    for (const pending of replies.values()) pending.reject(error)
    replies.clear()
  }
  const reader = new FrameReader((line) => {
    let frame: Record<string, any>
    try {
      frame = JSON.parse(line)
    } catch {
      fail(new Error(`Invalid protocol stdout: ${line}`))
      return
    }
    if (frame.type === 'ready') {
      assert.equal(frame.protocol, 1)
      readyResolve()
      return
    }
    if (frame.type === 'core-request') {
      requests.push(frame.channel)
      let result: unknown
      if (frame.channel === 'core:apps:list') result = []
      else if (frame.channel === 'clipboard:read') result = clipboard
      else if (frame.channel === 'clipboard:write') {
        if (frame.payload[0] === 'reject this write') {
          send({ type: 'core-reply', id: frame.id, error: 'fixture write rejected' })
          return
        }
        clipboard = frame.payload[0]
        result = { ok: true }
      } else if (frame.channel === 'core:platform:paste') {
        assert.equal(clipboard, 'pasted Қаза🐎')
        result = { ok: true }
      } else if (frame.channel === 'core:platform:clipboard-write') {
        assert.ok(frame.payload[0].file === pngPath || frame.payload[0].html === '<b>hello</b>')
        result = { ok: true }
      } else if (frame.channel === 'core:ai:extension-ask') {
        assert.equal(frame.payload[0], original)
        result = ' ✓'
      } else {
        send({
          type: 'core-reply',
          id: frame.id,
          error: `Unexpected host request: ${frame.channel}`,
        })
        return
      }
      send({ type: 'core-reply', id: frame.id, result })
    }
    if (frame.type === 'reply') {
      const pending = replies.get(frame.id)
      replies.delete(frame.id)
      if (frame.error) pending?.reject(new Error(String(frame.error)))
      else pending?.resolve(frame.result)
    }
  })
  child.stdin.on('error', fail)
  child.stdout.on('data', (chunk: Buffer) => {
    try {
      reader.push(chunk)
    } catch (error) {
      fail(error instanceof Error ? error : new Error(String(error)))
    }
  })
  child.stdout.on('end', () => {
    try {
      reader.finish()
    } catch (error) {
      fail(error instanceof Error ? error : new Error(String(error)))
    }
  })
  child.on('error', (error) => {
    readyReject(error)
    for (const pending of replies.values()) pending.reject(error)
  })
  const exited = new Promise<number | null>((resolve) =>
    child.on('exit', (code) => {
      const error = new Error(`Worker exited (${code}): ${stderr}`)
      readyReject(error)
      for (const pending of replies.values()) pending.reject(error)
      replies.clear()
      resolve(code)
    })
  )
  let sequence = 0
  async function invoke(channel: string, ...payload: unknown[]): Promise<any> {
    if (protocolFailure) throw protocolFailure
    const id = String(++sequence)
    const reply = new Promise((resolve, reject) => {
      replies.set(id, { resolve, reject })
      send({ type: 'invoke', id, channel, payload })
    })
    return deadline(reply)
  }
  try {
    await deadline(ready)
    for (const text of [
      'ASCII clipboard',
      'Қазақша\u2028日本語🐎\u2029',
      'escaped \r\n and \u0000',
    ]) {
      clipboard = original = text
      requests.length = 0
      const result = await invoke('extension:run-command', {
        extensionId: 'raycast.protocol-fixture',
        commandName: 'run',
      })
      assert.equal(result.ok, true, result.message)
      assert.equal(result.mode, 'no-view')
      assert.equal(clipboard, `${original} ✓`)
      assert.deepEqual(requests, [
        'core:apps:list',
        'clipboard:read',
        'core:ai:extension-ask',
        'clipboard:write',
      ])
    }
    for (const [commandName, expected] of [
      ['paste', ['core:apps:list', 'clipboard:write', 'core:platform:paste']],
      ['image', ['core:apps:list', 'core:platform:clipboard-write']],
      ['html', ['core:apps:list', 'core:platform:clipboard-write']],
    ] as const) {
      requests.length = 0
      const result = await invoke('extension:run-command', {
        extensionId: 'raycast.protocol-fixture',
        commandName,
      })
      assert.equal(result.ok, true, result.message)
      assert.deepEqual(requests, expected)
    }
    const view = await invoke('extension:run-command', {
      extensionId: 'raycast.protocol-fixture',
      commandName: 'paste-action',
    })
    assert.equal(view.ok, true, view.message)
    assert.equal(view.mode, 'view')
    requests.length = 0
    const action = await invoke('extension:invoke-action', {
      sessionId: view.sessionId,
      actionId: view.actions[0].id,
    })
    assert.equal(action.ok, true, action.message)
    assert.deepEqual(requests, ['clipboard:write', 'core:platform:paste'])
    await invoke('extension:dispose-session', { sessionId: view.sessionId })
    const failure = await invoke('extension:run-command', {
      extensionId: 'raycast.protocol-fixture',
      commandName: 'failure',
    })
    assert.equal(failure.ok, false)
    assert.match(failure.message, /fixture write rejected/)
    await assert.rejects(invoke('search:all', 'private profile data'), /does not handle search:all/)
    child.stdin.end()
    assert.equal(await deadline(exited), 0)
    console.log(
      'Extension Worker: actual Bun process, ASCII/Unicode, clipboard/AI, image/HTML, paste, host errors, boundary rejection and EOF cleanup passed.'
    )
  } catch (error) {
    throw new Error(
      `${String(error)}\nHost requests: ${requests.join(', ')}\nWorker stderr:\n${stderr}`
    )
  } finally {
    child.kill()
    await rm(root, { recursive: true, force: true })
  }
}

async function deadline<T>(promise: Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error('Worker test timed out')), 10_000)
      }),
    ])
  } finally {
    if (timer) clearTimeout(timer)
  }
}

void main().catch((error) => {
  console.error(error)
  process.exitCode = 1
})
