#!/usr/bin/env python3
"""Supplemental opening + authoritative final-query parity, outside timing runs."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from desktop import Desktop, prepare, require_unlocked_display

spec=importlib.util.spec_from_file_location('input_bench',Path(__file__).with_name('responsiveness-desktop.py'))
b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)


def normalize(value, root):
    if isinstance(value,str):return value.replace(str(root),'<PROFILE>')
    if isinstance(value,list):return [normalize(v,root) for v in value]
    if isinstance(value,dict):return {k:normalize(v,root) for k,v in value.items()}
    return value


def main():
    p=argparse.ArgumentParser();p.add_argument('--baseline',type=Path,required=True);p.add_argument('--candidate',type=Path,required=True)
    p.add_argument('--model',required=True);p.add_argument('--template-result',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    a=p.parse_args()
    require_unlocked_display()
    if a.output.exists():raise ValueError('refusing to overwrite evidence')
    result=dict(openings=[],parity=[],completed=False)
    try:
        for variant in ['baseline','candidate','candidate','baseline','baseline','candidate']:
            binary=getattr(a,variant);root=Path(tempfile.mkdtemp(prefix='tezbar-input-opening-'));prepare(root,a.model);d=Desktop(root)
            try:
                d.launch(binary)
                value=dict(variant=variant,root=str(root),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),ready_ms=d.ready_ms,first_ui_ms=d.first_ui_ms,viewport=d.js('return {width:innerWidth,height:innerHeight,pixelRatio:devicePixelRatio};'))
                result['openings'].append(value);print('opening',variant,value['first_ui_ms'],flush=True)
            finally:d.close()
        template=json.loads(a.template_result.read_text())
        # Both processes receive separate APFS clones of one completed isolated
        # index, preserving its authoritative IDs, row order, paths and vectors.
        # Original benchmark files are read only. They are not re-indexed here.
        for variant in ['baseline','candidate']:
            binary=getattr(a,variant);root=Path(tempfile.mkdtemp(prefix='tezbar-input-parity-'));prepare(root)
            shutil.rmtree(root/'data');subprocess.run(['cp','-cR',str(Path(template['root'])/'data'),str(root/'data')],check=True)
            shutil.copytree(Path(template['root'])/'config',root/'config',dirs_exist_ok=True)
            d=Desktop(root)
            try:
                d.launch(binary);d.wait(lambda:d.request('core:launcher:status')['status']=='ready')
                d.wait(lambda:(lambda s:s['semanticSearch']['runtimeState']=='ready' and not s['semanticSearch']['indexing'])(d.request('knowledge:snapshot')))
                if time.monotonic()-d.started<12:time.sleep(12-(time.monotonic()-d.started))
                b.f.visible(d);d.js(b.INSTALL)
                values=[]
                for deep in [False,True]:
                    b.reset(d)
                    for i in range(4):
                        queries=['!orch','!orchards','!orchards apples','!orchards apples pears'] if deep else ['ru','runtime','runtime-fixture',f'runtime-fixture-{100+i:05}']
                        value=b.burst(d,queries,2 if i%2==0 else 20,deep)
                        expected=d.js('return await window.__inputBench.original('+json.dumps(queries[-1])+');')
                        assert value['finalRows']==expected,'fresh final reply differs from authoritative final query'
                        values.append(dict(query=queries[-1],rows=normalize(expected,root),dispatched=value['dispatched'],domTitles=value['domTitles']))
                result['parity'].append(dict(variant=variant,root=str(root),values=values))
                print('parity',variant,len(values),'fresh final replies equal direct native rows/actions',flush=True)
                if variant=='candidate':
                    b.reset(d)
                    during=d.js('''const s=window.__inputBench;const offset=s.calls.length;
                      document.querySelector('#command-input').dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}));
                      s.input('composition intermediate','composition intermediate');
                      await new Promise(r=>setTimeout(r,40));return s.calls.slice(offset).map(c=>c.query);''')
                    assert 'composition intermediate' not in during,'intermediate IME dispatched'
                    d.command('hide');d.wait(lambda:d.js("return document.querySelector('#command-input')?.value==='' && document.documentElement.classList.contains('tezbar-window-hidden');"),5)
                    b.f.visible(d)
                    resumed=b.burst(d,['runtime-fixture-00123'],0)
                    assert resumed['finalRows']==d.js("return await window.__inputBench.original('runtime-fixture-00123');")
                    result['candidate_ime_hide_recovery']=True
                    print('candidate synthetic composition + native hide/reopen recovery passed',flush=True)
            finally:
                if d.pid:
                    try:d.js("await window.__TAURI_INTERNALS__.invoke('set_suppress_blur_hide',{value:false}); return true;")
                    finally:d.close()
        baseline=result['parity'][0]['values'];candidate=result['parity'][1]['values']
        assert [(v['query'],v['rows']) for v in baseline]==[(v['query'],v['rows']) for v in candidate], 'A/B ordered final rows/scores/actions differ on cloned index'
        result['exact_final_query_parity']=True
        result['completed']=True
    finally:a.output.write_text(json.dumps(result,indent=2)+'\n')

if __name__=='__main__':main()
