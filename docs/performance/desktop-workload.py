#!/usr/bin/env python3
"""Production-service workloads inside a release Tauri/WebKit app, not a headless core."""
import argparse, hashlib, json, pathlib, sqlite3, tempfile, threading, time
from desktop import Desktop,prepare,summarize,processes

def measure(d,label,fn):
    stop=threading.Event();d.sample(label)
    def sample():
        while not stop.wait(.5): d.sample(label)
    t=threading.Thread(target=sample);t.start();beg=time.monotonic()
    try: value=fn()
    finally: stop.set();t.join();d.sample(label)
    result=summarize([s for s in d.samples if s['phase']==label]);result['operation_wall_s']=time.monotonic()-beg;result['value']=value
    print(label,json.dumps({k:v for k,v in result.items() if k not in ['value']}),flush=True)
    return result

def bridge_search(d,queries):
    return d.js('''const queries='''+json.dumps(queries)+''';const samples=[];for(const query of queries){const start=performance.now();const rows=await window.tezbar.searchAll(query);samples.push({query,ms:performance.now()-start,count:rows.length,ids:rows.map(r=>r.id),scores:rows.map(r=>r.score)});}return samples;''')

def ui_search(d,query):
    # Drive React's actual input and await its actual backend result, then two
    # animation frames. This includes the existing 80/180 ms input debounce.
    return d.js('''const query='''+json.dumps(query)+''';const original=window.tezbar.searchAll;let completed;const done=new Promise(r=>completed=r);window.tezbar.searchAll=async function(q){const t=performance.now();const rows=await original(q);if(q===query)completed({ipcMs:performance.now()-t,count:rows.length});return rows;};try{const input=document.querySelector('input');if(!input)throw Error('command input absent');const start=performance.now();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,query);input.dispatchEvent(new Event('input',{bubbles:true}));const result=await Promise.race([done,new Promise((_,r)=>setTimeout(()=>r(Error('UI search did not complete')),10000))]);await new Promise(requestAnimationFrame);await new Promise(requestAnimationFrame);return {...result,inputToPaintMs:performance.now()-start};}finally{window.tezbar.searchAll=original;}''')

def image_retention(d, fixture):
    d.js(pathlib.Path(fixture).read_text()+'''return true;''')
    mounted=d.js('''
      const api=window.__tezbarImageFixture;
      const descriptor=Object.getOwnPropertyDescriptor(window,'caches');
      const create=URL.createObjectURL, revoke=URL.revokeObjectURL;
      const count=96, bytes=262144; let created=0,revoked=0,loaded=0;
      URL.createObjectURL=function(blob){created++;return create.call(URL,blob)};
      URL.revokeObjectURL=function(url){revoked++;return revoke.call(URL,url)};
      Object.defineProperty(window,'caches',{configurable:true,value:{open:async()=>({match:async(key)=>{
        const svg='<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="green"/><!--'+key;
        const blob=new Blob([svg+' '.repeat(bytes-svg.length-9)+'--></svg>'],{type:'image/svg+xml'});
        if(blob.size!==bytes)throw Error('fixture payload size changed');
        return new Response(blob);
      }})}});
      const container=document.createElement('div');container.style.cssText='position:absolute;top:0;left:0;width:512px';document.body.appendChild(container);
      const leases=Array.from({length:count},(_,i)=>{
        const src='https://fixture.invalid/retention-'+i+'.svg';
        return api.acquireExtensionImage?api.acquireExtensionImage(src,'icon'):{source:api.cachedExtensionImageSource(src,'icon'),release:()=>{}};
      });
      try {
        await Promise.all(leases.map(async lease=>{
          const img=document.createElement('img');img.width=64;img.height=64;
          const done=new Promise((resolve,reject)=>{img.onload=()=>{loaded++;resolve()};img.onerror=()=>reject(Error('fixture image decode failed'))});
          img.src=await lease.source;container.appendChild(img);await done;
        }));
      } catch(error) {container.remove();for(const lease of leases)lease.release();throw error;}
      window.__releaseImageFixture=()=>{
        container.remove();for(const lease of leases)lease.release();leases.length=0;
        if(descriptor)Object.defineProperty(window,'caches',descriptor);else delete window.caches;
        URL.createObjectURL=create;URL.revokeObjectURL=revoke;
        return {created,revoked,unrevoked:created-revoked};
      };
      return {count,bytesPerImage:bytes,created,loaded,unrevoked:created-revoked};
    ''')
    held=d.idle('image_mounted',3)
    released=d.js('return window.__releaseImageFixture();')
    settled=d.idle('image_released',30)
    return dict(mounted=mounted,held=held,released=released,settled=settled)

