#!/usr/bin/env python3
"""Full owned desktop family with real indexing + native foreground demand.

No UI frame/typing claim. Default OS-temp profiles avoid the launcher's ignored
'build' component. Keeps every native/WebKit/Metal/model/supervisor owner.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import sqlite3
import tempfile
import time
from desktop import Desktop, prepare, processes, summarize

spec=importlib.util.spec_from_file_location('model_memory',Path(__file__).with_name('model-memory.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--binary',type=Path,required=True)
    p.add_argument('--model',required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--long-chunks',action='store_true')
    p.add_argument('--retention',type=float,default=0)
    a=p.parse_args()
    assert 0<=a.retention<110
    a.output.mkdir(exist_ok=False)
    root=Path(tempfile.mkdtemp(prefix='tb-fam-'))
    assert 'build' not in root.parts, 'launcher intentionally excludes build directories'
    prepare(root,a.model)
    if a.long_chunks:
        original='Search indexing preserves background progress and foreground response.'
        replacement=''.join(chr(0x4e00+i) for i in range(len(original)))
        for f in (root/'knowledge-docs').glob('*.txt'):f.write_text(f.read_text().replace(original,replacement))
    d=Desktop(root)
    result=dict(completed=False,root=str(root),long_chunks=a.long_chunks,
        binary_sha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),pressure_before=m.pressure(),
        scope='complete actual desktop family, native IPC foreground demand, no visible UI timing',metrics={})
    try:
        d.launch(a.binary)
        d.wait(lambda:d.request('core:launcher:status')['status']=='ready')
        db=sqlite3.connect('file:'+str(root/'data/search/index.sqlite3')+'?mode=ro',uri=True)
        result['launcher_files']=db.execute("SELECT count(*) FROM documents WHERE category='files'").fetchone()[0]
        db.close()
        assert result['launcher_files']==5000
        result['metrics']['initial_idle']=d.idle('initial_idle',3)
        d.request('knowledge:speed:set','fast');d.request('knowledge:depth:set','smart')
        d.request('knowledge:root:add',str(root/'knowledge-docs'))
        d.request('knowledge:index:start')
        d.wait(lambda:(lambda s:s['state']=='completed' and s['processedSources']==96)(d.request('knowledge:snapshot')['status']))
        db=sqlite3.connect('file:'+str(root/'data/knowledge/knowledge.sqlite3')+'?mode=ro',uri=True)
        assert db.execute('SELECT count(*) FROM knowledge_chunks').fetchone()[0]==192
        start=time.monotonic();d.sample('semantic');next_probe=start+2;bursts=[]
        d.request('knowledge:embedding:set','embeddinggemma-2','q4_k_m')
        actual=None
        while True:
            d.sample('semantic')
            count=db.execute('SELECT count(*) FROM knowledge_gemma_embeddings').fetchone()[0]
            if count==192 and actual is None: actual=time.monotonic()-start
            status=d.request('knowledge:snapshot')['semanticSearch']
            if actual is not None and not status['indexing'] and status['indexedChunks']==192:break
            if time.monotonic()-start>180:raise RuntimeError('indexing timed out')
            if time.monotonic()>=next_probe:
                values=[]
                for query in ['runtime-fixture','runtime-fixture-00','runtime-fixture-00123']:
                    t=time.monotonic();rows=d.request('search:all',query)
                    values.append(dict(query=query,ms=(time.monotonic()-t)*1000,rows=rows))
                    time.sleep(.02)
                bursts.append(values);next_probe=time.monotonic()+2
            time.sleep(.03)
        result['metrics']['semantic']=summarize([s for s in d.samples if s['phase']=='semantic'])
        result['metrics']['semantic'].update(actual_complete_s=actual,bursts=bursts)
        result['metrics']['retained_idle']=d.idle('retained_idle',30)
        result['runtime_status']=d.request('knowledge:snapshot')['semanticSearch']
        digest=hashlib.sha256()
        for path,offset,vector in db.execute('SELECT s.path,c.start_offset,g.vector FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id ORDER BY s.path,c.start_offset'):
            digest.update(Path(path).name.encode());digest.update(str(offset).encode());digest.update(vector)
        result['vectors_sha256']=digest.hexdigest();db.close()
        expected=('5ac291cdacc7493c03dab643d5ab1567d31f9cbc8a694b139d2a91aedf7c9ab2' if a.long_chunks else '27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c')
        assert result['vectors_sha256']==expected
        if a.retention:
            d.request('search:all','!orchards apples pears')
            before=next(x for x in processes(d.pid) if x['name']=='llama-server')
            result['metrics']['quiet90']=d.idle('quiet90',a.retention)
            after=next(x for x in processes(d.pid) if x['name']=='llama-server')
            assert (before['pid'],before['start'])==(after['pid'],after['start'])
            t=time.monotonic();result['first_after_quiet_rows']=d.request('search:all','!orchards apples pears afterquiet')
            result['first_after_quiet_ms']=(time.monotonic()-t)*1000
            result['same_retained_model']=True
        result['pressure_after']=m.pressure();result['completed']=True
    finally:
        result['samples']=d.samples;d.close();result['survivors']=processes(d.pid) if d.pid else []
        for f in ['stdout.log','stderr.log','startup-timeout-stack.txt']:
            if (root/f).exists():shutil.copyfile(root/f,a.output/f)
        (a.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    assert not result['survivors']


if __name__=='__main__':main()
