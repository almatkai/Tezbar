#!/usr/bin/env python3
"""Ordinary release executable + isolated installed-model upgrade, no bench override."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import signal
import subprocess
import tempfile
import time
from desktop import prepare, processes, scrub_fixture_clipboard


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--app', type=Path, required=True, help='Actually packaged ordinary Tezbar.app')
    p.add_argument('--template-result', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    a.output.mkdir(exist_ok=False)
    repo = Path(__file__).resolve().parents[2]
    pin = json.loads((repo/'src-tauri/runtime/embedding-macos-arm64.json').read_text())
    template = Path(json.loads(a.template_result.read_text())['root'])
    root = Path(tempfile.mkdtemp(prefix='tb-ord-'))
    prepare(root)
    shutil.rmtree(root/'data')
    subprocess.run(['cp', '-cR', str(template/'data'), str(root/'data')], check=True)
    shutil.copytree(template/'config', root/'config', dirs_exist_ok=True)
    # Start without an app runtime cache: prove offline first activation from
    # untouched old installed.json, not a fixture-generated replacement manifest.
    assert not (root/'data/knowledge/runtimes').exists()
    manifests = {str(f.relative_to(root)): sha(f) for f in (root/'data/knowledge/models').glob('*/installed.json')}
    app = root/'Tezbar Ordinary Fixture.app'
    subprocess.run(['cp', '-cR', str(a.app.resolve()), str(app)], check=True)
    plist = app/'Contents/Info.plist'
    info = plistlib.loads(plist.read_bytes())
    info['CFBundleIdentifier'] = 'com.tezbar.performance.ordinary-fixture'
    plist.write_bytes(plistlib.dumps(info))
    executable = app/'Contents/MacOS/tezbar'
    raw = executable.read_bytes()
    assert all(x not in raw for x in [b'TEZBAR_BENCH_HOME', b'__desktop_bench_reply'])
    result = dict(root=str(root), executable_sha256=sha(executable), completed=False, samples=[])
    pid = None
    start = time.monotonic()
    try:
        # Refuse to involve any running installed/other Tezbar; never stop one.
        rows = subprocess.check_output(['ps','-axo','pid=,comm='],text=True).splitlines()
        if any(line.strip().endswith('/Contents/MacOS/tezbar') for line in rows):
            raise RuntimeError('Another Tezbar is running; refusing ordinary-identity launch')
        subprocess.run(['open','-n','-g','-o',str(root/'stdout.log'),'--stderr',str(root/'stderr.log'),str(app),
            '--env','TEZBAR_DATA_DIR='+str(root/'data'),'--env','TEZBAR_CONFIG_DIR='+str(root/'config')],check=True)
        deadline = time.monotonic()+90
        model = None
        while time.monotonic()<deadline:
            rows = subprocess.check_output(['ps','-axo','pid=,comm='],text=True).splitlines()
            exact = [int(line.strip().split(None,1)[0]) for line in rows if line.strip().endswith(str(executable))]
            if exact:
                pid = exact[0]
                family = processes(pid)
                result['samples'].append(dict(t=time.monotonic()-start,processes=family))
                model = next((p for p in family if p['name']=='llama-server'), None)
                if model:
                    break
            time.sleep(.2)
        if not model:
            raise RuntimeError('Ordinary app did not activate model')
        result['model_first_observed_ms'] = (time.monotonic()-start)*1000
        # This is not a health/first-query timer; the benchmark app measures that.
        time.sleep(3)
        library_map = subprocess.check_output(['vmmap','-wide',str(model['pid'])],text=True,stderr=subprocess.STDOUT)
        (a.output/'model-vmmap.txt').write_text(library_map)
        expected = root/'data/knowledge/runtimes'/pin['archiveSha256']
        assert str(expected/'libllama.0.dylib') in library_map or str(expected/'libllama.0.6.0.dylib') in library_map
        actual = {}
        for f in expected.iterdir():
            assert f.is_file() and not f.is_symlink()
            actual[f.name] = dict(bytes=f.stat().st_size,sha256=sha(f))
        assert actual == pin['files']
        assert manifests == {str(f.relative_to(root)):sha(f) for f in (root/'data/knowledge/models').glob('*/installed.json')}
        assert not (root/'control.sock').exists()
        result.update(completed=True, archive_sha256=pin['archiveSha256'], runtime_files=actual,
            old_manifests_unchanged=True, model=model, production_runtime_path=str(expected))
    finally:
        if pid:
            try:
                subprocess.run(['osascript','-e','tell application '+json.dumps(str(app))+' to quit'],capture_output=True,timeout=5)
            except subprocess.TimeoutExpired:
                pass
            deadline=time.monotonic()+15
            while time.monotonic()<deadline and processes(pid):
                time.sleep(.1)
            if processes(pid):
                actual=subprocess.run(['ps','-p',str(pid),'-o','comm='],capture_output=True,text=True).stdout.strip()
                if actual == str(executable):
                    os.kill(pid,signal.SIGTERM)
                    result['shutdown_fallback_sigterm']=True
                    deadline=time.monotonic()+15
                    while time.monotonic()<deadline and processes(pid): time.sleep(.1)
            result['survivors']=processes(pid)
        scrub_fixture_clipboard(root)
        for name in ['stdout.log', 'stderr.log']:
            if (root/name).exists(): shutil.copyfile(root/name, a.output/name)
        (a.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    assert not result.get('survivors')


if __name__ == '__main__':
    main()
