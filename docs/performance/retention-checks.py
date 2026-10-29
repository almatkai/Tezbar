#!/usr/bin/env python3
"""First visible, uncached UI query after quiet model retention on a cloned index.

Showing and awaiting frames resumes WebKit before input; the earlier hidden
bridge probe intentionally remains a distinct measurement, never relabeled.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from desktop import Desktop, prepare, processes
from window_visibility import main_on_screen


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--baseline', type=Path, required=True)
    p.add_argument('--candidate', type=Path, required=True)
    p.add_argument('--template-result', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--seconds', type=float, default=90)
    p.add_argument('--candidate-library', type=Path, help='Explicit Q4 fixture library experiment; never an installed profile')
    a = p.parse_args()
    assert 0 < a.seconds < 110
    if a.output.exists():
        raise ValueError('refusing to overwrite evidence')
    spec = importlib.util.spec_from_file_location('responsiveness', Path(__file__).with_name('responsiveness-desktop.py'))
    workload = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(workload)
    template = Path(json.loads(a.template_result.read_text())['root'])
    result = dict(completed=False, seconds=a.seconds, runs=[])
    try:
        for variant in ['baseline', 'candidate']:
            root = Path(tempfile.mkdtemp(prefix='tezbar-retention-check-'))
            prepare(root)
            shutil.rmtree(root/'data')
            subprocess.run(['cp', '-cR', str(template/'data'), str(root/'data')], check=True)
            shutil.copytree(template/'config', root/'config', dirs_exist_ok=True)
            library = None
            if variant == 'candidate' and a.candidate_library:
                lib_spec = importlib.util.spec_from_file_location('model_memory', Path(__file__).with_name('model-memory.py'))
                lib = importlib.util.module_from_spec(lib_spec)
                lib_spec.loader.exec_module(lib)
                library = lib.experimental_library(root/'data/knowledge/models/embeddinggemma-2-q4_k_m',
                                                  a.candidate_library, rewrite_manifest=True)
            binary = getattr(a, variant)
            d = Desktop(root)
            entry = dict(variant=variant, root=str(root), experimental_library=library,
                         binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
            result['runs'].append(entry)
            try:
                d.launch(binary)
                d.wait(lambda: d.request('core:launcher:status')['status'] == 'ready')
                d.wait(lambda: (lambda s: s['semanticSearch']['runtimeState'] == 'ready' and
                    not s['semanticSearch']['indexing'])(d.request('knowledge:snapshot')))
                if time.monotonic()-d.started < 12:
                    time.sleep(12-(time.monotonic()-d.started))
                workload.f.visible(d)
                d.js(workload.INSTALL)
                workload.burst(d, ['!orchards apples pears'], 0, True)
                before = next(x for x in processes(d.pid) if x['name'] == 'llama-server')
                d.command('hide')
                d.wait(lambda: not main_on_screen(d.pid), 5)
                entry['quiet'] = d.idle('retention', a.seconds)
                assert d.request('knowledge:snapshot')['semanticSearch']['runtimeState'] == 'ready'
                after = next(x for x in processes(d.pid) if x['name'] == 'llama-server')
                assert (before['pid'], before['start']) == (after['pid'], after['start'])
                entry['same_model_process'] = True
                workload.f.visible(d)
                entry['first'] = workload.burst(d, ['!orchards apples pears afterquiet'], 0, True)
                expected = d.js("return await window.__inputBench.original('!orchards apples pears afterquiet');")
                assert entry['first']['finalRows'] == expected
                entry['recovery'] = workload.burst(d, ['!orchards apples pears recovery'], 0, True)
                entry['ordered_parity'] = []
                for query in ['!orchards', '!orchards apples', '!orchards apples pears', '!background progress']:
                    value = workload.burst(d, [query], 0, True)
                    expected = d.js('return await window.__inputBench.original('+json.dumps(query)+');')
                    assert value['finalRows'] == expected
                    entry['ordered_parity'].append(dict(query=query, rows=expected))
                print(variant, {k: entry['first'][k] for k in ['responseMs', 'ipcMs', 'inputToTwoFramesMs']}, flush=True)
            finally:
                entry['samples'] = d.samples
                if d.pid:
                    try:
                        d.js("await window.__TAURI_INTERNALS__.invoke('set_suppress_blur_hide',{value:false}); return true;")
                    finally:
                        d.close()
                entry['survivors'] = processes(d.pid) if d.pid else []
        assert result['runs'][0]['first']['finalRows'] == result['runs'][1]['first']['finalRows']
        assert result['runs'][0]['recovery']['finalRows'] == result['runs'][1]['recovery']['finalRows']
        assert result['runs'][0]['ordered_parity'] == result['runs'][1]['ordered_parity']
        result['completed'] = True
    finally:
        a.output.write_text(json.dumps(result, indent=2)+'\n')


if __name__ == '__main__':
    main()
