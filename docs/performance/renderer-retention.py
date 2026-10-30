#!/usr/bin/env python3
"""Lean 90s retention pair; exact ordered payload parity collected afterwards."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from desktop import prepare, processes, require_unlocked_display
from window_visibility import main_on_screen
spec=importlib.util.spec_from_file_location('renderer_loop',Path(__file__).with_name('renderer-loop.py'))
r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--original',type=Path,required=True)
    p.add_argument('--accepted',type=Path,required=True)
    p.add_argument('--template-result',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();require_unlocked_display()
    if a.output.exists():raise ValueError('refusing to overwrite evidence')
    template=Path(json.loads(a.template_result.read_text())['root'])
    out=dict(protocol='R8-lean-retention90-v1',completed=False,runs=[])
    try:
        for variant in ['original','accepted']:
            app=getattr(a,variant).resolve(strict=True);binary=app/'Contents/MacOS/tezbar'
            root=Path(tempfile.mkdtemp(prefix='tb-renderer-retain-'));prepare(root)
            shutil.rmtree(root/'data');subprocess.run(['cp','-cR',str(template/'data'),str(root/'data')],check=True)
            shutil.copytree(template/'config',root/'config',dirs_exist_ok=True)
            d=r.packaged(app,root)
            entry=dict(variant=variant,root=str(root),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest());out['runs'].append(entry)
            try:
                d.launch(binary)
                d.wait(lambda:d.request('core:launcher:status')['status']=='ready')
                d.wait(lambda:(lambda s:s['runtimeState']=='ready' and not s['indexing'])(d.request('knowledge:snapshot')['semanticSearch']))
                if time.monotonic()-d.started<12:time.sleep(12-(time.monotonic()-d.started))
                r.b.f.visible(d);d.js(r.LEAN_INSTALL)
                driver=r.LeanDesktop(d)
                r.b.burst(driver,['!orchards apples pears beforequiet'],0,True)
                before=next(p for p in processes(d.pid) if p['name']=='llama-server')
                d.command('hide');d.wait(lambda:not main_on_screen(d.pid),5)
                entry['quiet']=d.idle('quiet90',90)
                after=next(p for p in processes(d.pid) if p['name']=='llama-server')
                assert (before['pid'],before['start'])==(after['pid'],after['start'])
                assert d.request('knowledge:snapshot')['semanticSearch']['runtimeState']=='ready'
                entry['same_model_process']=True
                r.b.f.visible(d)
                entry['first']=r.b.burst(driver,['!orchards apples pears afterquiet'],0,True)
                entry['recovery']=r.b.burst(driver,['!orchards apples pears recovery'],0,True)
                # Full-row collection is now outside both quiet RAM and latency.
                d.js(r.b.INSTALL)
                d.js('window.__inputBench.deepMode=true; return true;')
                entry['parity']=[]
                for query in ['!orchards','!orchards apples','!orchards apples pears','!background progress']:
                    value=r.b.burst(d,[query],0,True)
                    expected=d.js('return await window.__inputBench.original('+json.dumps(query)+');')
                    assert value['finalRows']==expected
                    entry['parity'].append(dict(query=query,rows=expected))
                print(variant,entry['first']['responseMs'],entry['first']['inputToTwoFramesMs'],flush=True)
            finally:
                entry['samples']=d.samples
                try:
                    if d.pid:
                        try:d.js("await window.__TAURI_INTERNALS__.invoke('set_suppress_blur_hide',{value:false}); return true;")
                        finally:d.close()
                finally:entry['survivors']=processes(d.pid) if d.pid else []
        assert out['runs'][0]['parity']==out['runs'][1]['parity']
        out['exact_ordered_parity']=True;out['completed']=True
    finally:a.output.write_text(json.dumps(out,indent=2)+'\n')

if __name__=='__main__':main()
