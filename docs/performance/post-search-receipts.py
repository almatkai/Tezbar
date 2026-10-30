#!/usr/bin/env python3
"""Opus pass 2: post-search idle probe with timed physical-input receipts.

Same prefix as post-search-probe.py (hidden idle, 12 open/close searches, 8
simple bursts, 4 simple broad bursts, hide, 2 s settle, 30 s hidden window) on
the frozen matched apps. Differences, all read-only:

- every sample carries an in-process HIDIdleTime receipt (hid_receipt.py) with
  monotonic pre/post read timestamps, plus host per-thread CPU; a 0.25 s
  watcher records the same (thread names/CPU only, no task suspension);
- after the window (never inside a timer): WebContent/host `vmmap -summary`,
  the content descriptor of the same broad replies the renderer received
  (row count, serialized length, ordered digest) and a runtime fingerprint
  of the frontend assets the WebView actually loaded (the frontend is embedded
  in Contents/MacOS/tezbar, so app inventories alone cannot prove identity);
- system load and WindowServer CPU time around the window as covariates.

No clipboard read/write, no physical-input suppression, no GC/purge/pressure.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time
import sys

sys.path.insert(0, str(Path(__file__).parent))
from desktop import prepare, processes, require_unlocked_display, scrub_fixture_clipboard
import hid_receipt

spec = importlib.util.spec_from_file_location('post_search_probe', Path(__file__).with_name('post-search-probe.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
r, b, f = probe.r, probe.b, probe.f
SIMPLE = ['ru', 'run', 'runt', 'runtime', 'runtime-fixture', 'runtime-fixture-00']
VMMAP_ROLES = ['host', 'com.apple.WebKit.WebContent']

FINGERPRINT = r'''
const origin = location.origin;
const urls = new Set([location.href]);
for (const s of document.scripts) if (s.src) urls.add(s.src);
for (const l of document.querySelectorAll('link[href]')) urls.add(l.href);
for (const e of performance.getEntriesByType('resource')) urls.add(e.name);
const out = {origin, secure: isSecureContext, assets: []};
for (const u of [...urls].filter(u => u.startsWith(origin)).sort()) {
  const bytes = new Uint8Array(await (await fetch(u, {cache: 'no-store'})).arrayBuffer());
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
  out.assets.push({path: u.slice(origin.length), bytes: bytes.length,
                   sha256: [...digest].map(x => x.toString(16).padStart(2, '0')).join('')});
}
return out;
'''

DESCRIPTOR = r'''
const out = [];
for (const q of QUERIES) {
  const rows = await window.tezbar.searchAll(q);
  const text = JSON.stringify(rows);
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text)));
  out.push({query: q, rows: rows.length, json_chars: text.length,
            ordered_sha256: [...digest].map(x => x.toString(16).padStart(2, '0')).join('')});
}
return out;
'''


def windowserver_cpu_s():
    # Read-only `ps` of the system compositor's accumulated CPU time (no signal, no attach).
    out = subprocess.run(['ps', '-axo', 'time=,comm='], capture_output=True, text=True).stdout
    for line in out.splitlines():
        t, _, comm = line.strip().partition(' ')
        if comm.endswith('/WindowServer'):
            parts = [float(x) for x in t.replace('-', ':').split(':')]
            secs = 0.
            for x in parts: secs = secs*60+x
            return secs
    return None


def instrument(d):
    """Append a timed HID receipt and host thread CPU to every harness sample."""
    original = d.sample

    def sample(label):
        value = original(label)
        value['t_sampled'] = time.monotonic()-d.started
        value['hid'] = hid_receipt.receipt(d.started)
        value['host_threads'] = probe.thread_times(d.pid) if d.pid else {}
        return value
    d.sample = sample
    return d


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--app', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--label', required=True, choices=['control', 'candidate'])
    p.add_argument('--model', help='copy this installed model into the fixture (APFS clone), as broad-full.py does')
    a = p.parse_args()
    require_unlocked_display()
    a.output.mkdir(parents=True, exist_ok=False)
    app = a.app.resolve(strict=True)
    binary = app/'Contents/MacOS/tezbar'
    root = Path(tempfile.mkdtemp(prefix='tb-receipts-'))
    prepare(root, a.model)
    d = instrument(r.packaged(app, root))
    result = dict(protocol='opus2-post-search-receipts-v1', label=a.label, model=bool(a.model), root=str(root), app=str(app),
                  binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), metrics={}, completed=False,
                  load_before=os.getloadavg())
    lean = r.LeanDesktop(d)
    try:
        d.launch(binary)
        d.js('window.localStorage.clear(); return true;')
        d.wait(lambda: d.request('core:launcher:status')['status'] == 'ready')
        d.request('knowledge:snapshot')
        if time.monotonic()-d.started < 12: time.sleep(12-(time.monotonic()-d.started))
        d.command('hide'); d.idle('settle_hidden', 2)
        result['metrics']['hidden_idle'] = f.idle(d, 'hidden_idle', False)
        f.visible(d); d.js(r.LEAN_INSTALL)

        def cycles():
            for i in range(12):
                d.command('hide'); d.wait(lambda: not r.main_on_screen(d.pid), 5)
                d.wait(lambda: d.js("return document.querySelector('input')?.value === '' && document.documentElement.classList.contains('tezbar-window-hidden');"), 5)
                f.visible(d); f.w.ui_search(d, f'runtime-fixture-{i:05}'); time.sleep(.12)
        f.w.measure(d, 'search_open_close', cycles)
        b.reset(d)

        def simple():
            return [b.burst(lean, SIMPLE+[f'runtime-fixture-{100+i:05}'], 2 if i % 2 == 0 else 20) for i in range(8)]
        result['metrics']['simple_bursts'] = f.w.measure(d, 'simple_bursts', simple)

        def broad():
            return [b.burst(lean, probe.SIMPLE_BROAD, 2 if i % 2 == 0 else 20) for i in range(4)]
        result['metrics']['simple_broad_bursts'] = f.w.measure(d, 'simple_broad_bursts', broad)
        d.command('hide'); d.idle('settle_search', 2)
        watched, stop = [], threading.Event()

        def watch():
            while not stop.wait(.25):
                watched.append(dict(t=time.monotonic()-d.started, hid=hid_receipt.receipt(d.started), threads=probe.thread_times(d.pid)))
        watcher = threading.Thread(target=watch); watcher.start()
        ws0 = windowserver_cpu_s()
        try:
            result['metrics']['post_search_idle'] = f.idle(d, 'post_search_idle', False)
        finally:
            stop.set(); watcher.join()
        result['windowserver_cpu_s'] = None if ws0 is None or windowserver_cpu_s() is None else windowserver_cpu_s()-ws0
        result['load_after_window'] = os.getloadavg()
        result['watch'] = watched
        # Post-window inspections only; nothing below is inside a timer.
        result['inspections'] = processes(d.pid)
        for proc in result['inspections']:
            if proc['role'] in VMMAP_ROLES:
                captured = subprocess.run(['vmmap', '-summary', str(proc['pid'])], capture_output=True, text=True)
                (a.output/('post_search_idle-'+proc['role']+'-vmmap.txt')).write_text(captured.stdout+captured.stderr)
        result['reply_descriptor'] = d.js(DESCRIPTOR.replace('QUERIES', json.dumps(probe.SIMPLE_BROAD)))
        result['frontend_fingerprint'] = d.js(FINGERPRINT)
        result['completed'] = True
    except Exception as e:
        result['error'] = str(e)
        raise
    finally:
        result['samples'] = d.samples
        try:
            if d.pid:
                try: d.js("await window.__TAURI_INTERNALS__.invoke('set_suppress_blur_hide',{value:false}); return true;")
                finally: d.close()
        finally:
            result['survivors'] = processes(d.pid) if d.pid else []
            if not result['survivors']: scrub_fixture_clipboard(root)
            (a.output/'result.json').write_text(json.dumps(result, indent=2)+'\n')
    m = result['metrics']['post_search_idle']
    print(a.label, json.dumps({k: m[k] for k in ['cpu_s', 'interrupt_wakeups', 'footprint_median_bytes', 'rss_last_bytes']}), flush=True)


if __name__ == '__main__': main()
