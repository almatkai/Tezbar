#!/usr/bin/env python3
"""The accepted visible-search workload plus out-of-timer component inspection.

Keeps the model retained, the same frontend/index/query cadence and complete
responsible-PID accounting. Profile tools only inspect this owned app family.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import time
from desktop import Desktop, prepare, processes
from window_visibility import main_on_screen, owned_on_screen_bounds

spec = importlib.util.spec_from_file_location('model_memory', Path(__file__).with_name('model-memory.py'))
model_memory = importlib.util.module_from_spec(spec)
spec.loader.exec_module(model_memory)
capture, pressure = model_memory.capture, model_memory.pressure


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--binary', required=True)
    p.add_argument('--model', required=True)
    p.add_argument('--output', required=True)
    p.add_argument('--retained-seconds', type=float, default=0,
                   help='Additional retained-model first-query guard, below the unchanged 120s policy')
    p.add_argument('--experimental-library', type=Path, help='Explicit cloned-runtime experiment; never the installed model')
    a = p.parse_args()
    output = Path(a.output).resolve()
    output.mkdir(exist_ok=False)
    spec = importlib.util.spec_from_file_location('responsiveness', Path(__file__).with_name('responsiveness-desktop.py'))
    workload = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(workload)
    root = tempfile.mkdtemp(prefix='tezbar-memory-desktop-')
    prepare(root, a.model)
    library = None
    if a.experimental_library:
        directory = Path(root)/'data/knowledge/models'/Path(a.model).name
        library = model_memory.experimental_library(directory, a.experimental_library, rewrite_manifest=True)
    d = Desktop(root)
    result = dict(root=root, binary_sha256=hashlib.sha256(Path(a.binary).read_bytes()).hexdigest(),
                  completed=False, experimental_library=library, metrics={}, inspections={})
    original_sample, original_idle = d.sample, d.idle

    def sample(label):
        s = original_sample(label)
        s['owned_on_screen_bounds'] = owned_on_screen_bounds(d.pid)
        s['main_on_screen'] = main_on_screen(d.pid)
        return s

    def idle(label, seconds):
        value = original_idle(label, seconds)
        if label in ['hidden_idle', 'active_model_idle', 'post_repeat_idle']:
            inspection = dict(pressure=pressure(), processes=processes(d.pid), tools=[])
            for proc in inspection['processes']:
                if proc['role'] in ['host', 'llama-server', 'com.apple.WebKit.WebContent', 'com.apple.WebKit.GPU']:
                    dest = output / (label + '-' + proc['role'] + '-vmmap.txt')
                    inspection['tools'].append(capture(dest, ['vmmap', '-summary', str(proc['pid'])]))
            result['inspections'][label] = inspection
        return value

    d.sample, d.idle = sample, idle
    try:
        d.launch(Path(a.binary).resolve())
        d.js('window.localStorage.clear(); return true;')
        result.update(ready_ms=d.ready_ms, first_ui_ms=d.first_ui_ms)
        workload.scenario(d, result['metrics'])
        assert result['metrics']['vectors_sha256'] == '27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
        if a.retained_seconds:
            assert 0 < a.retained_seconds < 115
            # A real warm request establishes last_used; never defeat the idle
            # policy with keepalive requests, forced GC or model replacement.
            before = workload.f.w.bridge_search(d, ['!orchards apples pears'])[0]
            result['metrics']['retained_quiet'] = original_idle('retained_quiet', a.retained_seconds)
            assert d.request('knowledge:snapshot')['semanticSearch']['runtimeState'] == 'ready'
            after = workload.f.w.bridge_search(d, ['!orchards apples pears'])[0]
            assert (before['ids'], before['scores']) == (after['ids'], after['scores'])
            result['metrics']['retained_first_query'] = dict(before=before, after=after)
        # Outside timing: sampled stacks identify sleeping/ticking native owners.
        for proc in processes(d.pid):
            if proc['role'] in ['host', 'native-supervisor', 'llama-server']:
                result.setdefault('stacks', []).append(capture(output/(proc['role']+'-sample.txt'),
                    ['sample', str(proc['pid']), '3', '-file', str(output/(proc['role']+'-stack.txt'))]))
        result['completed'] = True
    except Exception as e:
        result['error'] = str(e)
        raise
    finally:
        result['samples'] = d.samples
        (output/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        if d.pid:
            try:
                d.js("await window.__TAURI_INTERNALS__.invoke('set_suppress_blur_hide',{value:false}); return true;")
            finally:
                d.close()
        (output/'cleanup.json').write_text(json.dumps(dict(owned_survivors=processes(d.pid) if d.pid else []), indent=2)+'\n')


if __name__ == '__main__':
    main()
