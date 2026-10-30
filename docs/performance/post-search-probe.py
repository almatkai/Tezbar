#!/usr/bin/env python3
"""Opus pass 1: short post-search idle loop for the R14 host regression.

Replays the exact broad-full.py prefix that precedes `post_search_idle`
(hidden idle, 12 open/close searches, 8 simple bursts, 4 simple broad bursts,
hide, 2 s settle) and then samples a 30 s hidden window. No model, indexing or
later phases. `--stack` additionally runs macOS `sample` on the owned host
across the window (+0.2 s .. +29.2 s); stack runs are
diagnostic and never used as timing evidence. Same visibility/fresh-reply/
trusted-input guards; no clipboard read/write; packaged app resources only.
"""
import argparse
import ctypes
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import threading
import time
from desktop import prepare, processes, require_unlocked_display, scrub_fixture_clipboard

spec = importlib.util.spec_from_file_location('renderer_loop', Path(__file__).with_name('renderer-loop.py'))
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)
b = r.b
f = b.f
SIMPLE_BROAD = ['ru', 'run', 'runt', 'runtime', 'runtime-fixture']
LIBPROC = ctypes.CDLL('/usr/lib/libproc.dylib')


class ThreadInfo(ctypes.Structure):
    _fields_ = [('user', ctypes.c_uint64), ('system', ctypes.c_uint64)] + [(n, ctypes.c_int32) for n in
               ['cpu_usage', 'policy', 'run_state', 'flags', 'sleep_time', 'curpri', 'priority', 'maxpriority']] + [('name', ctypes.c_char * 64)]


def hid_idle_s():
    # Read-only system idle time since the last physical HID input; no event content.
    out = subprocess.run(['ioreg', '-c', 'IOHIDSystem', '-d', '4', '-k', 'HIDIdleTime'], capture_output=True, text=True).stdout
    for line in out.splitlines():
        if '"HIDIdleTime"' in line: return int(line.rsplit('=', 1)[1])/1e9
    return None


def thread_times(pid):
    # PROC_PIDLISTTHREADS / PROC_PIDTHREADINFO: no task suspension, names only.
    ids = (ctypes.c_uint64 * 512)()
    n = LIBPROC.proc_pidinfo(pid, 6, ctypes.c_uint64(0), ids, ctypes.sizeof(ids)) // 8
    out = {}
    for tid in ids[:n]:
        info = ThreadInfo()
        if LIBPROC.proc_pidinfo(pid, 5, ctypes.c_uint64(tid), ctypes.byref(info), ctypes.sizeof(info)) == ctypes.sizeof(info):
            out[str(tid)] = [info.name.decode(errors='replace'), (info.user+info.system)/1e9]
    return out


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--app', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--label', required=True)
    p.add_argument('--stack', action='store_true')
    p.add_argument('--trigger', action='store_true', help='diagnostic: one 1.5 s host sample when a CPU burst is detected')
    p.add_argument('--no-broad', action='store_true', help='diagnostic: omit the four broad bursts')
    p.add_argument('--model', help='copy this installed model into the fixture (APFS clone), as broad-full.py does')
    p.add_argument('--vmmap', action='store_true', help='run broad-full.py\'s hidden_idle vmmap inspection')
    a = p.parse_args()
    require_unlocked_display()
    a.output.mkdir(parents=True, exist_ok=False)
    app = a.app.resolve(strict=True)
    binary = app/'Contents/MacOS/tezbar'
    root = Path(tempfile.mkdtemp(prefix='tb-postsearch-'))
    prepare(root, a.model)
    d = r.packaged(app, root)
    result = dict(protocol='opus1-post-search-probe-v1', label=a.label, stack=a.stack, broad=not a.no_broad, model=bool(a.model), vmmap=a.vmmap,
                  root=str(root), app=str(app), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  metrics={}, completed=False)
    lean = r.LeanDesktop(d)
    try:
        d.launch(binary)
        d.js('window.localStorage.clear(); return true;')
        d.wait(lambda: d.request('core:launcher:status')['status'] == 'ready')
        d.request('knowledge:snapshot')
        if time.monotonic()-d.started < 12: time.sleep(12-(time.monotonic()-d.started))
        d.command('hide'); d.idle('settle_hidden', 2)
        result['metrics']['hidden_idle'] = f.idle(d, 'hidden_idle', False)
        if a.vmmap:
            for proc in processes(d.pid):
                if proc['role'] in ['host', 'llama-server', 'com.apple.WebKit.WebContent', 'com.apple.WebKit.GPU']:
                    captured = subprocess.run(['vmmap', '-summary', str(proc['pid'])], capture_output=True, text=True)
                    (a.output/('hidden_idle-'+proc['role']+'-vmmap.txt')).write_text(captured.stdout+captured.stderr)
        f.visible(d); d.js(r.LEAN_INSTALL)
        def cycles():
            for i in range(12):
                d.command('hide'); d.wait(lambda: not r.main_on_screen(d.pid), 5)
                d.wait(lambda: d.js("return document.querySelector('input')?.value === '' && document.documentElement.classList.contains('tezbar-window-hidden');"), 5)
                f.visible(d); f.w.ui_search(d, f'runtime-fixture-{i:05}'); time.sleep(.12)
        f.w.measure(d, 'search_open_close', cycles)
        b.reset(d)
        def simple():
            for i in range(8):
                b.burst(lean, ['ru', 'run', 'runt', 'runtime', 'runtime-fixture', 'runtime-fixture-00', f'runtime-fixture-{100+i:05}'], 2 if i % 2 == 0 else 20)
        result['metrics']['simple_bursts'] = {k: v for k, v in f.w.measure(d, 'simple_bursts', simple).items() if k != 'value'}
        if not a.no_broad:
            def broad():
                for i in range(4): b.burst(lean, SIMPLE_BROAD, 2 if i % 2 == 0 else 20)
            result['metrics']['simple_broad_bursts'] = {k: v for k, v in f.w.measure(d, 'simple_broad_bursts', broad).items() if k != 'value'}
        d.command('hide'); d.idle('settle_search', 2)
        stack = None
        threads = []
        stop = threading.Event()
        def watch():
            # Trigger at most one short sample when the host burns >6 ms CPU in 250 ms.
            fired = False; prev = None
            while not stop.wait(.25):
                now = dict(t=time.monotonic()-d.started, hid_idle_s=hid_idle_s(), threads=thread_times(d.pid)); threads.append(now)
                if prev and a.trigger and not fired:
                    used = sum(v[1]-prev['threads'].get(k, [None, v[1]])[1] for k, v in now['threads'].items())
                    if used > .006:
                        fired = True; result['trigger_t'] = now['t']
                        subprocess.run(['sample', str(d.pid), '1.5', '1', '-mayDie', '-file', str(a.output/'host-burst-sample.txt')], capture_output=True)
                prev = now
        watcher = threading.Thread(target=watch); watcher.start()
        if a.stack:
            def run_sample():
                time.sleep(.2)
                subprocess.run(['sample', str(d.pid), '29', '1', '-mayDie', '-file', str(a.output/'host-sample.txt')], capture_output=True)
            stack = threading.Thread(target=run_sample); stack.start()
        result['metrics']['post_search_idle'] = f.idle(d, 'post_search_idle', False)
        if stack: stack.join()
        stop.set(); watcher.join(); result['host_threads'] = threads
        result['host_threads_after'] = subprocess.run(['ps', '-M', '-p', str(d.pid)], capture_output=True, text=True).stdout.count('\n') - 1
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
