#!/usr/bin/env node
// Prepare the pinned native helper resources before packaging Tezbar.
// Bundle mapping required: ../native/audio-device -> native/audio-device.
import { createHash } from 'node:crypto';
import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const destination = path.join(root, 'native', 'audio-device');
const resources = {
  darwin: [
    {
      name: 'audio-devices',
      url: 'https://raw.githubusercontent.com/raycast/extensions/668e9109768f48a76a8e09b6f06d804b4e8b2068/extensions/audio-device/assets/audio-devices',
      sha256: '638e4535c28f5fa8f9b8720ab63f672c8ce329dfcd5e7e01f11fbb3851a8f537',
    },
    {
      name: 'sound-control',
      url: 'https://raw.githubusercontent.com/raycast/extensions/fc737c076e1698e51f0378cd46293358e019ee91/extensions/audio-device/assets/sound-control',
      sha256: '0f7d825d5fe08476b7efe881c79d4b391352ffa625891cb6df9d59853e1c7c88',
    },
  ],
  win32: [{
    name: 'win-audio-cli.exe',
    url: 'https://github.com/Inovvia/go-win-audio-cli/releases/download/1.3.1/win-audio-cli.exe',
    sha256: '9bda285caea2477f5504a73b532d4102e5d8e37a63c16669faca1834ccfd2048',
  }],
};
const maxBytes = 16 * 1024 * 1024;

function verify(bytes, resource) {
  const actual = createHash('sha256').update(bytes).digest('hex');
  if (actual !== resource.sha256) {
    throw new Error(`${resource.name} checksum mismatch: expected ${resource.sha256}, got ${actual}`);
  }
}

async function prepare(resource, dir = destination, fetchImpl = fetch) {
  const target = path.join(dir, resource.name);
  try {
    verify(await readFile(target), resource);
    return target;
  } catch (error) {
    if (error?.code !== 'ENOENT' && !String(error?.message).includes('checksum mismatch')) throw error;
  }
  await mkdir(dir, { recursive: true });
  let url = resource.url;
  let response;
  for (let redirects = 0; redirects <= 5; redirects++) {
    if (new URL(url).protocol !== 'https:') throw new Error(`${resource.name} requires HTTPS`);
    response = await fetchImpl(url, { redirect: 'manual', signal: AbortSignal.timeout(30_000) });
    if (![301, 302, 303, 307, 308].includes(response.status)) break;
    if (redirects === 5) throw new Error(`${resource.name} redirected too many times`);
    const location = response.headers?.get('location');
    if (!location) throw new Error(`${resource.name} redirect has no location`);
    url = new URL(location, url).href;
  }
  if (!response.ok || new URL(response.url || url).protocol !== 'https:') {
    throw new Error(`${resource.name} download failed or redirected away from HTTPS (${response.status})`);
  }
  const declaredSize = Number(response.headers?.get('content-length'));
  if (declaredSize > maxBytes) throw new Error(`${resource.name} exceeds size limit`);
  let bytes;
  if (response.body?.getReader) {
    const reader = response.body.getReader();
    const chunks = [];
    let size = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > maxBytes) {
        await reader.cancel();
        throw new Error(`${resource.name} exceeds size limit`);
      }
      chunks.push(value);
    }
    bytes = Buffer.concat(chunks, size);
  } else {
    // Supports a simple fake response in the self-test.
    bytes = Buffer.from(await response.arrayBuffer());
    if (bytes.length > maxBytes) throw new Error(`${resource.name} exceeds size limit`);
  }
  verify(bytes, resource);
  const temporary = `${target}.${process.pid}.download`;
  try {
    await writeFile(temporary, bytes, { mode: 0o700 });
    await rm(target, { force: true });
    await rename(temporary, target);
  } finally {
    await rm(temporary, { force: true });
  }
  return target;
}

async function selfTest() {
  const { mkdtemp } = await import('node:fs/promises');
  const { tmpdir } = await import('node:os');
  const dir = await mkdtemp(path.join(tmpdir(), 'tezbar-audio-'));
  const bytes = Buffer.from('fake-native-helper-fixture');
  const descriptor = { name: 'fixture.exe', url: 'https://example.test/fixture.exe', sha256: createHash('sha256').update(bytes).digest('hex') };
  let calls = 0;
  const fakeFetch = async () => { calls++; return { ok: true, status: 200, url: descriptor.url, headers: new Headers(), arrayBuffer: async () => bytes }; };
  try {
    assert.equal(await prepare(descriptor, dir, fakeFetch), path.join(dir, descriptor.name));
    assert.equal(await prepare(descriptor, dir, fakeFetch), path.join(dir, descriptor.name));
    assert.equal(calls, 1);
    assert.deepEqual(await readFile(path.join(dir, descriptor.name)), bytes);
    await writeFile(path.join(dir, descriptor.name), 'corrupt');
    await assert.rejects(prepare(descriptor, dir, async () => ({ ...await fakeFetch(), arrayBuffer: async () => Buffer.from('wrong') })), /checksum mismatch/);
    assert.equal((await readFile(path.join(dir, descriptor.name))).toString(), 'corrupt');
    await rm(path.join(dir, descriptor.name));
    await assert.rejects(prepare(descriptor, dir, async () => { throw new Error('offline'); }), /offline/);
    console.log('Native V2 audio provisioning self-test passed');
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.includes('--self-test')) {
    await selfTest();
  } else {
    const platform = process.argv.includes('--all') ? ['darwin', 'win32'] : [process.platform];
    for (const name of platform) {
      if (!resources[name]) throw new Error(`Audio helpers are unavailable for ${name}`);
      for (const resource of resources[name]) console.log(await prepare(resource));
    }
  }
}

export { prepare, resources };
