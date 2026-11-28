#!/usr/bin/env python3
"""Focused fresh-process simple-search repro; frozen packaged apps, no GC.

old: unchanged R7 metadata/full-reply wrapper and burst serialization.
lean: same input, fresh-final-reply/DOM/two-frame guards, no returned full rows.
Payload accounting is opt-in and outside latency acceptance (string lengths only).
"""
import argparse
import ctypes
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from desktop import Desktop, LIB, Usage, prepare, processes, require_unlocked_display
from window_visibility import main_on_screen, owned_on_screen_bounds

SPEC = importlib.util.spec_from_file_location('response', Path(__file__).with_name('responsiveness-desktop.py'))
b = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(b)


def packaged(app, root):
    class PackagedDesktop(Desktop):
        def launch(self, executable):
            subprocess.run(['cp', '-cR', str(app), str(self.root/'Tezbar Performance.app')], check=True)
            shutil.rmtree(self.root/'Tezbar Performance.app/Contents/_CodeSignature', ignore_errors=True)
            super().launch(executable)
            self.host_start = next(p['start'] for p in processes(self.pid) if p['pid']==self.pid)

        def close(self):
            if self.pid and hasattr(self, 'host_start'):
                usage=Usage()
                if LIB.proc_pid_rusage(self.pid,4,ctypes.byref(usage))==0 and usage.proc_start_abstime!=self.host_start:
                    raise RuntimeError('host PID/start identity changed; refusing shutdown')
            super().close()
    d = PackagedDesktop(root)
    original = d.sample
    def sample(label):
        value = original(label)
        value['main_on_screen'] = main_on_screen(d.pid)
        value['owned_on_screen_bounds'] = owned_on_screen_bounds(d.pid)
        return value
    d.sample = sample
    return d


LEAN_INSTALL = b.INSTALL.replace('call.rows = rows;', '''
    // Only the latest reply is owned, only until its DOM check completes.
    if(state.captureFinal && query === state.latest) state.final = {call, rows};
''')


