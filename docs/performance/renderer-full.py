#!/usr/bin/env python3
"""R8: unchanged R7 desktop workload, with explicitly lean burst receipts.

No application changes. Frozen .apps supply their own embedded frontend/resources.
Full ordered row/score/action parity is measured separately, never retained by
this timed wrapper. Historical R7 scripts and evidence remain unchanged.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
from desktop import prepare, processes, require_unlocked_display

spec=importlib.util.spec_from_file_location('renderer_loop',Path(__file__).with_name('renderer-loop.py'))
r=importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--app',type=Path,required=True)
    p.add_argument('--model',required=True)
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args()
    require_unlocked_display()
    a.output.mkdir(parents=True,exist_ok=False)
    app=a.app.resolve(strict=True)
    binary=app/'Contents/MacOS/tezbar'
    root=Path(tempfile.mkdtemp(prefix='tb-renderer-full-'))
    prepare(root,a.model)
    d=r.packaged(app,root)
    result=dict(protocol='R8-R7-workload-lean-replies-v1',production_optimization=False,root=str(root),app=str(app),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),metrics={},inspections={},completed=False)
    workload=r.b
    workload.INSTALL=r.LEAN_INSTALL
    original_burst=workload.burst
    workload.burst=lambda desktop,*args,**kwargs:original_burst(r.LeanDesktop(desktop),*args,**kwargs)
    original_idle=d.idle
    def idle(label,seconds):
        value=original_idle(label,seconds)
        if label in ['hidden_idle','active_model_idle','post_repeat_idle']:
            result['inspections'][label]=processes(d.pid)
            for proc in result['inspections'][label]:
                if proc['role'] in ['host','llama-server','com.apple.WebKit.WebContent','com.apple.WebKit.GPU']:
                    captured=subprocess.run(['vmmap','-summary',str(proc['pid'])],capture_output=True,text=True)
                    (a.output/(label+'-'+proc['role']+'-vmmap.txt')).write_text(captured.stdout+captured.stderr)
        return value
    d.idle=idle
    try:
        d.launch(binary)
        d.js('window.localStorage.clear(); return true;')
        result.update(ready_ms=d.ready_ms,first_ui_ms=d.first_ui_ms,viewport=d.js('return {width:innerWidth,height:innerHeight,pixelRatio:devicePixelRatio};'))
        workload.scenario(d,result['metrics'])
        assert result['metrics']['vectors_sha256']=='27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c'
        result['completed']=True
    except Exception as error:
        result['error']=str(error)
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

if __name__=='__main__':main()
