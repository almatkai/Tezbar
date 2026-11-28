#!/usr/bin/env python3
"""Time actual durable learned-vector completion, separately from cached UI counts.
Uses a read-only connection to the isolated database, not a substitute embedder.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import tempfile
import time
from desktop import Desktop, prepare
# Hyphenated CLI module is also imported as the single workload source.
import importlib.util
spec = importlib.util.spec_from_file_location('workload',Path(__file__).with_name('desktop-workload.py'))
workload = importlib.util.module_from_spec(spec);spec.loader.exec_module(workload)

p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--model',required=True);p.add_argument('--output',required=True);a=p.parse_args()
root=tempfile.mkdtemp(prefix='tezbar-desktop-');prepare(root,a.model);d=Desktop(root);result={}
try:
    d.launch(Path(a.binary).resolve())
    d.wait(lambda:d.request('core:launcher:status')['status']=='ready')
    d.request('knowledge:speed:set','fast');d.request('knowledge:depth:set','smart');d.request('knowledge:root:add',str(d.root/'knowledge-docs'));d.request('knowledge:index:start')
    d.wait(lambda:d.request('knowledge:snapshot')['status']['state']=='completed')
    if time.monotonic()-d.started<12:time.sleep(12-(time.monotonic()-d.started))
    d.command('hide')
    # Let the ordinary home warmup/foreground grace period after hide settle.
    time.sleep(1.5)
    db=sqlite3.connect('file:'+str(d.root/'data/knowledge/knowledge.sqlite3')+'?mode=ro',uri=True)
    assert db.execute('SELECT count(*) FROM knowledge_chunks').fetchone()[0]==192
    assert db.execute('SELECT count(*) FROM knowledge_gemma_embeddings').fetchone()[0]==0
    def indexing():
        start=time.monotonic();d.request('knowledge:embedding:set','embeddinggemma-2','q4_k_m')
        probes=[];next_probe=start+2;first_saved=None
        while True:
            count=db.execute('SELECT count(*) FROM knowledge_gemma_embeddings').fetchone()[0]
            if count and first_saved is None:first_saved=time.monotonic()-start
            if count==192:break
            if time.monotonic()-start>120:raise RuntimeError('durable embedding timeout')
            if time.monotonic()>=next_probe:
                probes+=workload.bridge_search(d,['runtime-fixture-00123']);next_probe=time.monotonic()+2
            time.sleep(.02)
        actual=time.monotonic()-start
        return dict(actual_complete_s=actual,first_saved_s=first_saved,rows=count,foreground_searches=probes)
    result['actual']=workload.measure(d,'durable_vectors',indexing)
    start=time.monotonic()
    d.wait(lambda:(lambda s:s['indexedChunks']==192 and s['totalChunks']==192)(d.request('knowledge:snapshot')['semanticSearch']))
    result['additional_published_count_wait_s']=time.monotonic()-start
    digest=hashlib.sha256()
    for path,offset,vector in db.execute('SELECT s.path,c.start_offset,g.vector FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id ORDER BY s.path,c.start_offset'):
        digest.update(Path(path).name.encode());digest.update(str(offset).encode());digest.update(vector)
    result['vectors_sha256']=digest.hexdigest()
    result['vector_bytes']=db.execute('SELECT sum(length(vector)) FROM knowledge_gemma_embeddings').fetchone()[0]
    result['binary_sha256']=hashlib.sha256(Path(a.binary).read_bytes()).hexdigest();result['root']=root;db.close()
finally:
    result['samples']=d.samples;Path(a.output).write_text(json.dumps(result,indent=2)+'\n');d.close()
print(json.dumps(dict(actual_complete_s=result['actual']['value']['actual_complete_s'],
    cpu_s=result['actual']['cpu_s'], rows=result['actual']['value']['rows'],
    additional_published_count_wait_s=result['additional_published_count_wait_s'],
    vector_bytes=result['vector_bytes'], vectors_sha256=result['vectors_sha256'])),flush=True)