def scenario(d,model,fixture=None):
    out={}
    d.wait(lambda:d.request('core:launcher:status')['status']=='ready',120)
    d.request('knowledge:snapshot')
    d.wait(lambda:d.request('knowledge:snapshot')['status']['state'] not in ['scanning','indexing'],120)
    # Pass the 8-second initial Knowledge reconciliation for both builds.
    if time.monotonic()-d.started<12: time.sleep(12-(time.monotonic()-d.started))
    d.command('hide');out['hidden_idle']=d.idle('hidden_idle',30)
    d.command('show');out['visible_idle']=d.idle('visible_idle',30)
    out['bridge_search']=measure(d,'bridge_search',lambda:bridge_search(d,['runtime-fixture-00123','runtime-fixture','rutnime','no-such-runtime-fixture']*10))
    def cycles():
        samples=[]
        for i in range(12):
            d.command('show')
            samples.append(ui_search(d,f'runtime-fixture-{i:05}'))
            d.command('hide');time.sleep(.12)
        return samples
    out['search_open_close']=measure(d,'search_open_close',cycles)
    d.command('show')
    d.request('knowledge:speed:set','fast')
    d.request('knowledge:depth:set','smart')
    d.request('knowledge:root:add',str(d.root/'knowledge-docs'))
    def indexing():
        start=time.monotonic();d.request('knowledge:index:start');latency=[]
        while True:
            latency += bridge_search(d,['runtime-fixture-00123'])
            snap=d.request('knowledge:snapshot')
            if snap['status']['state']=='failed':raise RuntimeError(snap['status'].get('error'))
            if snap['status']['state']=='completed' and snap['status'].get('processedSources',0)>=96:break
            if time.monotonic()-start>180:raise RuntimeError('indexing timeout')
            # Production indexing yields for 1.2s after foreground search. A
            # continuous 100ms flood would intentionally starve this workload.
            time.sleep(1.5)
        return dict(count=snap['status']['processedSources'],searches=latency)
    out['index_with_search']=measure(d,'index_with_search',indexing)
    out['deep_lexical']=measure(d,'deep_lexical',lambda:bridge_search(d,['!orchards apples pears']*10))
    assert all(s['count']>0 for s in out['deep_lexical']['value'])
    def features():
        latencies=[]
        for surface,selector in [('notes','[aria-label="Quick Notes"]'),('snippets','[aria-label="Snippets"]'),('indexing','[aria-label="Indexing Status"]'),('extensions','[aria-label="Extensions"]')]*2:
            start=time.monotonic();d.command('surface',surface=surface)
            d.wait(lambda:d.js('return !!document.querySelector('+json.dumps(selector)+')'),20)
            latencies.append(dict(surface=surface,ready_ms=(time.monotonic()-start)*1000))
            d.command('surface',surface='command')
        return latencies
    out['feature_visits']=measure(d,'feature_visits',features)
    d.command('hide');out['post_features']=d.idle('post_features',30)
    if model:
        def semantic():
            start=time.monotonic();d.request('knowledge:embedding:set','embeddinggemma-2','q4_k_m')
            d.wait(lambda:d.request('knowledge:snapshot')['semanticSearch']['runtimeState']=='ready',120)
            ready_ms=(time.monotonic()-start)*1000
            # Inference and the real learned-index queue run to completion.
            probes=[];last_probe=time.monotonic();deadline=time.monotonic()+240
            while True:
                snap=d.request('knowledge:snapshot')['semanticSearch']
                if snap.get('indexedChunks',0)>0 and snap.get('indexedChunks')==snap.get('totalChunks') and not snap.get('indexing'):break
                if time.monotonic()>deadline:raise RuntimeError('semantic indexing timeout')
                if time.monotonic()-last_probe>=2:
                    probes+=bridge_search(d,['runtime-fixture-00123']);last_probe=time.monotonic()
                time.sleep(.2)
            snap=d.request('knowledge:snapshot')['semanticSearch']
            return dict(runtime_ready_ms=ready_ms,indexedChunks=snap['indexedChunks'],totalChunks=snap['totalChunks'],foreground_searches=probes)
        out['semantic_index']=measure(d,'semantic_index',semantic)
        # Exercise an actual cold owned model and cold search cache using the
        # ordinary pause/resume lifecycle, with the learned index preserved.
        # No inference or timer is interrupted: the completion milestone above
        # has already been reached, and the first foreground query is awaited.
        d.request('knowledge:index:pause')
        assert d.request('knowledge:snapshot')['semanticSearch']['runtimeState']=='stopped'
        d.request('knowledge:index:resume')
        d.command('show')
        out['before_cold_query']=d.request('knowledge:snapshot')['semanticSearch']['runtimeState']
        out['deep_model_first']=measure(d,'deep_model_first',lambda:bridge_search(d,['!orchards apples pears']))
        out['after_cold_query']=d.request('knowledge:snapshot')['semanticSearch']['runtimeState']
        def cold_ready():
            d.wait(lambda:(lambda s:s['semanticSearch']['runtimeState']=='ready' and not s['semanticSearch'].get('indexing') and s['status']['state']=='completed')(d.request('knowledge:snapshot')),120)
            return True
        out['cold_to_ready']=measure(d,'cold_to_ready',cold_ready)
        out['deep_model_warm']=measure(d,'deep_model_warm',lambda:bridge_search(d,['!orchards apples pears']*10))
        assert all(s['count']>0 for s in out['deep_model_warm']['value'])
        out['after_warm_query']=d.request('knowledge:snapshot')['semanticSearch']['runtimeState']
        d.command('hide');out['active_model_idle']=d.idle('active_model_idle',30)
    if fixture:
        out['image_retention']=image_retention(d,fixture)
        if model:out['model_after_images']=d.request('knowledge:snapshot')['semanticSearch']['runtimeState']
    return out

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--output',required=True);p.add_argument('--root');p.add_argument('--model');p.add_argument('--image-fixture');args=p.parse_args()
    root=args.root or tempfile.mkdtemp(prefix='tezbar-desktop-');prepare(root,args.model);d=Desktop(root)
    result=dict(root=root,source_revision='354689e + desktop-bench harness (baseline)' if 'baseline' in args.binary else 'runtime candidate',binary_sha256=hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest())
    try:
        d.launch(pathlib.Path(args.binary).resolve());print('launch',d.pid,d.ready_ms,d.first_ui_ms,flush=True)
        # Keep isolated WebKit localStorage from previous test apps out of the
        # measured workload. The initial UI milestone is reported separately.
        d.js('window.localStorage.clear(); return true;')
        result.update(ready_ms=d.ready_ms,first_ui_ms=d.first_ui_ms,metrics=scenario(d,args.model,args.image_fixture))
        with sqlite3.connect('file:'+str(d.root/'data/search/index.sqlite3')+'?mode=ro',uri=True) as db:
            result['launcher_counts']=dict(db.execute('SELECT category,count(*) FROM documents GROUP BY category'))
        with sqlite3.connect('file:'+str(d.root/'data/knowledge/knowledge.sqlite3')+'?mode=ro',uri=True) as db:
            result['knowledge_counts']={table:db.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ['knowledge_sources','knowledge_chunks']}
    finally:
        result['samples']=d.samples
        pathlib.Path(args.output).write_text(json.dumps(result,indent=2)+'\n')
        d.close()
