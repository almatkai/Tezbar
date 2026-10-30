#!/usr/bin/env python3
"""Single-variable experiments against the pinned, cloned runtime, never installs.

Run separately from desktop timings. The production supervisor owns each model
process; closing its lease must terminate/reap the owned tree. No global cache
flushing or pressure generation. Outputs are immutable, including failed runs.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import socket
import shutil
import struct
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from desktop import processes, summarize


CONFIGS = {
    'baseline': [],
    'ubatch2048': ['--ubatch-size', '2048'],
    'no-warmup': ['--no-warmup'],
    'poll0': ['--poll', '0', '--poll-batch', '0'],
    'http1': ['--threads-http', '1'],
    'flash-on': ['--flash-attn', 'on'],
    'no-residency': [],
}
ENVS = {'no-residency': {'GGML_METAL_NO_RESIDENCY': '1'}}


def experimental_library(directory, library, rewrite_manifest=False):
    """Change only a disposable clone, identifying every changed library alias.

    A regenerated fixture manifest is for this explicit experiment, never a
    production artifact identity or an installed-profile migration.
    """
    directory = Path(directory).resolve()
    roots = [Path(tempfile.gettempdir()).resolve(), Path('/tmp').resolve()]
    if not any(root in directory.parents for root in roots):
        raise ValueError('experimental libraries require a temporary clone')
    target = next(directory.glob('runtime/**/libllama.0.6.0.dylib'))
    if directory not in target.resolve().parents:
        raise ValueError('runtime library alias escapes temporary clone')
    result = dict(path=str(library.resolve()), original_sha256=hashlib.sha256(target.read_bytes()).hexdigest(),
                  sha256=hashlib.sha256(library.read_bytes()).hexdigest(), rewritten_fixture_manifest=rewrite_manifest)
    shutil.copy2(library, target)
    if rewrite_manifest:
        runtime = directory/'runtime'
        manifest_path = directory/'installed.json'
        manifest = json.loads(manifest_path.read_text())
        assert manifest['schema'] == 2
        files, digests = {}, {}
        for item in sorted(runtime.rglob('*')):
            if not item.is_file():
                continue
            resolved = item.resolve()
            if runtime.resolve() not in resolved.parents:
                raise ValueError('runtime alias escapes temporary fixture')
            if resolved not in digests:
                digests[resolved] = hashlib.sha256(item.read_bytes()).hexdigest()
            files[item.relative_to(runtime).as_posix()] = dict(bytes=item.stat().st_size, sha256=digests[resolved])
        manifest['runtimeFiles'] = files
        manifest_path.write_text(json.dumps(manifest, indent=2)+'\n')
        result['fixture_manifest_sha256'] = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    return result


def capture(path, args):
    p = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    path.write_text(p.stdout)
    return dict(path=str(path), code=p.returncode)


def pressure():
    return {name: subprocess.run(args, capture_output=True, text=True).stdout for name, args in [
        ('pressure', ['memory_pressure', '-Q']),
        ('swap', ['sysctl', 'vm.swapusage']),
        ('pages', ['vm_stat']),
    ]}


def vector_bytes(rows):
    values = bytearray()
    for row in rows:
        v = row['embedding'][:256]
        scale = max(map(abs, v))
        magnitude = math.sqrt(sum((x / scale) ** 2 for x in v))
        values.extend(struct.pack('<256f', *(x / scale / magnitude for x in v)))
    return bytes(values)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--model', required=True)
    p.add_argument('--supervisor', required=True)
    p.add_argument('--config', choices=CONFIGS, required=True)
    p.add_argument('--threads', type=int, default=8)
    p.add_argument('--output', required=True)
    p.add_argument('--idle', type=float, default=12)
    p.add_argument('--normal-only', action='store_true', help='Skip large-input boundary tests for repeated speed-mode checks')
    p.add_argument('--library', type=Path, help='Experimental libllama replacement in this temporary clone only')
    p.add_argument('--retention', type=float, default=0, help='Additional quiet interval before a first-query/recovery probe')
    a = p.parse_args()
    out = Path(a.output).resolve()
    out.mkdir(exist_ok=False)
    clone = Path(tempfile.mkdtemp(prefix='tezbar-model-memory-')) / 'model'
    subprocess.run(['cp', '-cR', a.model, str(clone)], check=True)
    binary = next(clone.glob('runtime/**/llama-server'))
    model = next(clone.glob('*.gguf'))
    library = None
    if a.library:
        library = experimental_library(clone, a.library)
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        port = s.getsockname()[1]
    endpoint = f'http://127.0.0.1:{port}'
    # Ephemeral fixture token, never recorded in command-line/process captures.
    token = os.urandom(32).hex()
    args = [str(Path(a.supervisor).resolve()), '--tezbar-private-child-supervisor-v1', str(binary),
            '--model', str(model), '--embedding', '--pooling', 'mean', '--host', '127.0.0.1',
            '--port', str(port), '--api-key', token, '--ctx-size', '8192', '--batch-size', '8192',
            '--ubatch-size', '8192', '--parallel', '1', '--threads', str(a.threads)] + CONFIGS[a.config]
    result = dict(config=a.config, threads=a.threads, flags=CONFIGS[a.config], root=str(clone.parent),
                  runtime_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  model_sha256=hashlib.sha256(model.read_bytes()).hexdigest(),
                  supervisor_sha256=hashlib.sha256(Path(a.supervisor).read_bytes()).hexdigest(),
                  experimental_library=library,
                  pressure_before=pressure(), completed=False, workloads=[], samples=[])
    vectors = {}
    start = time.monotonic()
    log = (out / 'server.log').open('w')
    result['child_environment'] = ENVS.get(a.config, {})
    child = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=log,
                             env={**os.environ, **ENVS.get(a.config, {})})
    result['popen_ms'] = (time.monotonic()-start)*1000
    result['health_attempts'] = []

    def request(route, body=None):
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(endpoint + route, data=data,
            headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'})
        with urllib.request.urlopen(req, timeout=120) as r:
            return json.load(r)

    def sample(label):
        result['samples'].append(dict(t=time.monotonic()-start, phase=label, processes=processes(child.pid)))

    def idle(label, seconds=None):
        sample(label)
        until = time.monotonic() + (a.idle if seconds is None else seconds)
        while time.monotonic() < until:
            time.sleep(min(.5, max(0, until-time.monotonic())))
            sample(label)
        result[label] = summarize([s for s in result['samples'] if s['phase'] == label])

    def embed(label, inputs, repeats=1):
        row = dict(name=label, inputs=len(inputs), timings_ms=[], tokens=[])
        sample(label)
        try:
            row['tokens'] = [len(request('/tokenize', {'content': x, 'add_special': True})['tokens']) for x in inputs]
            for _ in range(repeats):
                begin = time.monotonic()
                reply = request('/v1/embeddings', {'input': inputs, 'encoding_format': 'float'})
                row['timings_ms'].append((time.monotonic()-begin)*1000)
                rows = sorted(reply['data'], key=lambda x: x['index'])
                assert len(rows) == len(inputs) and all(len(x['embedding']) == 768 for x in rows)
                raw = json.dumps(rows, separators=(',', ':')).encode()
                digest = hashlib.sha256(raw).hexdigest()
                if label in vectors:
                    assert vectors[label] == rows, 'unstable repeated embeddings'
                vectors[label] = rows
                row['raw_sha256'] = digest
                row['normalized_f32_sha256'] = hashlib.sha256(vector_bytes(rows)).hexdigest()
                row['usage'] = reply.get('usage')
                sample(label)
        except urllib.error.HTTPError as e:
            row['error'] = dict(status=e.code, body=e.read().decode())
        result['workloads'].append(row)
        print(label, row, flush=True)

    try:
        deadline = start + 90
        while True:
            if child.poll() is not None:
                raise RuntimeError('runtime exited during startup')
            attempt_start = time.monotonic()
            if (out/'server.log').stat().st_size and 'first_log_observed_ms' not in result:
                result['first_log_observed_ms'] = (attempt_start-start)*1000
            sample('startup')
            try:
                request('/health')
                result['health_attempts'].append(dict(start_ms=(attempt_start-start)*1000, duration_ms=(time.monotonic()-attempt_start)*1000, ok=True))
                break
            except (OSError, urllib.error.HTTPError) as error:
                result['health_attempts'].append(dict(start_ms=(attempt_start-start)*1000, duration_ms=(time.monotonic()-attempt_start)*1000, ok=False, error=type(error).__name__))
                if time.monotonic() > deadline:
                    raise RuntimeError('startup deadline')
                time.sleep(.05)
        result['ready_ms'] = (time.monotonic()-start)*1000
        result['props'] = request('/props')
        idle('ready_idle')
        embed('query', ['task: search result | query: orchards apples pears'], 12)
        # Production chunk envelope: 1200 UTF-16 units plus a long file title.
        # Diverse scripts/punctuation exercise tokenizer expansion without truncation.
        chunk = (('Runtime fixture. Orchards contain apples and pears. '*24)[:1200]
                 if a.normal_only else ''.join(chr(0x4e00+i) for i in range(1200)))
        inputs = ['title: ' + ('Long file title ' * 15) + str(i) + ' | text: ' + chunk for i in range(16)]
        for n in [4, 8, 16]:
            embed('chunks'+str(n), inputs[:n], 2)
        idle('indexed_idle')
        if a.retention:
            embed('retention-primer', ['task: search result | query: orchards apples pears'])
            idle('retention_idle', a.retention)
            embed('retention-first-query', ['task: search result | query: orchards apples pears'])
            embed('retention-recovery', ['task: search result | query: orchards apples pears'], 5)
            assert vectors['retention-first-query'] == vectors['retention-primer'] == vectors['retention-recovery']
        # The app has no foreground character cap. Preserve its 8192-token context.
        # Repeated ASCII words give a stable, exact tokenizer boundary, including BOS.
        for n in ([] if a.normal_only else [2048, 4096, 8192, 8193]):
            text = 'a ' * (n-3) + 'a'
            actual = len(request('/tokenize', {'content': text, 'add_special': True})['tokens'])
            assert actual == n, (n, actual)
            embed('sequence'+str(n), [text])
        idle('long_input_idle')
        for proc in processes(child.pid):
            if proc['name'] == 'llama-server':
                result['vmmap'] = capture(out/'model-vmmap.txt', ['vmmap', '-wide', str(proc['pid'])])
                result['footprint'] = capture(out/'model-footprint.txt', ['footprint', '--swapped', '--wired', str(proc['pid'])])
        result['completed'] = True
    finally:
        result['pressure_after'] = pressure()
        if child.stdin:
            child.stdin.close()
        child.wait(timeout=15)
        result['exit_code'] = child.returncode
        result['owned_survivors'] = processes(child.pid)
        log.close()
        (out/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        (out/'vectors.json').write_text(json.dumps(vectors)+'\n')
    print('summary', result['ready_ms'], result.get('indexed_idle'), flush=True)


if __name__ == '__main__':
    main()
