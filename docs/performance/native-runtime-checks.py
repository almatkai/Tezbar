#!/usr/bin/env python3
"""Native IPC/index parity and 90s retention in the complete actual desktop family.

No animation-frame/UI-latency claim: this remains runnable with a locked display.
Runtime selection is the production implementation, never a library override.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import time
from desktop import Desktop, prepare, processes


def normalize(value, root):
    if isinstance(value,str): return value.replace(str(root),'<PROFILE>')
    if isinstance(value,list): return [normalize(v,root) for v in value]
    if isinstance(value,dict): return {k:normalize(v,root) for k,v in value.items()}
    return value


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--baseline',type=Path,required=True)
    p.add_argument('--candidate',type=Path,required=True)
    p.add_argument('--template-result',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--seconds',type=float,default=90)
    a=p.parse_args()
    assert 0<a.seconds<110
    a.output.mkdir(exist_ok=False)
    template=Path(json.loads(a.template_result.read_text())['root'])
    result=dict(completed=False,runs=[],scope='native replies; complete desktop family; no visible-frame claim')
    try:
        for variant in ['baseline','candidate']:
            root=Path(tempfile.mkdtemp(prefix='tb-nat-'))
            prepare(root)
            shutil.rmtree(root/'data')
            subprocess.run(['cp','-cR',str(template/'data'),str(root/'data')],check=True)
            shutil.copytree(template/'config',root/'config',dirs_exist_ok=True)
            binary=getattr(a,variant)
            d=Desktop(root)
            entry=dict(variant=variant,root=str(root),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
            result['runs'].append(entry)
            try:
                d.launch(binary)
                d.wait(lambda:d.request('core:launcher:status')['status']=='ready')
                d.wait(lambda:(lambda s:s['semanticSearch']['runtimeState']=='ready' and not s['semanticSearch']['indexing'])(d.request('knowledge:snapshot')))
                entry['runtime_status']=d.request('knowledge:snapshot')['semanticSearch']
                entry['queries']=[]
                for query in ['runtime-fixture-00123','!orchards','!orchards apples','!orchards apples pears','!background progress']:
                    begin=time.monotonic()
                    rows=d.request('search:all',query)
                    entry['queries'].append(dict(query=query,ms=(time.monotonic()-begin)*1000,rows=normalize(rows,root)))
                before=next(p for p in processes(d.pid) if p['name']=='llama-server')
                entry['quiet']=d.idle('retention',a.seconds)
                after=next(p for p in processes(d.pid) if p['name']=='llama-server')
                assert (before['pid'],before['start'])==(after['pid'],after['start'])
                entry['same_model_process']=True
                begin=time.monotonic()
                entry['first_rows']=normalize(d.request('search:all','!orchards apples pears afterquiet'),root)
                entry['first_ms']=(time.monotonic()-begin)*1000
                db=sqlite3.connect('file:'+str(root/'data/knowledge/knowledge.sqlite3')+'?mode=ro',uri=True)
                rows=db.execute('SELECT * FROM knowledge_gemma_embeddings ORDER BY chunk_id').fetchall()
                entry['vector_rows']=len(rows)
                # Full native/search signatures are compared below; the accepted
                # desktop corpus separately pins all 192 stored vector payloads.
                db.close()
            finally:
                entry['samples']=d.samples
                d.close()
                entry['survivors']=processes(d.pid) if d.pid else []
                for f in ['stdout.log','stderr.log','startup-timeout-stack.txt']:
                    if (root/f).exists():shutil.copyfile(root/f,a.output/(variant+'-'+f))
        b,c=result['runs']
        assert [(v['query'],v['rows']) for v in b['queries']]==[(v['query'],v['rows']) for v in c['queries']]
        assert b['first_rows']==c['first_rows']
        assert b['vector_rows']==c['vector_rows']==192
        assert all(not r['survivors'] for r in result['runs'])
        result['completed']=True
    finally:(a.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')


if __name__=='__main__':main()
