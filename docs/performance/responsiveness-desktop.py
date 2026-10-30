#!/usr/bin/env python3
"""Input scheduling A/B: actual React input, fresh native replies, DOM + two frames.
Uses the existing isolated release desktop/profile/accounting harness. Never
writes the clipboard, replaces an installed app, or changes the real profile.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sqlite3
import tempfile
import time
from desktop import Desktop, prepare, summarize
from window_visibility import main_on_screen, owned_on_screen_bounds

spec = importlib.util.spec_from_file_location('final_desktop', Path(__file__).with_name('final-desktop.py'))
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)

# Counters are wrappers around the real bridge, not replacements for its work.
INSTALL = r'''
const original = window.tezbar.searchAll;
const state = {calls:[], latest:null, active:0, maxActive:0, deepMode:false, trustedInputEvents:0};
// Count physical input interference without retaining its text or key values.
for(const kind of ['input','keydown','compositionstart','compositionend']) {
  document.addEventListener(kind,event=>{if(event.isTrusted)state.trustedInputEvents++;},true);
}
window.__inputBench = state;
state.allowed = new Set(['']);
state.original = original;
state.input = (text, query) => {
  state.latest = query;
  state.allowed.add(query);
  const input = document.querySelector('#command-input');
  if (!input) throw Error('command input absent');
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,text);
  input.dispatchEvent(new Event('input',{bubbles:true}));
};
window.tezbar.searchAll = async function(query) {
  const call = {query:state.allowed.has(query)?query:'<UNEXPECTED_INPUT>', start:performance.now()};
  state.calls.push(call); state.active++;
  state.maxActive = Math.max(state.maxActive,state.active);
  try {
    const rows = await original(query);
    call.end = performance.now(); call.count = rows.length;
    call.obsolete = query !== state.latest;
    // Keep full ordered scores/actions for final-reply parity, outside latency.
    call.rows = rows;
    return rows;
  } catch(error) {
    call.end = performance.now(); call.error = String(error);
    throw error;
  } finally {state.active--;}
};
return true;
'''


def burst(d, queries, gap_ms, deep=False):
    """Fresh final reply required; cached/optimistic DOM alone never completes it."""
    return d.js(r'''
const queries = ''' + json.dumps(queries) + r''';
const gap = ''' + json.dumps(gap_ms) + r''';
const deep = ''' + json.dumps(deep) + r''';
const state = window.__inputBench;
const offset = state.calls.length;
const trustedStart = state.trustedInputEvents;
for (const call of state.calls) delete call.rows;
const target = queries.at(-1);
const start = performance.now(); let lastInput;
// A deep mode input omits its hidden ! sentinel after the first input.
for (let i=0;i<queries.length;i++) {
  const query = queries[i];
  const text = deep && state.deepMode ? query.slice(1) : query;
  lastInput = performance.now(); state.input(text,query);
  state.deepMode = deep;
  if (i+1<queries.length) await new Promise(r=>setTimeout(r,gap));
}
const deadline = performance.now()+15000;
let final;
while (!(final=state.calls.slice(offset).find(c=>c.query===target && c.end && !c.error))) {
  if(performance.now()>deadline) throw Error('final input did not complete: '+target+'; trustedInputEvents='+(state.trustedInputEvents-trustedStart)+'; dispatched='+state.calls.slice(offset).map(c=>c.query).join(','));
  await new Promise(r=>setTimeout(r,1));
}
// Same response + two animation frames proxy as the preserved ui_search helper.
await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame);
const twoFrames = performance.now();
const titles = final.rows.map(r=>r.title);
const domMatches = () => {
  const actual = Array.from(document.querySelectorAll('button.list-row[data-tone="accent"]'));
  // Renderer may prepend the unchanged Deep Search recommendation. Backend rows
  // must occur in exact order with titles present; no invocation/open side effect.
  let cursor=0;
  for(const title of titles) {
    while(cursor<actual.length && !actual[cursor].textContent.includes(title)) cursor++;
    if(cursor===actual.length) return false;
    cursor++;
  }
  return titles.length>0;
};
while(!domMatches()) {
  if(performance.now()>deadline) throw Error('fresh final rows did not commit: '+JSON.stringify({target,titles,input:document.querySelector('#command-input')?.value,hidden:document.hidden,hiddenClass:document.documentElement.classList.contains('tezbar-window-hidden'),actual:Array.from(document.querySelectorAll('button.list-row[data-tone="accent"]')).map(e=>e.textContent)}));
  await new Promise(requestAnimationFrame);
}
const domObserved = performance.now();
// Allow all real superseded calls to settle before taking work counters.
while(state.active) {
  if(performance.now()>deadline) throw Error('superseded work did not settle');
  await new Promise(r=>setTimeout(r,1));
}
const calls = state.calls.slice(offset);
if(state.trustedInputEvents!==trustedStart) throw Error('Physical input interfered with synthetic workload; no input text/keys captured');
const result = {
  trustedInputEvents:state.trustedInputEvents-trustedStart,
  inputs:queries.length, gapMs:gap, query:target,
  inputToTwoFramesMs:twoFrames-lastInput,
  inputToCorrectDomObservedMs:domObserved-lastInput,
  responseMs:final.end-lastInput, ipcMs:final.end-final.start,
  burstWallMs:performance.now()-start,
  dispatched:calls.length, completed:calls.filter(c=>c.end&&!c.error).length,
  obsoleteCompleted:calls.filter(c=>c.obsolete).length,
  errors:calls.filter(c=>c.error).map(c=>({query:c.query,error:c.error})),
  calls:calls.map(({rows,...c})=>c),
  finalRows:final.rows,
  domTitles:titles,
};
// Do not turn the measurement wrapper into a retained-results cache.
for (const call of calls) delete call.rows;
return result;
''')


def reset(d):
    # Real UI clear + Backspace exits deep mode; hide/reopen then verifies the
    # ordinary reset milestone instead of replacing/reloading the WebView.
    d.js('''const s=window.__inputBench; s.input('',''); await new Promise(r=>setTimeout(r,20));
      document.querySelector('#command-input').dispatchEvent(new KeyboardEvent('keydown',{key:'Backspace',bubbles:true})); s.deepMode=false; return true;''')
    d.command('hide')
    d.wait(lambda: not main_on_screen(d.pid), 5)
    d.wait(lambda: d.js("return document.querySelector('#command-input')?.value==='' && document.documentElement.classList.contains('tezbar-window-hidden');"), 5)
    f.visible(d)


def measured_bursts(d, label, deep=False, repetitions=4):
    def run():
        values=[]
        for i in range(repetitions):
            if deep:
                queries=['!orch','!orchards','!orchards apples','!orchards apples pears']
            else:
                queries=['ru','run','runt','runtime','runtime-fixture','runtime-fixture-00',f'runtime-fixture-{100+i:05}']
            values.append(burst(d,queries,2 if i%2==0 else 20,deep))
        return values
    return f.w.measure(d,label,run)


def scenario(d, out):
    d.wait(lambda:d.request('core:launcher:status')['status']=='ready')
    d.request('knowledge:snapshot')
    if time.monotonic()-d.started<12:time.sleep(12-(time.monotonic()-d.started))
    d.command('hide');d.idle('settle_hidden',2)
    out['hidden_idle']=f.idle(d,'hidden_idle',False)
    f.visible(d);d.js(INSTALL)
    # Preserve the exact earlier 12 hide/show/input/response+two-frame metric.
    def cycles():
        values=[]
        for i in range(12):
            d.command('hide');d.wait(lambda:not main_on_screen(d.pid),5)
            d.wait(lambda:d.js("return document.querySelector('input')?.value === '' && document.documentElement.classList.contains('tezbar-window-hidden');"),5)
            f.visible(d)
            values.append(f.w.ui_search(d,f'runtime-fixture-{i:05}'))
            time.sleep(.12)
        return values
    out['search_open_close']=f.w.measure(d,'search_open_close',cycles)
    reset(d)
    out['simple_bursts']=measured_bursts(d,'simple_bursts',repetitions=8)
    d.command('hide');d.idle('settle_search',2)
    out['post_search_idle']=f.idle(d,'post_search_idle',False)
    f.visible(d)
    d.request('knowledge:speed:set','fast');d.request('knowledge:depth:set','smart')
    d.request('knowledge:root:add',str(d.root/'knowledge-docs'))
    def source():
        start=time.monotonic();d.request('knowledge:index:start');probes=[]
        while True:
            probes.append(burst(d,['runtime-fixture','runtime-fixture-00','runtime-fixture-00123'],20))
            s=d.request('knowledge:snapshot')['status']
            if s['state']=='failed':raise RuntimeError('source indexing failed')
            if s['state']=='completed' and s['processedSources']>=96:break
            if time.monotonic()-start>180:raise RuntimeError('source indexing timeout')
            time.sleep(1.5)
        return dict(processed=s['processedSources'],bursts=probes)
    out['source_index_with_bursts']=f.w.measure(d,'source_index_with_bursts',source)
    reset(d)
    out['deep_lexical_bursts']=measured_bursts(d,'deep_lexical_bursts',True)
    reset(d)
    db=sqlite3.connect('file:'+str(d.root/'data/knowledge/knowledge.sqlite3')+'?mode=ro',uri=True)
    assert db.execute('SELECT count(*) FROM knowledge_chunks').fetchone()[0]==192
    def semantic():
        start=time.monotonic();d.request('knowledge:embedding:set','embeddinggemma-2','q4_k_m')
        next_probe=time.monotonic()+2;probes=[];actual=None;durable=None
        while True:
            count=db.execute('SELECT count(*) FROM knowledge_gemma_embeddings').fetchone()[0]
            if count==192 and actual is None:
                actual=time.monotonic()-start
                d.sample('semantic_index_with_bursts')
                durable=summarize([v for v in d.samples if v['phase']=='semantic_index_with_bursts'])
            s=d.request('knowledge:snapshot')['semanticSearch']
            if actual is not None and s['indexedChunks']==192 and s['totalChunks']==192 and not s['indexing']:break
            if time.monotonic()-start>240:raise RuntimeError('semantic indexing timeout')
            if time.monotonic()>=next_probe:
                probes.append(burst(d,['runtime-fixture','runtime-fixture-00','runtime-fixture-00123'],20))
                next_probe=time.monotonic()+2
            time.sleep(.02)
        return dict(actual_complete_s=actual,durable_family=durable,published_complete_s=time.monotonic()-start,bursts=probes)
    out['semantic_index_with_bursts']=f.w.measure(d,'semantic_index_with_bursts',semantic)
    d.command('hide');d.wait(lambda:not main_on_screen(d.pid),5)
    out['active_model_idle']=f.idle(d,'active_model_idle',False)
    assert d.request('knowledge:snapshot')['semanticSearch']['runtimeState']=='ready'
    f.visible(d)
    out['deep_model_bursts']=measured_bursts(d,'deep_model_bursts',True,8)
    # Repeat useful work to expose retained family growth without forcing GC,
    # model unloading, page recreation, or shrinking any production cache.
    out['repeat_deep_model_bursts']=measured_bursts(d,'repeat_deep_model_bursts',True,16)
    d.command('hide');d.wait(lambda:not main_on_screen(d.pid),5)
    out['post_repeat_idle']=f.idle(d,'post_repeat_idle',False)
    assert d.request('knowledge:snapshot')['semanticSearch']['runtimeState']=='ready'
    digest=hashlib.sha256()
    for path,offset,vector in db.execute('SELECT s.path,c.start_offset,g.vector FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id=g.chunk_id JOIN knowledge_sources s ON s.id=c.source_id ORDER BY s.path,c.start_offset'):
        digest.update(Path(path).name.encode());digest.update(str(offset).encode());digest.update(vector)
    out['vectors_sha256']=digest.hexdigest();db.close()
    out['bridge_totals']=d.js('''const s=window.__inputBench; return {dispatched:s.calls.length,completed:s.calls.filter(c=>c.end&&!c.error).length,errors:s.calls.filter(c=>c.error).map(c=>c.error),maxActive:s.maxActive,active:s.active};''')


def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--model',required=True);p.add_argument('--output',required=True)
    a=p.parse_args();output=Path(a.output)
    if output.exists():raise ValueError('refusing to overwrite measurement evidence')
    root=tempfile.mkdtemp(prefix='tezbar-input-');prepare(root,a.model);d=Desktop(root)
    original_sample=d.sample
    def sample(label):
        s=original_sample(label);s['owned_on_screen_bounds']=owned_on_screen_bounds(d.pid)
        s['main_on_screen']=main_on_screen(d.pid);return s
    d.sample=sample
    result=dict(protocol='bounded-input-durable-completion-v1',root=root,binary=str(Path(a.binary).resolve()),binary_sha256=hashlib.sha256(Path(a.binary).read_bytes()).hexdigest(),metrics={},completed=False)
    try:
        d.launch(Path(a.binary).resolve());d.js('window.localStorage.clear(); return true;')
        result.update(ready_ms=d.ready_ms,first_ui_ms=d.first_ui_ms)
        print('launch',d.pid,d.ready_ms,d.first_ui_ms,flush=True)
        scenario(d,result['metrics']);result['completed']=True
    finally:
        result['samples']=d.samples;output.write_text(json.dumps(result,indent=2)+'\n')
        if d.pid:
            try:d.js("await window.__TAURI_INTERNALS__.invoke('set_suppress_blur_hide',{value:false}); return true;")
            finally:d.close()

if __name__=='__main__': main()
