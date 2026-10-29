#!/usr/bin/env python3
"""Final desktop protocol: native visibility, durable inference and image ownership.
Uses production UI/services, read-only observation and a separate test identity.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sqlite3
import tempfile
import time
from desktop import Desktop, prepare
from window_visibility import main_on_screen, owned_on_screen_bounds
spec=importlib.util.spec_from_file_location('workload',Path(__file__).with_name('desktop-workload.py'))
w=importlib.util.module_from_spec(spec);spec.loader.exec_module(w)

def visible(d):
    d.command('show')
    # Test-window modal/drag guard only; avoids a focus change silently turning
    # a visible-idle sample into hidden idle. It is restored on exit.
    d.js("await window.__TAURI_INTERNALS__.invoke('set_suppress_blur_hide',{value:true}); return true;")
    d.wait(lambda:main_on_screen(d.pid),5)
    d.js("await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame); return true;")

def idle(d,label,on_screen):
    result=d.idle(label,30)
    assert all(s['main_on_screen']==on_screen for s in d.samples if s['phase']==label),label+' visibility changed'
    return result

def run(d,out,image_fixture):
    d.wait(lambda:d.request('core:launcher:status')['status']=='ready')
    d.request('knowledge:snapshot')
    if time.monotonic()-d.started<12:time.sleep(12-(time.monotonic()-d.started))
    d.command('hide');d.idle('settle_hidden',2)
    out['hidden_idle']=idle(d,'hidden_idle',False)
    visible(d);d.idle('settle_visible',2)
    out['visible_idle']=idle(d,'visible_idle',True)
    out['bridge_search']=w.measure(d,'bridge_search',lambda:w.bridge_search(d,['runtime-fixture-00123','runtime-fixture','rutnime','no-such-runtime-fixture']*10))
    def cycles():
        values=[]
        for i in range(12):
            d.command('hide');d.wait(lambda:not main_on_screen(d.pid),5)
            # Await the real close/reset before the next native show/input.
            d.wait(lambda:d.js("return document.querySelector('input')?.value === '' && document.documentElement.classList.contains('tezbar-window-hidden');"),5)
            visible(d)
            values.append(w.ui_search(d,f'runtime-fixture-{i:05}'))
            time.sleep(.12)
        return values
    out['search_open_close']=w.measure(d,'search_open_close',cycles)
    d.request('knowledge:speed:set','fast');d.request('knowledge:depth:set','smart');d.request('knowledge:root:add',str(d.root/'knowledge-docs'))
    def source_index():
        start=time.monotonic();d.request('knowledge:index:start');searches=[]
        while True:
            searches+=w.bridge_search(d,['runtime-fixture-00123'])
            snapshot=d.request('knowledge:snapshot')
            if snapshot['status']['state']=='failed':raise RuntimeError('source indexing failed')
            if snapshot['status']['state']=='completed' and snapshot['status']['processedSources']>=96:break
            if time.monotonic()-start>120:raise RuntimeError('source indexing timeout')
            time.sleep(1.5)
        return dict(count=snapshot['status']['processedSources'],searches=searches)
    out['index_with_search']=w.measure(d,'index_with_search',source_index)
    out['deep_lexical']=w.measure(d,'deep_lexical',lambda:w.bridge_search(d,['!orchards apples pears']*10))
    assert all(v['count']>0 for v in out['deep_lexical']['value'])
    def features():
        values=[]
        for surface,selector in [('notes','[aria-label="Quick Notes"]'),('snippets','[aria-label="Snippets"]'),('indexing','[aria-label="Indexing Status"]'),('extensions','[aria-label="Extensions"]')]*2:
            start=time.monotonic();d.command('surface',surface=surface)
            d.wait(lambda:d.js('return !!document.querySelector('+json.dumps(selector)+')'),20)
            values.append(dict(surface=surface,ready_ms=(time.monotonic()-start)*1000))
            d.command('surface',surface='command')
        return values
    out['feature_visits']=w.measure(d,'feature_visits',features)
    d.command('hide');d.idle('settle_features',2)
    out['post_features']=idle(d,'post_features',False)
    db=sqlite3.connect('file:'+str(d.root/'data/knowledge/knowledge.sqlite3')+'?mode=ro',uri=True)
    assert db.execute('SELECT count(*) FROM knowledge_chunks').fetchone()[0]==192
    def semantic():
        start=time.monotonic();d.request('knowledge:embedding:set','embeddinggemma-2','q4_k_m')
        probes=[];next_probe=time.monotonic()+2;actual=None;first_saved=None
        while True:
            count=db.execute('SELECT count(*) FROM knowledge_gemma_embeddings').fetchone()[0]
            if count and first_saved is None:first_saved=time.monotonic()-start
            if count==192 and actual is None:actual=time.monotonic()-start
            snap=d.request('knowledge:snapshot')['semanticSearch']
            if actual is not None and snap['indexedChunks']==192 and snap['totalChunks']==192 and not snap['indexing']:break
            if time.monotonic()-start>180:raise RuntimeError('semantic indexing timeout')
            if time.monotonic()>=next_probe:
                probes+=w.bridge_search(d,['runtime-fixture-00123']);next_probe=time.monotonic()+2
            time.sleep(.02)
        return dict(actual_complete_s=actual,first_saved_s=first_saved,published_complete_s=time.monotonic()-start,
                    indexedChunks=snap['indexedChunks'],totalChunks=snap['totalChunks'],foreground_searches=probes)
    out['semantic_index']=w.measure(d,'semantic_index',semantic)
    out['active_model_idle']=idle(d,'active_model_idle',False)
    out['model_idle_scope']='After bulk inference, without model restart/unload'
    out['image_retention']=w.image_retention(d,image_fixture)
    out['model_after_images']=d.request('knowledge:snapshot')['semanticSearch']['runtimeState']
    assert out['model_after_images']=='ready'
    # Cold query timing is explicitly separate from the retained-RAM windows.
    d.request('knowledge:index:pause')
    assert d.request('knowledge:snapshot')['semanticSearch']['runtimeState']=='stopped'
    d.request('knowledge:index:resume');visible(d)
    out['before_cold_query']=d.request('knowledge:snapshot')['semanticSearch']['runtimeState']
    out['deep_model_first']=w.measure(d,'deep_model_first',lambda:w.bridge_search(d,['!orchards apples pears']))
    out['after_cold_query']=d.request('knowledge:snapshot')['semanticSearch']['runtimeState']
    out['cold_to_ready']=w.measure(d,'cold_to_ready',lambda:d.wait(lambda:(lambda s:s['semanticSearch']['runtimeState']=='ready' and not s['semanticSearch']['indexing'] and s['status']['state']=='completed')(d.request('knowledge:snapshot')),120))
    out['deep_model_warm']=w.measure(d,'deep_model_warm',lambda:w.bridge_search(d,['!orchards apples pears']*10))
    assert all(v['count']>0 for v in out['deep_model_warm']['value'])
    out['after_warm_query']=d.request('knowledge:snapshot')['semanticSearch']['runtimeState']
    digest=hashlib.sha256()
    for path,offset,vector in db.execute('SELECT s.path,c.start_offset,g.vector FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id ORDER BY s.path,c.start_offset'):
        digest.update(Path(path).name.encode());digest.update(str(offset).encode());digest.update(vector)
    out['vectors_sha256']=digest.hexdigest();out['vector_bytes']=db.execute('SELECT sum(length(vector)) FROM knowledge_gemma_embeddings').fetchone()[0]
    db.close()

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--model',required=True);p.add_argument('--image-fixture',required=True);p.add_argument('--output',required=True);a=p.parse_args()
    root=tempfile.mkdtemp(prefix='tezbar-desktop-');prepare(root,a.model);d=Desktop(root)
    original_sample=d.sample
    def sample(label):
        value=original_sample(label)
        value['owned_on_screen_bounds']=owned_on_screen_bounds(d.pid)
        value['main_on_screen']=any(w['width']>=480 and w['height']>=80 for w in value['owned_on_screen_bounds'])
        if label=='visible_idle' and not value['main_on_screen']:
            value['unexpected_visibility_state']=d.js("return {hidden:document.hidden,hiddenClass:document.documentElement.classList.contains('tezbar-window-hidden'),width:innerWidth,height:innerHeight,focused:document.hasFocus()};")
        return value
    d.sample=sample
    result=dict(protocol='visibility-and-durable-completion-v2',root=root,binary_sha256=hashlib.sha256(Path(a.binary).read_bytes()).hexdigest(),metrics={},completed=False)
    try:
        d.launch(Path(a.binary).resolve());d.js('window.localStorage.clear(); return true;')
        result.update(ready_ms=d.ready_ms,first_ui_ms=d.first_ui_ms)
        print('launch',d.pid,d.ready_ms,d.first_ui_ms,flush=True)
        run(d,result['metrics'],a.image_fixture)
        with sqlite3.connect('file:'+str(d.root/'data/search/index.sqlite3')+'?mode=ro',uri=True) as db:
            result['launcher_counts']=dict(db.execute('SELECT category,count(*) FROM documents GROUP BY category'))
        with sqlite3.connect('file:'+str(d.root/'data/knowledge/knowledge.sqlite3')+'?mode=ro',uri=True) as db:
            result['knowledge_counts']={table:db.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ['knowledge_sources','knowledge_chunks']}
        result['completed']=True
    finally:
        result['samples']=d.samples;Path(a.output).write_text(json.dumps(result,indent=2)+'\n')
        if d.pid:
            try:d.js("await window.__TAURI_INTERNALS__.invoke('set_suppress_blur_hide',{value:false}); return true;")
            finally:d.close()