class LeanDesktop:
    """Adapt just the old burst script; no changes to input/wait/DOM guards."""
    def __init__(self, d): self.d = d
    def js(self, script):
        script = script.replace('const offset = state.calls.length;', 'delete state.final; state.captureFinal = true; const offset = state.calls.length;')
        script = script.replace('const titles = final.rows.map(r=>r.title);', 'const titles = state.final.rows.map(r=>r.title);')
        script = script.replace('finalRows:final.rows,', '')
        script = script.replace('domTitles:titles,', 'domRowCount:titles.length,')
        script = script.replace('return result;', 'state.captureFinal = false; delete state.final; return result;')
        try:
            return self.d.js(script)
        except Exception:
            # Preserve the guard/socket error even if cleanup also disconnects.
            try:
                self.d.js('window.__inputBench.captureFinal=false; delete window.__inputBench.final; return true;')
            except Exception:
                pass
            raise


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--app', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--protocol', choices=['old', 'lean'], required=True)
    p.add_argument('--payload', action='store_true', help='Diagnostic only, string lengths without serialization')
    p.add_argument('--replay', type=Path, help='Diagnostic only: replay recorded IPC cadence without React input commits')
    p.add_argument('--vmmap', action='store_true')
    p.add_argument('--idle-seconds', type=float, default=8)
    p.add_argument('--owners', action='store_true', help='Diagnostic DOM/asset/storage counters and heap summary')
    a = p.parse_args()
    require_unlocked_display()
    a.output.mkdir(parents=True, exist_ok=False)
    app = a.app.resolve(strict=True)
    binary = app/'Contents/MacOS/tezbar'
    root = Path(tempfile.mkdtemp(prefix='tb-renderer-'))
    prepare(root)
    d = packaged(app, root)
    result = dict(protocol=a.protocol, replay=str(a.replay) if a.replay else None, payload_diagnostic=a.payload, root=str(root), app=str(app), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), completed=False, metrics={})
    try:
        d.launch(binary)
        d.js('window.localStorage.clear(); return true;')
        result.update(ready_ms=d.ready_ms, first_ui_ms=d.first_ui_ms)
        d.wait(lambda:d.request('core:launcher:status')['status']=='ready')
        d.request('knowledge:snapshot')
        if time.monotonic()-d.started < 12: time.sleep(12-(time.monotonic()-d.started))
        d.command('hide')
        result['metrics']['before'] = d.idle('before', 5)
        b.f.visible(d)
        d.js(LEAN_INSTALL if a.protocol=='lean' else b.INSTALL)
        if a.payload:
            d.js('''const original=window.tezbar.searchAll; const counts=[];window.__payloadCounts=counts;
            window.tezbar.searchAll=async function(q){const rows=await original(q);let strings=0,characters=0,utf8StringBytes=0,maxString=0;const fields={};const encoder=new TextEncoder();
            function visit(v,key){if(typeof v==='string'){strings++;characters+=v.length;utf8StringBytes+=encoder.encode(v).length;maxString=Math.max(maxString,v.length);fields[key]=(fields[key]||0)+v.length;}else if(Array.isArray(v)){for(const x of v)visit(x,key);}else if(v&&typeof v==='object'){for(const [k,x]of Object.entries(v))visit(x,k);}}visit(rows,'rows');
            counts.push({rows:rows.length,strings,characters,utf8StringBytes,maxString,fields});return rows;};return true;''')
        if a.owners:
            d.js('''const owner={mutations:[],assets:0,assetCharacters:0};window.__owner=owner;
            const obs=new MutationObserver(records=>{owner.mutations.push({t:performance.now(),rows:document.querySelectorAll('button.list-row[data-tone="accent"]').length,added:records.reduce((n,r)=>n+r.addedNodes.length,0),removed:records.reduce((n,r)=>n+r.removedNodes.length,0)});});obs.observe(document.querySelector('#root')||document.body,{subtree:true,childList:true});
            const asset=window.tezbar.getAssetIconDataUrl;window.tezbar.getAssetIconDataUrl=async function(...args){owner.assets++;const value=await asset(...args);owner.assetCharacters+=value?.length||0;return value;};return true;''')
        driver = LeanDesktop(d) if a.protocol=='lean' else d
        def work():
            if a.replay:
                recorded=json.loads(a.replay.read_text())['metrics']['simple_bursts']['value']
                script='''const bursts='''+json.dumps([v['calls'] for v in recorded])+''';const values=[];
                for(const calls of bursts){const start=performance.now();const promises=[];for(const call of calls){const delay=call.start-calls[0].start-(performance.now()-start);if(delay>0)await new Promise(r=>setTimeout(r,delay));promises.push(window.tezbar.searchAll(call.query).then(rows=>rows.length));}values.push(await Promise.all(promises));}return values;'''
                return d.js(script)
            values=[]
            for i in range(8):
                queries=['ru','run','runt','runtime','runtime-fixture','runtime-fixture-00',f'runtime-fixture-{100+i:05}']
                values.append(b.burst(driver,queries,2 if i%2==0 else 20))
                d.sample('after_burst_'+str(i))
            return values
        result['metrics']['simple_bursts'] = b.f.w.measure(d,'simple_bursts',work)
        d.command('hide')
        result['metrics']['after'] = d.idle('after', a.idle_seconds)
        assert all(not s['main_on_screen'] for s in d.samples if s['phase'] in ['before','after'])
        if a.payload: result['payload'] = d.js('return window.__payloadCounts;')
        if a.owners:
            result['owners']=d.js('return {...window.__owner,storage:Object.keys(localStorage).map(k=>({key:k,characters:localStorage.getItem(k).length})),performanceMemory:performance.memory||null};')
            for proc in processes(d.pid):
                if proc['role']=='com.apple.WebKit.WebContent':
                    r=subprocess.run(['heap','--noContent','-s',str(proc['pid'])],capture_output=True,text=True)
                    (a.output/'webcontent-heap.txt').write_text(r.stdout+r.stderr)
        result['bridge'] = d.js('const s=window.__inputBench;return {dispatched:s.calls.length,active:s.active,maxActive:s.maxActive};')
        if a.vmmap:
            result['inspections']=[]
            for proc in processes(d.pid):
                if proc['role'] in ['host','com.apple.WebKit.WebContent','com.apple.WebKit.GPU']:
                    dest=a.output/(proc['role']+'-vmmap.txt')
                    r=subprocess.run(['vmmap','-summary',str(proc['pid'])],capture_output=True,text=True)
                    dest.write_text(r.stdout+r.stderr)
                    result['inspections'].append(dict(pid=proc['pid'],start=proc['start'],path=str(dest),returncode=r.returncode))
        result['completed']=True
    except Exception as e:
        result['error']=str(e)
        raise
    finally:
        result['samples']=d.samples
        try:
            if d.pid:
                try:d.js("await window.__TAURI_INTERNALS__.invoke('set_suppress_blur_hide',{value:false}); return true;")
                finally:d.close()
        finally:
            result['survivors']=processes(d.pid) if d.pid else []
            (a.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result['metrics'].items() if k!='simple_bursts'}), flush=True)

if __name__=='__main__': main()
